use anyhow::{anyhow, Result};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use super::backend::ScanBackendKind;
use super::file_info::{DirectoryNode, FileInfo, ScanResult};
use super::progress::ScanProgress;
use crate::winfs::{self, MftEntry};

const LARGE_FILE_THRESHOLD: u64 = 100 * 1024 * 1024;
const HYDRATION_CHUNK_SIZE: usize = 2048;
const PROGRESS_INTERVAL_MS: u64 = 300;

pub type ProgressCallback = Arc<dyn Fn(ScanProgress) + Send + Sync>;

#[derive(Default)]
struct ChunkScanState {
    dir_nodes: HashMap<PathBuf, DirectoryNode>,
    dir_file_stats: HashMap<PathBuf, (u64, usize)>,
    large_files: Vec<FileInfo>,
    inaccessible_count: usize,
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
    if !winfs::supports_mft_scan(path) {
        return Ok(None);
    }

    let root_file_id = match winfs::get_path_file_id(path) {
        Some(file_id) => file_id,
        None => return Ok(None),
    };
    let volume = match winfs::query_volume_details(path) {
        Some(volume) => volume,
        None => return Ok(None),
    };

    println!(
        "\n========== MFT + USN 深度扫描开始 ==========\n扫描路径: {}\n卷根路径: {}\n卷标识: {} | 文件系统: {}",
        path.display(),
        volume.volume_root.display(),
        volume.device_path,
        volume.file_system
    );

    let start = Instant::now();
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

    let mft_entries = winfs::enumerate_mft(path)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(anyhow!("扫描已取消"));
    }
    println!("[mft-usn] MFT 枚举完成: {} 条记录", mft_entries.len());

    let records = Arc::new(
        mft_entries
            .into_iter()
            .map(|entry| (entry.file_id, entry))
            .collect::<HashMap<_, _>>(),
    );

    let descendant_ids = collect_descendant_ids(root_file_id, records.as_ref(), cancelled.as_ref());
    if cancelled.load(Ordering::Relaxed) {
        return Err(anyhow!("扫描已取消"));
    }

    let mut path_cache = HashMap::with_capacity(descendant_ids.len() + 1);
    for file_id in &descendant_ids {
        let _ = resolve_entry_path(*file_id, root_file_id, path, records.as_ref(), &mut path_cache);
    }
    let path_cache = Arc::new(path_cache);

    let entry_ids: Vec<u64> = descendant_ids
        .into_iter()
        .filter(|file_id| *file_id != root_file_id)
        .collect();

    let total_files = Arc::new(AtomicUsize::new(0));
    let total_dirs = Arc::new(AtomicUsize::new(0));
    let total_size = Arc::new(AtomicU64::new(0));
    let progress_stop = Arc::new(AtomicBool::new(false));

    let progress_handle = spawn_progress_thread(
        progress.clone(),
        start,
        estimated_files.max(entry_ids.len()).max(1),
        Arc::clone(&total_files),
        Arc::clone(&total_dirs),
        Arc::clone(&total_size),
        Arc::clone(&progress_stop),
        Arc::clone(&cancelled),
    );

    let partials: Vec<ChunkScanState> = entry_ids
        .par_chunks(HYDRATION_CHUNK_SIZE)
        .map(|chunk| {
            process_chunk(
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

    progress_stop.store(true, Ordering::Relaxed);
    if let Some(handle) = progress_handle {
        let _ = handle.join();
    }

    if cancelled.load(Ordering::Relaxed) {
        return Err(anyhow!("扫描已取消"));
    }

    let mut aggregate = ChunkScanState::default();
    for partial in partials {
        aggregate.merge(partial);
    }

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
    all_paths.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

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

    sort_directory_tree(&mut directories);

    let scanned_files = aggregate.scanned_files;
    let scanned_size = aggregate.total_size;
    let scanned_dirs = sum_dir_count(&directories);
    let journal = winfs::query_usn_checkpoint(path);
    let duration = start.elapsed();

    println!(
        "========== MFT + USN 深度扫描完成 ==========\n耗时: {:.2}s | 文件: {} | 目录: {} | 大小: {:.2} GB",
        duration.as_secs_f64(),
        scanned_files,
        scanned_dirs,
        scanned_size as f64 / 1024.0 / 1024.0 / 1024.0
    );

    Ok(Some(ScanResult {
        root_path: path.to_string_lossy().to_string(),
        total_size: scanned_size,
        total_files: scanned_files,
        total_dirs: scanned_dirs,
        scan_duration_ms: duration.as_millis() as u64,
        directories,
        large_files: aggregate.large_files,
        inaccessible_count: aggregate.inaccessible_count,
        scan_backend: Some(ScanBackendKind::MftUsn.label().to_string()),
        root_file_id: Some(root_file_id),
        usn_journal_id: journal.map(|item| item.journal_id),
        usn_next_usn: journal.map(|item| item.next_usn),
    }))
}

fn process_chunk(
    scan_root: &Path,
    chunk: &[u64],
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

    for file_id in chunk {
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

        if record.is_reparse_point {
            if record.is_dir {
                let modified_time = query_metadata(
                    scan_root,
                    #[cfg(windows)]
                    volume.as_ref(),
                    *file_id,
                    true,
                )
                .ok()
                .and_then(|meta| meta.modified_time);

                chunk_state.dir_nodes.insert(
                    entry_path.clone(),
                    DirectoryNode {
                        path: entry_path.to_string_lossy().to_string(),
                        name: record.name.clone(),
                        size: 0,
                        file_count: 0,
                        dir_count: 1,
                        children: Vec::new(),
                        has_children: false,
                        is_symlink: true,
                        link_target: resolve_link_target(entry_path),
                        safety: None,
                        modified_time,
                        file_id: None,
                    },
                );
                chunk_state.scanned_dirs += 1;
                total_dirs.fetch_add(1, Ordering::Relaxed);
            }
            continue;
        }

        if record.is_dir {
            let metadata = query_metadata(
                scan_root,
                #[cfg(windows)]
                volume.as_ref(),
                *file_id,
                true,
            )
            .ok();

            if metadata.is_none() {
                chunk_state.inaccessible_count += 1;
            }

            chunk_state.dir_nodes.insert(
                entry_path.clone(),
                DirectoryNode {
                    path: entry_path.to_string_lossy().to_string(),
                    name: record.name.clone(),
                    size: 0,
                    file_count: 0,
                    dir_count: 1,
                    children: Vec::new(),
                    has_children: false,
                    is_symlink: false,
                    link_target: None,
                    safety: None,
                    modified_time: metadata.as_ref().and_then(|item| item.modified_time),
                    file_id: Some(*file_id),
                },
            );
            chunk_state.scanned_dirs += 1;
            total_dirs.fetch_add(1, Ordering::Relaxed);
            continue;
        }

        let metadata = match query_metadata(
            scan_root,
            #[cfg(windows)]
            volume.as_ref(),
            *file_id,
            false,
        ) {
            Ok(metadata) => metadata,
            Err(_) => {
                chunk_state.inaccessible_count += 1;
                continue;
            }
        };

        if let Some(parent) = entry_path.parent() {
            let stats = chunk_state
                .dir_file_stats
                .entry(parent.to_path_buf())
                .or_insert((0, 0));
            stats.0 += metadata.size;
            stats.1 += 1;
        }

        chunk_state.scanned_files += 1;
        chunk_state.total_size += metadata.size;
        total_files.fetch_add(1, Ordering::Relaxed);
        total_size.fetch_add(metadata.size, Ordering::Relaxed);

        if metadata.size >= large_file_threshold {
            let modified_at = metadata
                .modified_time
                .and_then(|secs| chrono::DateTime::from_timestamp(secs as i64, 0))
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default();

            chunk_state.large_files.push(FileInfo {
                path: entry_path.to_string_lossy().to_string(),
                name: record.name.clone(),
                size: metadata.size,
                extension: entry_path
                    .extension()
                    .and_then(|item| item.to_str())
                    .unwrap_or("")
                    .to_string(),
                modified_at,
                is_readonly: metadata.is_readonly || record.is_readonly,
                is_symlink: false,
                link_target: None,
            });
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

fn collect_descendant_ids(
    root_file_id: u64,
    records: &HashMap<u64, MftEntry>,
    cancelled: &AtomicBool,
) -> Vec<u64> {
    let mut descendants = vec![root_file_id];
    let mut cache = HashMap::new();

    for file_id in records.keys().copied() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        if file_id == root_file_id {
            continue;
        }

        if is_descendant_of_root(file_id, root_file_id, records, &mut cache) {
            descendants.push(file_id);
        }
    }

    descendants
}

fn is_descendant_of_root(
    file_id: u64,
    root_file_id: u64,
    records: &HashMap<u64, MftEntry>,
    cache: &mut HashMap<u64, bool>,
) -> bool {
    if file_id == root_file_id {
        return true;
    }
    if let Some(cached) = cache.get(&file_id) {
        return *cached;
    }

    let mut lineage = Vec::new();
    let mut seen = HashSet::new();
    let mut current = file_id;

    loop {
        if current == root_file_id {
            for item in lineage {
                cache.insert(item, true);
            }
            return true;
        }

        if let Some(cached) = cache.get(&current).copied() {
            for item in lineage {
                cache.insert(item, cached);
            }
            return cached;
        }

        if !seen.insert(current) {
            for item in lineage {
                cache.insert(item, false);
            }
            return false;
        }

        let Some(record) = records.get(&current) else {
            for item in lineage {
                cache.insert(item, false);
            }
            return false;
        };

        lineage.push(current);
        current = record.parent_file_id;
    }
}

fn resolve_entry_path(
    file_id: u64,
    root_file_id: u64,
    root_path: &Path,
    records: &HashMap<u64, MftEntry>,
    cache: &mut HashMap<u64, PathBuf>,
) -> Option<PathBuf> {
    if let Some(existing) = cache.get(&file_id) {
        return Some(existing.clone());
    }

    if file_id == root_file_id {
        let root = root_path.to_path_buf();
        cache.insert(file_id, root.clone());
        return Some(root);
    }

    let record = records.get(&file_id)?;
    let parent_path = resolve_entry_path(record.parent_file_id, root_file_id, root_path, records, cache)?;
    let resolved = parent_path.join(&record.name);
    cache.insert(file_id, resolved.clone());
    Some(resolved)
}

fn spawn_progress_thread(
    progress: Option<ProgressCallback>,
    start: Instant,
    estimated_total: usize,
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

        loop {
            std::thread::sleep(std::time::Duration::from_millis(PROGRESS_INTERVAL_MS));
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
            let raw_pct = (cur_files as f64 / estimated_total as f64 * 100.0).min(99.0);
            let pct = raw_pct.max(last_pct);

            last_files = cur_files;
            last_time = now;
            last_pct = pct;

            emit_progress(
                Some(&progress),
                ScanProgress {
                    scanned_files: cur_files as u64,
                    scanned_dirs: cur_dirs as u64,
                    total_size: cur_size,
                    current_path: "MFT/USN 元数据水合中...".to_string(),
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
        node.children.sort_by(|a, b| b.size.cmp(&a.size));
    }
    nodes.sort_by(|a, b| b.size.cmp(&a.size));
}

fn sum_dir_count(nodes: &[DirectoryNode]) -> usize {
    nodes.iter().map(|node| node.dir_count).sum()
}

fn resolve_link_target(path: &Path) -> Option<String> {
    let resolved = if let Ok(target) = fs::read_link(path) {
        if target.is_absolute() {
            target
        } else if let Some(parent) = path.parent() {
            parent.join(&target)
        } else {
            target
        }
    } else {
        let canonical = fs::canonicalize(path).ok()?;
        let original = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(path)
        };

        if canonical == original {
            return None;
        }

        canonical
    };

    Some(resolved.to_string_lossy().to_string())
}

fn emit_progress(callback: Option<&ProgressCallback>, progress: ScanProgress) {
    if let Some(callback) = callback {
        callback(progress);
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
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

        let result = scan_path(
            &workspace.root,
            None,
            64,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
        .expect("expected NTFS MFT backend to run");

        assert_eq!(result.scan_backend.as_deref(), Some("mft_usn"));
        assert_eq!(result.total_files, 3);
        assert_eq!(result.total_dirs, 3);
        assert_eq!(result.total_size, "root-bytes".len() as u64 + 128 + "payload".len() as u64);

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

        let result = scan_path(
            &workspace.root,
            None,
            64,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
        .expect("expected NTFS MFT backend to run");

        let (Some(journal_id), Some(next_usn), Some(root_file_id)) =
            (result.usn_journal_id, result.usn_next_usn, result.root_file_id)
        else {
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
                .map(|changes| !changes.changed_dirs.is_empty() || changes.root_files_changed)
                .unwrap_or(false)
            {
                change_set = current;
                break;
            }

            std::thread::sleep(Duration::from_millis(200));
        }

        let change_set = change_set.expect("expected a USN delta result");
        assert!(
            change_set
                .changed_dirs
                .contains(&nested.to_string_lossy().to_string()),
            "expected nested directory to be marked changed, got {:?}",
            change_set.changed_dirs
        );
    }
}
