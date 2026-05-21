use anyhow::{anyhow, Result};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use super::backend::ScanBackendKind;
use super::file_info::{DirectoryNode, FileInfo, ScanResult};
use super::incremental;
use super::path_utils::{normalized_path_key, normalized_path_key_str, normalized_path_starts_with};
use super::progress::ScanProgress;
use super::timing::StageTimer;
use crate::winfs::{self, MftEntry};

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

const LARGE_FILE_THRESHOLD: u64 = 100 * 1024 * 1024;
const DIRECTORY_CHUNK_SIZE: usize = 512;
const POST_SCAN_RESCAN_LIMIT: usize = 2048;
const MAX_POST_SCAN_RETRIES: usize = 1;
const PROGRESS_INTERVAL_MS: u64 = 300;

const STAGE_INIT: usize = 0;
const STAGE_ENUM_MFT: usize = 1;
const STAGE_COLLECT_TREE: usize = 2;
const STAGE_RESOLVE_PATHS: usize = 3;
const STAGE_HYDRATE: usize = 4;
const STAGE_AGGREGATE: usize = 5;

pub type ProgressCallback = Arc<dyn Fn(ScanProgress) + Send + Sync>;

struct ProgressGuard {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

struct DirectoryBatch {
    path: PathBuf,
    child_ids: Vec<u64>,
}

struct DescendantPlan {
    entry_count: usize,
    path_cache: HashMap<u64, PathBuf>,
    directory_batches: Vec<DirectoryBatch>,
}

struct ScanPassResult {
    result: ScanResult,
    start_checkpoint: Option<winfs::UsnJournalCheckpoint>,
    end_checkpoint: Option<winfs::UsnJournalCheckpoint>,
}

enum PostScanDecision {
    Complete(ScanResult),
    Retry(String, ScanResult),
}

impl ProgressGuard {
    fn new(stop: Arc<AtomicBool>, handle: Option<std::thread::JoinHandle<()>>) -> Self {
        Self { stop, handle }
    }
}

impl Drop for ProgressGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(windows)]
fn get_disk_used_bytes(path: &Path) -> Option<u64> {
    let path_str = path.to_string_lossy().to_string();
    let mut wide_path: Vec<u16> = path_str.encode_utf16().collect();
    wide_path.push(0);

    let mut free_bytes_available = 0u64;
    let mut total_bytes = 0u64;
    let mut total_free_bytes = 0u64;

    unsafe {
        if GetDiskFreeSpaceExW(
            PCWSTR(wide_path.as_ptr()),
            Some(&mut free_bytes_available),
            Some(&mut total_bytes),
            Some(&mut total_free_bytes),
        )
        .is_ok()
        {
            Some(total_bytes.saturating_sub(total_free_bytes))
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
fn get_disk_used_bytes(_path: &Path) -> Option<u64> {
    None
}

fn note_inaccessible_path(chunk_state: &mut ChunkScanState, path: &Path) {
    if chunk_state.inaccessible_samples.len() < 5 {
        chunk_state
            .inaccessible_samples
            .push(path.to_string_lossy().to_string());
    }
}

fn metadata_from_path(path: &Path) -> std::io::Result<winfs::FileIdMetadata> {
    let metadata = std::fs::symlink_metadata(path)?;
    let modified_time = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());

    Ok(winfs::FileIdMetadata {
        size: metadata.len(),
        modified_time,
        is_readonly: metadata.permissions().readonly(),
    })
}

#[derive(Default)]
struct ChunkScanState {
    dir_nodes: HashMap<PathBuf, DirectoryNode>,
    dir_file_stats: HashMap<PathBuf, (u64, usize)>,
    large_files: Vec<FileInfo>,
    inaccessible_count: usize,
    metadata_fallback_count: usize,
    metadata_fallback_bytes: u64,
    inaccessible_samples: Vec<String>,
    scanned_files: usize,
    scanned_dirs: usize,
    total_size: u64,
}

impl ChunkScanState {
    fn merge(&mut self, other: Self) {
        self.dir_nodes.extend(other.dir_nodes);
        for (path, (size, count)) in other.dir_file_stats {
            let entry = self.dir_file_stats.entry(path).or_insert((0, 0));
            entry.0 += size;
            entry.1 += count;
        }
        self.large_files.extend(other.large_files);
        self.inaccessible_count += other.inaccessible_count;
        self.metadata_fallback_count += other.metadata_fallback_count;
        self.metadata_fallback_bytes += other.metadata_fallback_bytes;
        while self.inaccessible_samples.len() < 5 {
            let Some(sample) = other
                .inaccessible_samples
                .get(self.inaccessible_samples.len())
            else {
                break;
            };
            self.inaccessible_samples.push(sample.clone());
        }
        self.scanned_files += other.scanned_files;
        self.scanned_dirs += other.scanned_dirs;
        self.total_size += other.total_size;
    }
}

pub fn scan_path(
    path: &Path,
    progress: Option<ProgressCallback>,
    estimated_files: usize,
    cancelled: Arc<AtomicBool>,
) -> Result<Option<ScanResult>> {
    let total_timer = StageTimer::start("mft-usn", format!("scan_path path={}", path.display()));
    let support_timer = StageTimer::start(
        "mft-usn",
        format!("supports_mft_scan_check path={}", path.display()),
    );
    let supported = winfs::supports_mft_scan(path);
    support_timer.finish_with(format!("supported={supported}"));
    if !supported {
        total_timer.finish_with("status=unsupported");
        return Ok(None);
    }

    let privilege_timer = StageTimer::start(
        "mft-usn",
        format!("enable_scan_privileges path={}", path.display()),
    );
    let enabled_privileges = winfs::enable_best_effort_scan_privileges();
    privilege_timer.finish_with(format!(
        "count={} names={}",
        enabled_privileges.len(),
        if enabled_privileges.is_empty() {
            "none".to_string()
        } else {
            enabled_privileges.join(",")
        }
    ));
    if !enabled_privileges.is_empty() {
        tracing::info!(
            "[mft-usn] enabled privileges: {}",
            enabled_privileges.join(", ")
        );
    }

    let file_id_timer = StageTimer::start(
        "mft-usn",
        format!("query_root_file_id path={}", path.display()),
    );
    let root_file_id = match winfs::get_path_file_id(path) {
        Some(file_id) => {
            file_id_timer.finish_with(format!("found=true file_id={file_id}"));
            file_id
        }
        None => {
            file_id_timer.finish_with("found=false");
            total_timer.finish_with("status=no_root_file_id");
            return Ok(None);
        }
    };
    let volume_timer = StageTimer::start(
        "mft-usn",
        format!("query_volume_details path={}", path.display()),
    );
    let volume = match winfs::query_volume_details(path) {
        Some(volume) => {
            volume_timer.finish_with(format!(
                "found=true volume_root={} file_system={}",
                volume.volume_root.display(),
                volume.file_system
            ));
            volume
        }
        None => {
            volume_timer.finish_with("found=false");
            total_timer.finish_with("status=no_volume_details");
            return Ok(None);
        }
    };

    for attempt in 0..=MAX_POST_SCAN_RETRIES {
        let attempt_timer = StageTimer::start(
            "mft-usn",
            format!("scan_attempt index={} path={}", attempt + 1, path.display()),
        );
        let Some(pass) = scan_path_once(
            path,
            &volume,
            root_file_id,
            progress.clone(),
            estimated_files,
            Arc::clone(&cancelled),
        )?
        else {
            attempt_timer.finish_with("status=unsupported_or_cancelled_before_pass");
            total_timer.finish_with("status=unsupported_or_cancelled_before_pass");
            return Ok(None);
        };

        let reconcile_timer = StageTimer::start(
            "mft-usn",
            format!(
                "reconcile_post_scan attempt={} path={}",
                attempt + 1,
                path.display()
            ),
        );
        match reconcile_post_scan_window(
            path,
            pass.result,
            pass.start_checkpoint,
            pass.end_checkpoint,
        ) {
            PostScanDecision::Complete(result) => {
                let detail = format!(
                    "status=complete files={} dirs={} size={} inaccessible={}",
                    result.total_files,
                    result.total_dirs,
                    result.total_size,
                    result.inaccessible_count
                );
                reconcile_timer.finish_with(&detail);
                attempt_timer.finish_with(&detail);
                total_timer.finish_with(&detail);
                return Ok(Some(result));
            }
            PostScanDecision::Retry(reason, result) if attempt < MAX_POST_SCAN_RETRIES => {
                let detail = format!(
                    "status=retry reason={} files={} dirs={} size={}",
                    reason, result.total_files, result.total_dirs, result.total_size
                );
                reconcile_timer.finish_with(&detail);
                attempt_timer.finish_with(&detail);
                tracing::info!(
                    "[mft-usn] post-scan reconcile requires retry: {} | rerunning fresh scan",
                    reason
                );
                if cancelled.load(Ordering::Relaxed) {
                    total_timer.finish_with("status=cancelled_during_retry");
                    return Err(anyhow!("扫描已取消"));
                }
                let _ = result;
            }
            PostScanDecision::Retry(reason, result) => {
                let detail = format!(
                    "status=retry_budget_exhausted reason={} files={} dirs={} size={}",
                    reason, result.total_files, result.total_dirs, result.total_size
                );
                reconcile_timer.finish_with(&detail);
                attempt_timer.finish_with(&detail);
                total_timer.finish_with(&detail);
                tracing::info!(
                    "[mft-usn] post-scan reconcile incomplete after retry budget: {} | returning latest fresh snapshot",
                    reason
                );
                return Ok(Some(result));
            }
        }
    }

    total_timer.finish_with("status=completed_without_result");
    Ok(None)
}

fn scan_path_once(
    path: &Path,
    volume: &winfs::VolumeDetails,
    root_file_id: u64,
    progress: Option<ProgressCallback>,
    estimated_files: usize,
    cancelled: Arc<AtomicBool>,
) -> Result<Option<ScanPassResult>> {
    let total_timer = StageTimer::start(
        "mft-usn-pass",
        format!("scan_path_once path={}", path.display()),
    );
    tracing::info!(
        "\n========== MFT + USN 深度扫描开始 ==========\n扫描路径: {}\n卷根路径: {}\n卷标识: {} | 文件系统: {}",
        path.display(),
        volume.volume_root.display(),
        volume.device_path,
        volume.file_system
    );

    let start = Instant::now();
    let checkpoint_start_timer = StageTimer::start(
        "mft-usn-pass",
        format!("query_start_usn_checkpoint path={}", path.display()),
    );
    let start_checkpoint = winfs::query_usn_checkpoint(path);
    match &start_checkpoint {
        Some(checkpoint) => checkpoint_start_timer.finish_with(format!(
            "available=true journal_id={} next_usn={}",
            checkpoint.journal_id, checkpoint.next_usn
        )),
        None => checkpoint_start_timer.finish_with("available=false"),
    }

    let stage = Arc::new(AtomicUsize::new(STAGE_INIT));
    let estimated_total = Arc::new(AtomicUsize::new(estimated_files.max(1)));
    let prep_total = Arc::new(AtomicUsize::new(0));
    let prep_done = Arc::new(AtomicUsize::new(0));
    let total_files = Arc::new(AtomicUsize::new(0));
    let total_dirs = Arc::new(AtomicUsize::new(0));
    let total_size = Arc::new(AtomicU64::new(0));
    let progress_stop = Arc::new(AtomicBool::new(false));

    let progress_handle = spawn_progress_thread(
        progress.clone(),
        start,
        Arc::clone(&stage),
        Arc::clone(&estimated_total),
        Arc::clone(&prep_done),
        Arc::clone(&prep_total),
        Arc::clone(&total_files),
        Arc::clone(&total_dirs),
        Arc::clone(&total_size),
        Arc::clone(&progress_stop),
        Arc::clone(&cancelled),
    );
    let _progress_guard = ProgressGuard::new(Arc::clone(&progress_stop), progress_handle);

    emit_progress(
        progress.as_ref(),
        ScanProgress {
            scanned_files: 0,
            scanned_dirs: 0,
            total_size: 0,
            current_path: "MFT 索引初始化中...".to_string(),
            elapsed_ms: 0,
            files_per_second: 0.0,
            progress_percent: 0.0,
        },
    );

    stage.store(STAGE_ENUM_MFT, Ordering::Relaxed);
    let mft_enum_timer = StageTimer::start(
        "mft-usn-pass",
        format!("enumerate_mft path={}", path.display()),
    );
    let mft_entries = winfs::enumerate_mft(path)?;
    mft_enum_timer.finish_with(format!("records={}", mft_entries.len()));
    if cancelled.load(Ordering::Relaxed) {
        return Err(anyhow!("扫描已取消"));
    }
    tracing::info!("[mft-usn] MFT 枚举完成: {} 条记录", mft_entries.len());

    let records = Arc::new(
        mft_entries
            .into_iter()
            .map(|entry| (entry.file_id, entry))
            .collect::<HashMap<_, _>>(),
    );

    stage.store(STAGE_COLLECT_TREE, Ordering::Relaxed);
    stage.store(STAGE_RESOLVE_PATHS, Ordering::Relaxed);
    let plan_timer = StageTimer::start(
        "mft-usn-pass",
        format!("build_descendant_plan path={}", path.display()),
    );
    let plan = build_descendant_plan(
        root_file_id,
        path,
        records.as_ref(),
        cancelled.as_ref(),
        prep_total.as_ref(),
        prep_done.as_ref(),
    );
    plan_timer.finish_with(format!(
        "entry_count={} directory_batches={} path_cache={}",
        plan.entry_count,
        plan.directory_batches.len(),
        plan.path_cache.len()
    ));
    if cancelled.load(Ordering::Relaxed) {
        return Err(anyhow!("扫描已取消"));
    }

    let path_cache = Arc::new(plan.path_cache);
    estimated_total.store(
        estimated_files.max(plan.entry_count).max(1),
        Ordering::Relaxed,
    );
    stage.store(STAGE_HYDRATE, Ordering::Relaxed);

    let hydration_timer = StageTimer::start(
        "mft-usn-pass",
        format!("hydrate_directory_chunks path={}", path.display()),
    );
    let partials: Vec<ChunkScanState> = plan
        .directory_batches
        .par_chunks(DIRECTORY_CHUNK_SIZE)
        .map(|chunk| {
            process_directory_chunk(
                path,
                chunk,
                records.as_ref(),
                path_cache.as_ref(),
                LARGE_FILE_THRESHOLD,
                cancelled.as_ref(),
                total_files.as_ref(),
                total_dirs.as_ref(),
                total_size.as_ref(),
            )
        })
        .collect();
    hydration_timer.finish_with(format!(
        "directory_batches={} partials={}",
        plan.directory_batches.len(),
        partials.len()
    ));

    stage.store(STAGE_AGGREGATE, Ordering::Relaxed);
    if cancelled.load(Ordering::Relaxed) {
        return Err(anyhow!("扫描已取消"));
    }

    let aggregate_timer = StageTimer::start(
        "mft-usn-pass",
        format!("aggregate_chunk_results path={}", path.display()),
    );
    let mut aggregate = ChunkScanState::default();
    for partial in partials {
        aggregate.merge(partial);
    }
    aggregate_timer.finish_with(format!(
        "dir_nodes={} file_stats={} large_files={} scanned_files={} scanned_dirs={} inaccessible={}",
        aggregate.dir_nodes.len(),
        aggregate.dir_file_stats.len(),
        aggregate.large_files.len(),
        aggregate.scanned_files,
        aggregate.scanned_dirs,
        aggregate.inaccessible_count
    ));

    let tree_timer = StageTimer::start(
        "mft-usn-pass",
        format!("build_directory_tree path={}", path.display()),
    );
    let mut nodes_map = aggregate.dir_nodes;
    let (root_file_size, root_file_count) = aggregate
        .dir_file_stats
        .get(path)
        .copied()
        .unwrap_or((0, 0));

    for (dir_path, (size, count)) in &aggregate.dir_file_stats {
        if let Some(node) = nodes_map.get_mut(dir_path) {
            node.size = *size;
            node.file_count = *count;
        }
    }

    let mut all_paths: Vec<PathBuf> = nodes_map.keys().cloned().collect();
    all_paths.sort_by_key(|p| std::cmp::Reverse(p.components().count()));

    for dir_path in &all_paths {
        if let Some(parent_path) = dir_path.parent() {
            if let Some(child) = nodes_map.get(dir_path) {
                let child_size = child.size;
                let child_files = child.file_count;
                let child_dirs = child.dir_count;
                if let Some(parent) = nodes_map.get_mut(parent_path) {
                    parent.size += child_size;
                    parent.file_count += child_files;
                    parent.dir_count += child_dirs;
                }
            }
        }
    }

    for dir_path in &all_paths {
        if let Some(parent_path) = dir_path.parent() {
            if parent_path != path {
                if let Some(child_node) = nodes_map.remove(dir_path) {
                    if let Some(parent_node) = nodes_map.get_mut(parent_path) {
                        parent_node.children.push(child_node);
                    }
                }
            }
        }
    }

    let mut directories: Vec<DirectoryNode> = nodes_map
        .into_iter()
        .filter(|(candidate, _)| candidate.parent() == Some(path))
        .map(|(_, node)| node)
        .collect();

    if root_file_size > 0 || root_file_count > 0 {
        directories.push(DirectoryNode {
            path: path.to_string_lossy().to_string(),
            name: "根目录文件".to_string(),
            size: root_file_size,
            file_count: root_file_count,
            dir_count: 0,
            children: Vec::new(),
            has_children: false,
            is_symlink: false,
            link_target: None,
            safety: None,
            modified_time: None,
            file_id: None,
        });
    }
    tree_timer.finish_with(format!(
        "top_level_nodes={} total_paths={} root_file_count={} root_file_size={}",
        directories.len(),
        all_paths.len(),
        root_file_count,
        root_file_size
    ));

    let sort_timer = StageTimer::start(
        "mft-usn-pass",
        format!("sort_directory_tree path={}", path.display()),
    );
    sort_directory_tree(&mut directories);
    sort_timer.finish_with(format!("top_level_nodes={}", directories.len()));

    let scanned_files = aggregate.scanned_files;
    let scanned_size = aggregate.total_size;
    let scanned_dirs = sum_dir_count(&directories);
    let checkpoint_end_timer = StageTimer::start(
        "mft-usn-pass",
        format!("query_end_usn_checkpoint path={}", path.display()),
    );
    let end_checkpoint = winfs::query_usn_checkpoint(path);
    match &end_checkpoint {
        Some(checkpoint) => checkpoint_end_timer.finish_with(format!(
            "available=true journal_id={} next_usn={}",
            checkpoint.journal_id, checkpoint.next_usn
        )),
        None => checkpoint_end_timer.finish_with("available=false"),
    }
    let duration = start.elapsed();

    if let Some(checkpoint) = &end_checkpoint {
        tracing::info!(
            "[mft-usn] captured USN checkpoint journal_id={} next_usn={}",
            checkpoint.journal_id, checkpoint.next_usn
        );
    } else {
        tracing::info!(
            "[mft-usn] USN checkpoint unavailable after scan; subsequent runs will rebuild the deep cache instead of using slow mtime incremental"
        );
    }

    tracing::info!(
        "========== MFT + USN 深度扫描完成 ==========\n耗时: {:.2}s | 文件: {} | 目录: {} | 大小: {:.2} GB",
        duration.as_secs_f64(),
        scanned_files,
        scanned_dirs,
        scanned_size as f64 / 1024.0 / 1024.0 / 1024.0
    );

    let disk_usage_timer = StageTimer::start(
        "mft-usn-pass",
        format!("query_disk_usage path={}", path.display()),
    );
    let disk_used = get_disk_used_bytes(path);
    let mut system_reserved_bytes = 0;
    match disk_used {
        Some(bytes) => {
            disk_usage_timer.finish_with(format!("disk_used={bytes}"));
            let missing = bytes.saturating_sub(scanned_size);
            system_reserved_bytes = missing;
            let missing_pct = if bytes > 0 {
                missing as f64 / bytes as f64 * 100.0
            } else {
                0.0
            };
            tracing::info!(
                "磁盘已用: {:.2} GB | 扫描到普通文件: {:.2} GB | 系统保留/卷元数据差额: {:.2} GB ({:.1}%)",
                bytes as f64 / 1024.0 / 1024.0 / 1024.0,
                scanned_size as f64 / 1024.0 / 1024.0 / 1024.0,
                missing as f64 / 1024.0 / 1024.0 / 1024.0,
                missing_pct
            );
        }
        None => disk_usage_timer.finish_with("disk_used=unavailable"),
    }

    tracing::info!(
        "[mft-usn] metadata path fallback hits={} recovered_size={:.2} GB unresolved_inaccessible={}",
        aggregate.metadata_fallback_count,
        aggregate.metadata_fallback_bytes as f64 / 1024.0 / 1024.0 / 1024.0,
        aggregate.inaccessible_count
    );
    if !aggregate.inaccessible_samples.is_empty() {
        tracing::info!(
            "[mft-usn] inaccessible sample paths: {}",
            aggregate.inaccessible_samples.join(" | ")
        );
    }

    total_timer.finish_with(format!(
        "files={} dirs={} size={} inaccessible={} large_files={}",
        scanned_files,
        scanned_dirs,
        scanned_size,
        aggregate.inaccessible_count,
        aggregate.large_files.len()
    ));

    Ok(Some(ScanPassResult {
        start_checkpoint,
        end_checkpoint,
        result: ScanResult {
            root_path: path.to_string_lossy().to_string(),
            total_size: scanned_size,
            system_reserved_bytes,
            total_files: scanned_files,
            total_dirs: scanned_dirs,
            scan_duration_ms: duration.as_millis() as u64,
            directories,
            large_files: aggregate.large_files,
            inaccessible_count: aggregate.inaccessible_count,
            scan_backend: Some(ScanBackendKind::MftUsn.label().to_string()),
            root_file_id: Some(root_file_id),
            usn_journal_id: end_checkpoint.map(|item| item.journal_id),
            usn_next_usn: end_checkpoint.map(|item| item.next_usn),
            cache_schema_version: 0,
            env_fingerprint: Default::default(),
            scan_completed: false,
        },
    }))
}

fn build_descendant_plan(
    root_file_id: u64,
    root_path: &Path,
    records: &HashMap<u64, MftEntry>,
    cancelled: &AtomicBool,
    prep_total: &AtomicUsize,
    prep_done: &AtomicUsize,
) -> DescendantPlan {
    let mut children_by_parent: HashMap<u64, Vec<u64>> = HashMap::new();
    for (&file_id, record) in records {
        children_by_parent
            .entry(record.parent_file_id)
            .or_default()
            .push(file_id);
    }

    prep_total.store(records.len().max(1), Ordering::Relaxed);

    let mut path_cache = HashMap::with_capacity(records.len() + 1);
    let mut directory_batches = Vec::new();
    let mut visited = HashSet::new();
    let mut pending_dirs = vec![root_file_id];
    let mut processed = 0usize;

    path_cache.insert(root_file_id, root_path.to_path_buf());
    visited.insert(root_file_id);

    while let Some(parent_id) = pending_dirs.pop() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let Some(parent_path) = path_cache.get(&parent_id).cloned() else {
            continue;
        };
        let child_ids = children_by_parent.remove(&parent_id).unwrap_or_default();

        for child_id in &child_ids {
            let Some(record) = records.get(child_id) else {
                continue;
            };
            if !visited.insert(*child_id) {
                continue;
            }

            path_cache.insert(*child_id, parent_path.join(&record.name));
            processed += 1;

            if processed.is_multiple_of(4096) {
                prep_done.store(processed, Ordering::Relaxed);
            }

            if record.is_dir {
                pending_dirs.push(*child_id);
            }
        }

        directory_batches.push(DirectoryBatch {
            path: parent_path,
            child_ids,
        });
    }

    prep_done.store(processed.max(1), Ordering::Relaxed);

    DescendantPlan {
        entry_count: processed,
        path_cache,
        directory_batches,
    }
}

fn process_directory_chunk(
    scan_root: &Path,
    chunk: &[DirectoryBatch],
    records: &HashMap<u64, MftEntry>,
    path_cache: &HashMap<u64, PathBuf>,
    large_file_threshold: u64,
    cancelled: &AtomicBool,
    total_files: &AtomicUsize,
    total_dirs: &AtomicUsize,
    total_size: &AtomicU64,
) -> ChunkScanState {
    #[cfg(windows)]
    let volume = winfs::open_volume_handle(scan_root).ok();

    let mut chunk_state = ChunkScanState::default();

    for batch in chunk {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let mut entry_map = winfs::enumerate_directory(&batch.path, false)
            .ok()
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|entry| (normalized_name_key(&entry.name), entry))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();

        for file_id in &batch.child_ids {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }

            let Some(record) = records.get(file_id) else {
                chunk_state.inaccessible_count += 1;
                continue;
            };
            let Some(entry_path) = path_cache.get(file_id) else {
                chunk_state.inaccessible_count += 1;
                continue;
            };

            let entry = entry_map.remove(&normalized_name_key(&record.name));
            let entry_is_symlink = entry.as_ref().map(|item| item.is_symlink).unwrap_or(false);

            if record.is_dir {
                let is_symlink = entry
                    .as_ref()
                    .map(|item| item.is_symlink)
                    .unwrap_or(record.is_reparse_point);
                let modified_time = if is_symlink {
                    query_metadata_with_fallback(
                        &mut chunk_state,
                        scan_root,
                        #[cfg(windows)]
                        volume.as_ref(),
                        entry_path,
                        *file_id,
                        true,
                    )
                    .and_then(|meta| meta.modified_time)
                    .or_else(|| entry.as_ref().and_then(|item| item.modified_time))
                } else {
                    entry
                        .as_ref()
                        .and_then(|item| item.modified_time)
                        .or_else(|| {
                            query_metadata_with_fallback(
                                &mut chunk_state,
                                scan_root,
                                #[cfg(windows)]
                                volume.as_ref(),
                                entry_path,
                                *file_id,
                                true,
                            )
                            .and_then(|meta| meta.modified_time)
                        })
                };

                insert_directory_node(
                    &mut chunk_state,
                    total_dirs,
                    entry_path,
                    record,
                    *file_id,
                    is_symlink,
                    modified_time,
                );
                continue;
            }

            if let Some(entry) = entry {
                if !record.is_reparse_point && !entry.is_symlink {
                    add_file_to_state(
                        &mut chunk_state,
                        total_files,
                        total_size,
                        entry_path,
                        record,
                        entry.size,
                        entry.modified_time,
                        entry.is_readonly || record.is_readonly,
                        false,
                        large_file_threshold,
                    );
                    continue;
                }
            }

            let Some(metadata) = query_metadata_with_fallback(
                &mut chunk_state,
                scan_root,
                #[cfg(windows)]
                volume.as_ref(),
                entry_path,
                *file_id,
                false,
            ) else {
                continue;
            };

            add_file_to_state(
                &mut chunk_state,
                total_files,
                total_size,
                entry_path,
                record,
                metadata.size,
                metadata.modified_time,
                metadata.is_readonly || record.is_readonly,
                record.is_reparse_point || entry_is_symlink,
                large_file_threshold,
            );
        }
    }

    #[cfg(windows)]
    if let Some(handle) = volume {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(handle);
        }
    }

    chunk_state
}

fn insert_directory_node(
    chunk_state: &mut ChunkScanState,
    total_dirs: &AtomicUsize,
    entry_path: &Path,
    record: &MftEntry,
    file_id: u64,
    is_symlink: bool,
    modified_time: Option<u64>,
) {
    chunk_state.dir_nodes.insert(
        entry_path.to_path_buf(),
        DirectoryNode {
            path: entry_path.to_string_lossy().to_string(),
            name: record.name.clone(),
            size: 0,
            file_count: 0,
            dir_count: 1,
            children: Vec::new(),
            has_children: false,
            is_symlink,
            link_target: if is_symlink {
                winfs::resolve_link_target(entry_path)
            } else {
                None
            },
            safety: None,
            modified_time,
            file_id: (!is_symlink).then_some(file_id),
        },
    );
    chunk_state.scanned_dirs += 1;
    total_dirs.fetch_add(1, Ordering::Relaxed);
}

fn add_file_to_state(
    chunk_state: &mut ChunkScanState,
    total_files: &AtomicUsize,
    total_size: &AtomicU64,
    entry_path: &Path,
    record: &MftEntry,
    size: u64,
    modified_time: Option<u64>,
    is_readonly: bool,
    is_symlink: bool,
    large_file_threshold: u64,
) {
    if let Some(parent) = entry_path.parent() {
        let stats = chunk_state
            .dir_file_stats
            .entry(parent.to_path_buf())
            .or_insert((0, 0));
        stats.0 += size;
        stats.1 += 1;
    }

    chunk_state.scanned_files += 1;
    chunk_state.total_size += size;
    total_files.fetch_add(1, Ordering::Relaxed);
    total_size.fetch_add(size, Ordering::Relaxed);

    if size < large_file_threshold {
        return;
    }

    let modified_at = modified_time
        .and_then(|secs| chrono::DateTime::from_timestamp(secs as i64, 0))
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();

    chunk_state.large_files.push(FileInfo {
        path: entry_path.to_string_lossy().to_string(),
        name: record.name.clone(),
        size,
        extension: entry_path
            .extension()
            .and_then(|item| item.to_str())
            .unwrap_or("")
            .to_string(),
        modified_at,
        is_readonly,
        is_symlink,
        link_target: if is_symlink {
            winfs::resolve_link_target(entry_path)
        } else {
            None
        },
    });
}

fn normalized_name_key(name: &str) -> String {
    name.to_lowercase()
}

#[cfg(windows)]
fn query_metadata(
    scan_root: &Path,
    volume: Option<&windows::Win32::Foundation::HANDLE>,
    file_id: u64,
    open_as_directory: bool,
) -> std::io::Result<winfs::FileIdMetadata> {
    if let Some(volume) = volume {
        winfs::query_file_metadata_by_id_on_volume(*volume, file_id, open_as_directory)
    } else {
        winfs::query_file_metadata_by_id(scan_root, file_id, open_as_directory)
    }
}

#[cfg(not(windows))]
fn query_metadata(
    scan_root: &Path,
    file_id: u64,
    open_as_directory: bool,
) -> std::io::Result<winfs::FileIdMetadata> {
    winfs::query_file_metadata_by_id(scan_root, file_id, open_as_directory)
}

#[cfg(windows)]
fn query_metadata_with_fallback(
    chunk_state: &mut ChunkScanState,
    scan_root: &Path,
    volume: Option<&windows::Win32::Foundation::HANDLE>,
    entry_path: &Path,
    file_id: u64,
    open_as_directory: bool,
) -> Option<winfs::FileIdMetadata> {
    match query_metadata(scan_root, volume, file_id, open_as_directory) {
        Ok(metadata) => Some(metadata),
        Err(_) => match metadata_from_path(entry_path) {
            Ok(metadata) => {
                chunk_state.metadata_fallback_count += 1;
                if !open_as_directory {
                    chunk_state.metadata_fallback_bytes += metadata.size;
                }
                Some(metadata)
            }
            Err(_) => {
                chunk_state.inaccessible_count += 1;
                note_inaccessible_path(chunk_state, entry_path);
                None
            }
        },
    }
}

#[cfg(not(windows))]
fn query_metadata_with_fallback(
    chunk_state: &mut ChunkScanState,
    scan_root: &Path,
    entry_path: &Path,
    file_id: u64,
    open_as_directory: bool,
) -> Option<winfs::FileIdMetadata> {
    match query_metadata(scan_root, file_id, open_as_directory) {
        Ok(metadata) => Some(metadata),
        Err(_) => match metadata_from_path(entry_path) {
            Ok(metadata) => {
                chunk_state.metadata_fallback_count += 1;
                if !open_as_directory {
                    chunk_state.metadata_fallback_bytes += metadata.size;
                }
                Some(metadata)
            }
            Err(_) => {
                chunk_state.inaccessible_count += 1;
                note_inaccessible_path(chunk_state, entry_path);
                None
            }
        },
    }
}

fn reconcile_post_scan_window(
    root_path: &Path,
    mut result: ScanResult,
    start_checkpoint: Option<winfs::UsnJournalCheckpoint>,
    end_checkpoint: Option<winfs::UsnJournalCheckpoint>,
) -> PostScanDecision {
    let total_timer = StageTimer::start(
        "mft-usn-reconcile",
        format!("reconcile_post_scan_window path={}", root_path.display()),
    );
    let started = Instant::now();

    if let Some(checkpoint) = &end_checkpoint {
        result.usn_journal_id = Some(checkpoint.journal_id);
        result.usn_next_usn = Some(checkpoint.next_usn);
    }

    let (Some(start), Some(end)) = (start_checkpoint, end_checkpoint) else {
        total_timer.finish_with("status=skipped_missing_checkpoint");
        return PostScanDecision::Complete(result);
    };

    if start.journal_id != end.journal_id {
        total_timer.finish_with("status=retry reason=usn_journal_rotated_during_scan");
        return PostScanDecision::Retry("usn_journal_rotated_during_scan".to_string(), result);
    }

    if end.next_usn <= start.next_usn {
        total_timer.finish_with("status=no_post_scan_delta");
        return PostScanDecision::Complete(result);
    }

    let file_id_map_timer = StageTimer::start(
        "mft-usn-reconcile",
        format!("build_file_id_map path={}", root_path.display()),
    );
    let mut frn_to_path = HashMap::new();
    if let Some(root_file_id) = result.root_file_id {
        frn_to_path.insert(root_file_id, root_path.to_string_lossy().to_string());
    }
    build_file_id_map(&result.directories, &mut frn_to_path);
    file_id_map_timer.finish_with(format!("entries={}", frn_to_path.len()));

    let change_set_timer = StageTimer::start(
        "mft-usn-reconcile",
        format!("collect_post_scan_change_set path={}", root_path.display()),
    );
    let change_set = match winfs::collect_usn_changed_dirs(
        root_path,
        start,
        result.root_file_id,
        &frn_to_path,
    ) {
        Ok(Some(change_set)) => {
            change_set_timer.finish_with(format!(
                "recursive_dirs={} direct_file_dirs={} root_files_changed={}",
                change_set.recursive_dirs.len(),
                change_set.direct_file_dirs.len(),
                change_set.root_files_changed
            ));
            change_set
        }
        Ok(None) => {
            change_set_timer.finish_with("status=retry reason=post_scan_delta_unavailable");
            total_timer.finish_with("status=retry reason=post_scan_delta_unavailable");
            return PostScanDecision::Retry("post_scan_delta_unavailable".to_string(), result);
        }
        Err(err) => {
            change_set_timer.finish_with(format!(
                "status=retry reason=post_scan_delta_read_failed:{err}"
            ));
            total_timer.finish_with(format!(
                "status=retry reason=post_scan_delta_read_failed:{err}"
            ));
            return PostScanDecision::Retry(format!("post_scan_delta_read_failed: {err}"), result);
        }
    };

    let changed_dirs = normalize_changed_dirs(root_path, &change_set);
    let total_changes = changed_dirs.len() + usize::from(change_set.root_files_changed);

    if total_changes == 0 {
        total_timer.finish_with("status=no_changes_after_normalization");
        return PostScanDecision::Complete(result);
    }

    tracing::info!(
        "[mft-usn] post-scan delta detected changed_dirs={} root_files_changed={}",
        changed_dirs.len(),
        change_set.root_files_changed
    );

    if total_changes > POST_SCAN_RESCAN_LIMIT {
        total_timer.finish_with(format!(
            "status=retry reason=post_scan_delta_too_large total_changes={total_changes}"
        ));
        return PostScanDecision::Retry(
            format!("post_scan_delta_too_large({total_changes})"),
            result,
        );
    }

    let rescan_timer = StageTimer::start(
        "mft-usn-reconcile",
        format!("rescan_changed_dirs path={}", root_path.display()),
    );
    let mut rescanned_nodes = Vec::new();
    let mut rescanned_large_files = Vec::new();
    let mut deleted_paths = Vec::new();
    let mut own_overrides: HashMap<String, (u64, usize)> = HashMap::new();

    for changed_path in &changed_dirs {
        if changed_path.exists() {
            if let Some((node, large_files, own_size, own_file_count)) =
                incremental::rescan_directory_snapshot(changed_path, LARGE_FILE_THRESHOLD)
            {
                own_overrides.insert(
                    normalized_path_key_str(&node.path),
                    (own_size, own_file_count),
                );
                rescanned_nodes.push(node);
                rescanned_large_files.extend(large_files);
            } else {
                deleted_paths.push(changed_path.to_string_lossy().to_string());
            }
        } else {
            deleted_paths.push(changed_path.to_string_lossy().to_string());
        }
    }
    rescan_timer.finish_with(format!(
        "changed_dirs={} rescanned_nodes={} deleted_paths={} rescanned_large_files={}",
        changed_dirs.len(),
        rescanned_nodes.len(),
        deleted_paths.len(),
        rescanned_large_files.len()
    ));

    let merge_timer = StageTimer::start(
        "mft-usn-reconcile",
        format!("merge_reconciled_delta path={}", root_path.display()),
    );
    let mut directories = incremental::merge_scan_results_with_own(
        result.directories,
        rescanned_nodes,
        &own_overrides,
        deleted_paths,
        root_path,
    );

    let root_large_files = if change_set.root_files_changed {
        let (root_size, root_files, root_large_files) =
            incremental::scan_root_files(root_path, LARGE_FILE_THRESHOLD);
        incremental::upsert_root_files_node(&mut directories, root_path, root_size, root_files);
        root_large_files
    } else {
        Vec::new()
    };

    let changed_dir_keys: Vec<String> = changed_dirs
        .iter()
        .map(|path| normalized_path_key(path))
        .collect();
    let root_key = normalized_path_key(root_path);

    let mut large_files: Vec<FileInfo> = result
        .large_files
        .into_iter()
        .filter(|file| {
            let file_path = Path::new(&file.path);
            let file_key = normalized_path_key_str(&file.path);
            !changed_dir_keys
                .iter()
                .any(|changed_path| normalized_path_starts_with(&file_key, changed_path))
                && (!change_set.root_files_changed
                    || !file_path
                        .parent()
                        .map(|parent| normalized_path_key(parent) == root_key)
                        .unwrap_or(false))
        })
        .collect();
    large_files.extend(rescanned_large_files);
    large_files.extend(root_large_files);
    large_files.sort_by_key(|f| std::cmp::Reverse(f.size));

    let (total_size, total_files) = incremental::sum_tree(&directories);
    let total_dirs = incremental::count_directories(&directories, &result.root_path);

    result.directories = directories;
    result.large_files = large_files;
    result.total_size = total_size;
    result.total_files = total_files;
    result.total_dirs = total_dirs;
    result.scan_duration_ms += started.elapsed().as_millis() as u64;
    merge_timer.finish_with(format!(
        "total_files={} total_dirs={} total_size={} large_files={}",
        result.total_files,
        result.total_dirs,
        result.total_size,
        result.large_files.len()
    ));

    tracing::info!(
        "[mft-usn] post-scan delta reconciled in {:.2}ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    total_timer.finish_with(format!(
        "status=complete total_changes={} total_files={} total_dirs={} total_size={}",
        total_changes, result.total_files, result.total_dirs, result.total_size
    ));

    PostScanDecision::Complete(result)
}

fn build_file_id_map(nodes: &[DirectoryNode], map: &mut HashMap<u64, String>) {
    for node in nodes {
        if let Some(file_id) = node.file_id {
            map.insert(file_id, node.path.clone());
        }
        build_file_id_map(&node.children, map);
    }
}

fn normalize_changed_dirs(root_path: &Path, change_set: &winfs::UsnChangeSet) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = change_set
        .recursive_dirs
        .iter()
        .chain(change_set.direct_file_dirs.iter())
        .map(PathBuf::from)
        .filter(|candidate| candidate != root_path)
        .filter(|candidate| path_starts_with(candidate, root_path))
        .collect();

    paths.sort_by(|a, b| {
        a.components()
            .count()
            .cmp(&b.components().count())
            .then_with(|| a.cmp(b))
    });

    let mut normalized: Vec<PathBuf> = Vec::new();
    for path in paths {
        if normalized
            .iter()
            .any(|existing| path_starts_with(&path, existing))
        {
            continue;
        }
        normalized.push(path);
    }

    normalized
}

fn path_starts_with(candidate: &Path, prefix: &Path) -> bool {
    let candidate_key = normalized_path_key(candidate);
    let prefix_key = normalized_path_key(prefix);
    normalized_path_starts_with(&candidate_key, &prefix_key)
}

fn spawn_progress_thread(
    progress: Option<ProgressCallback>,
    start: Instant,
    stage: Arc<AtomicUsize>,
    estimated_total: Arc<AtomicUsize>,
    prep_done: Arc<AtomicUsize>,
    prep_total: Arc<AtomicUsize>,
    total_files: Arc<AtomicUsize>,
    total_dirs: Arc<AtomicUsize>,
    total_size: Arc<AtomicU64>,
    progress_stop: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    let progress = progress?;

    Some(std::thread::spawn(move || {
        let mut last_files = 0usize;
        let mut last_time = Instant::now();
        let mut last_pct = 0.0f64;
        let mut est = estimated_total.load(Ordering::Relaxed).max(1);

        loop {
            // 把 PROGRESS_INTERVAL_MS (300ms) 拆成 15 × 20ms，每片检查停止旗，
            // 避免扫描已结束但仍硬等满间隔的尾部延迟。
            let mut waited = 0u64;
            while waited < PROGRESS_INTERVAL_MS {
                if progress_stop.load(Ordering::Relaxed) || cancelled.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
                waited += 20;
            }
            if progress_stop.load(Ordering::Relaxed) || cancelled.load(Ordering::Relaxed) {
                break;
            }

            let cur_files = total_files.load(Ordering::Relaxed);
            let cur_dirs = total_dirs.load(Ordering::Relaxed);
            let cur_size = total_size.load(Ordering::Relaxed);
            let elapsed = start.elapsed().as_millis() as u64;
            let now = Instant::now();
            let dt = now.duration_since(last_time).as_secs_f64();
            let fps = if dt > 0.0 {
                (cur_files - last_files) as f64 / dt
            } else {
                0.0
            };

            last_files = cur_files;
            last_time = now;

            let stage_code = stage.load(Ordering::Relaxed);
            let (current_path, raw_pct) = match stage_code {
                STAGE_INIT => ("MFT 索引初始化中...".to_string(), 0.0),
                STAGE_ENUM_MFT => ("MFT 枚举中...".to_string(), 1.5),
                STAGE_COLLECT_TREE => ("MFT 子树收集中...".to_string(), 3.5),
                STAGE_RESOLVE_PATHS => {
                    let total = prep_total.load(Ordering::Relaxed).max(1);
                    let done = prep_done.load(Ordering::Relaxed).min(total);
                    let ratio = done as f64 / total as f64;
                    (
                        format!("MFT 路径索引构建中... {}/{}", done, total),
                        5.0 + (ratio * 5.0).min(5.0),
                    )
                }
                STAGE_HYDRATE => {
                    let atomic_est = estimated_total.load(Ordering::Relaxed).max(1);
                    if atomic_est > est {
                        est = atomic_est;
                    }
                    if cur_files > (est as f64 * 0.9) as usize {
                        est = ((cur_files.max(1) as f64) * 1.2) as usize;
                    }

                    let raw = (cur_files as f64 / est as f64 * 90.0).min(89.0);
                    ("MFT/USN 元数据水合中...".to_string(), 10.0 + raw)
                }
                STAGE_AGGREGATE => ("目录聚合与排序中...".to_string(), 99.0),
                _ => ("深度扫描中...".to_string(), last_pct),
            };

            let pct = raw_pct.max(last_pct);
            last_pct = pct;

            emit_progress(
                Some(&progress),
                ScanProgress {
                    scanned_files: cur_files as u64,
                    scanned_dirs: cur_dirs as u64,
                    total_size: cur_size,
                    current_path,
                    elapsed_ms: elapsed,
                    files_per_second: fps,
                    progress_percent: pct,
                },
            );
        }
    }))
}

fn sort_directory_tree(nodes: &mut [DirectoryNode]) {
    for node in nodes.iter_mut() {
        sort_directory_tree(&mut node.children);
        node.has_children = !node.children.is_empty();
        node.children.sort_by_key(|n| std::cmp::Reverse(n.size));
    }
    nodes.sort_by_key(|n| std::cmp::Reverse(n.size));
}

fn sum_dir_count(nodes: &[DirectoryNode]) -> usize {
    nodes.iter().map(|node| node.dir_count).sum()
}

fn emit_progress(callback: Option<&ProgressCallback>, progress: ScanProgress) {
    if let Some(callback) = callback {
        callback(progress);
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    fn test_lock() -> &'static Mutex<()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK.get_or_init(|| Mutex::new(()))
    }

    struct TestWorkspace {
        root: PathBuf,
    }

    impl TestWorkspace {
        fn new(name: &str) -> Self {
            let unique = format!(
                "{}-{}-{}",
                name,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let root = std::env::temp_dir().join(unique);
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn write_file(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = fs::File::create(path).unwrap();
        file.write_all(bytes).unwrap();
    }

    fn build_file_id_map(nodes: &[DirectoryNode], map: &mut HashMap<u64, String>) {
        for node in nodes {
            if let Some(file_id) = node.file_id {
                map.insert(file_id, node.path.clone());
            }
            build_file_id_map(&node.children, map);
        }
    }

    #[test]
    fn mft_scan_matches_expected_temp_tree() {
        let _guard = test_lock().lock().unwrap();
        let workspace = TestWorkspace::new("mft-scan");
        if !winfs::supports_mft_scan(&workspace.root) {
            return;
        }

        let alpha = workspace.root.join("alpha");
        let beta = alpha.join("beta");
        let empty = workspace.root.join("empty");
        fs::create_dir_all(&beta).unwrap();
        fs::create_dir_all(&empty).unwrap();
        write_file(&workspace.root.join("root.log"), b"root-bytes");
        write_file(&alpha.join("a.bin"), &[7u8; 128]);
        write_file(&beta.join("b.txt"), b"payload");

        let result = scan_path(&workspace.root, None, 64, Arc::new(AtomicBool::new(false)))
            .unwrap()
            .expect("expected NTFS MFT backend to run");

        assert_eq!(result.scan_backend.as_deref(), Some("mft_usn"));
        assert_eq!(result.total_files, 3);
        assert_eq!(result.total_dirs, 3);
        assert_eq!(
            result.total_size,
            "root-bytes".len() as u64 + 128 + "payload".len() as u64
        );

        let alpha_node = result
            .directories
            .iter()
            .find(|node| node.path == alpha.to_string_lossy())
            .expect("missing alpha node");
        assert_eq!(alpha_node.file_count, 2);
        assert_eq!(alpha_node.dir_count, 2);

        let root_files = result
            .directories
            .iter()
            .find(|node| node.path == workspace.root.to_string_lossy() && node.name == "根目录文件")
            .expect("missing root files node");
        assert_eq!(root_files.file_count, 1);
        assert_eq!(root_files.size, "root-bytes".len() as u64);
    }

    #[test]
    fn usn_change_collection_reports_modified_directory() {
        let _guard = test_lock().lock().unwrap();
        let workspace = TestWorkspace::new("usn-change");
        if !winfs::supports_mft_scan(&workspace.root) {
            return;
        }

        let nested = workspace.root.join("nested");
        let target_file = nested.join("target.txt");
        write_file(&target_file, b"before");

        let result = scan_path(&workspace.root, None, 64, Arc::new(AtomicBool::new(false)))
            .unwrap()
            .expect("expected NTFS MFT backend to run");

        let (Some(journal_id), Some(next_usn), Some(root_file_id)) = (
            result.usn_journal_id,
            result.usn_next_usn,
            result.root_file_id,
        ) else {
            return;
        };

        std::thread::sleep(Duration::from_millis(50));
        write_file(&target_file, b"after-change");

        let mut frn_to_path = HashMap::new();
        frn_to_path.insert(root_file_id, workspace.root.to_string_lossy().to_string());
        build_file_id_map(&result.directories, &mut frn_to_path);

        let checkpoint = winfs::UsnJournalCheckpoint {
            journal_id,
            next_usn,
        };

        let mut change_set = None;
        for _ in 0..10 {
            let current = winfs::collect_usn_changed_dirs(
                &workspace.root,
                checkpoint,
                Some(root_file_id),
                &frn_to_path,
            )
            .unwrap();

            if current
                .as_ref()
                .map(|changes| !changes.all_changed_dirs().is_empty() || changes.root_files_changed)
                .unwrap_or(false)
            {
                change_set = current;
                break;
            }

            std::thread::sleep(Duration::from_millis(200));
        }

        let change_set = change_set.expect("expected a USN delta result");
        let changed_dirs = change_set.all_changed_dirs();
        assert!(
            changed_dirs.contains(&nested.to_string_lossy().to_string()),
            "expected nested directory to be marked changed, got {:?}",
            changed_dirs
        );
    }
}
