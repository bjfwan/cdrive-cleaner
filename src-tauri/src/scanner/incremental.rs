use super::disk_scanner::{DiskScanner, ScanProgressEmitter};
use super::file_info::{DirectoryNode, FileInfo, ScanResult};
use super::path_utils::{normalized_path_key, normalized_path_key_str, normalized_path_starts_with};
use super::progress::ScanProgress;
use super::timing::StageTimer;
use crate::winfs;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tauri::{AppHandle, Emitter, Runtime};

type IncrementalProgressEmitter = std::sync::Arc<dyn Fn(IncrementalScanProgress) + Send + Sync>;
const CLOCK_SKEW_TOLERANCE_SECS: u64 = 60;
const MERGE_CANCEL_CHECK_STRIDE: usize = 1024;
pub trait CancellationLike: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

impl CancellationLike for crate::session::CancellationToken {
    fn is_cancelled(&self) -> bool {
        crate::session::CancellationToken::is_cancelled(self)
    }
}
fn wall_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
#[derive(Debug)]
pub enum IncrementalScanError {
    Cancelled,
}

impl std::fmt::Display for IncrementalScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IncrementalScanError::Cancelled => write!(f, "增量扫描已取消"),
        }
    }
}

impl std::error::Error for IncrementalScanError {}

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeStatus {
    Unchanged,
    Modified,
    Deleted,
    New,
}

#[derive(Debug, Clone)]
pub struct ChangedDirectory {
    pub path: PathBuf,
    pub status: ChangeStatus,
    pub mode: RescanMode,
}

#[derive(Debug, Clone)]
struct RescannedDirectory {
    node: DirectoryNode,
    large_files: Vec<FileInfo>,
    mode: RescanMode,
    own_size: u64,
    own_file_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescanMode {
    Recursive,
    DirectFilesOnly,
}

#[derive(Clone, serde::Serialize)]
pub struct IncrementalScanProgress {
    pub phase: String,
    pub total_dirs: usize,
    pub checked_dirs: usize,
    pub changed_dirs: usize,
    pub scanned_dirs: usize,
}

#[derive(Debug, Default)]
pub struct TreeMergeHealth {
    pub unique_dir_nodes: usize,
    pub duplicate_paths: usize,
    pub orphan_root_count: usize,
    pub orphan_root_samples: Vec<String>,
    pub inconsistent_node_count: usize,
    pub inconsistent_node_samples: Vec<String>,
}

#[cfg(windows)]
fn is_link_entry(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_entry(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn path_points_to_directory(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
}

#[cfg(windows)]
fn get_disk_used_bytes(path: &Path) -> Option<u64> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

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

fn path_matches(node_path: &str, other: &Path) -> bool {
    normalized_path_key_str(node_path) == normalized_path_key(other)
}

fn path_starts_with(candidate: &Path, prefix: &Path) -> bool {
    let candidate_key = normalized_path_key(candidate);
    let prefix_key = normalized_path_key(prefix);
    normalized_path_starts_with(&candidate_key, &prefix_key)
}

#[allow(dead_code)]
fn path_starts_with_str(candidate: &str, prefix: &Path) -> bool {
    path_starts_with(Path::new(candidate), prefix)
}

pub fn check_directory_changes(cached_node: &DirectoryNode, current_path: &Path) -> ChangeStatus {
    check_directory_changes_with_clock(cached_node, current_path, wall_now_secs())
}
pub fn check_directory_changes_with_clock(
    cached_node: &DirectoryNode,
    current_path: &Path,
    wall_now: u64,
) -> ChangeStatus {
    let current_metadata = match std::fs::symlink_metadata(current_path) {
        Ok(m) => m,
        Err(_) => return ChangeStatus::Deleted,
    };

    let is_directory = if is_link_entry(&current_metadata) {
        path_points_to_directory(current_path)
    } else {
        current_metadata.is_dir()
    };

    if !is_directory {
        return ChangeStatus::Deleted;
    }

    let current_modified = current_metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    if let Some(current_time) = current_modified {
        if current_time > wall_now.saturating_add(CLOCK_SKEW_TOLERANCE_SECS) {
            return ChangeStatus::Modified;
        }
    }

    match (cached_node.modified_time, current_modified) {
        (Some(cached_time), Some(current_time)) => {
            if cached_time == current_time
                || (current_time < cached_time
                    && cached_time - current_time < CLOCK_SKEW_TOLERANCE_SECS)
            {
                ChangeStatus::Unchanged
            } else {
                ChangeStatus::Modified
            }
        }
        _ => ChangeStatus::Modified,
    }
}

pub fn detect_changes_recursive(
    cached_tree: &[DirectoryNode],
    current_path: &Path,
) -> Vec<ChangedDirectory> {
    detect_changes_recursive_with_clock(cached_tree, current_path, wall_now_secs())
}

pub fn detect_changes_recursive_with_clock(
    cached_tree: &[DirectoryNode],
    current_path: &Path,
    wall_now: u64,
) -> Vec<ChangedDirectory> {
    let cached_nodes: Vec<&DirectoryNode> = cached_tree
        .iter()
        .filter(|node| !path_matches(&node.path, current_path))
        .collect();
    let cached_map: HashMap<String, &DirectoryNode> = cached_nodes
        .iter()
        .map(|node| (normalized_path_key_str(&node.path), *node))
        .collect();
    let mut current_dirs = HashSet::new();
    let mut changes = Vec::new();

    if let Ok(entries) = std::fs::read_dir(current_path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&entry_path) else {
                continue;
            };
            let is_directory = if is_link_entry(&metadata) {
                path_points_to_directory(&entry_path)
            } else {
                metadata.is_dir()
            };

            if !is_directory {
                continue;
            }

            let entry_key = normalized_path_key(&entry_path);
            current_dirs.insert(entry_key.clone());

            if let Some(node) = cached_map.get(&entry_key) {
                match check_directory_changes_with_clock(node, &entry_path, wall_now) {
                    ChangeStatus::Deleted => changes.push(ChangedDirectory {
                        path: entry_path,
                        status: ChangeStatus::Deleted,
                        mode: RescanMode::Recursive,
                    }),
                    ChangeStatus::Modified => changes.push(ChangedDirectory {
                        path: entry_path,
                        status: ChangeStatus::Modified,
                        mode: RescanMode::Recursive,
                    }),
                    ChangeStatus::Unchanged => {
                        changes.extend(detect_changes_recursive_with_clock(
                            &node.children,
                            &entry_path,
                            wall_now,
                        ));
                    }
                    ChangeStatus::New => {}
                }
            } else {
                changes.push(ChangedDirectory {
                    path: entry_path,
                    status: ChangeStatus::New,
                    mode: RescanMode::Recursive,
                });
            }
        }
    }

    for node in cached_nodes {
        if !current_dirs.contains(&normalized_path_key_str(&node.path)) {
            changes.push(ChangedDirectory {
                path: PathBuf::from(&node.path),
                status: ChangeStatus::Deleted,
                mode: RescanMode::Recursive,
            });
        }
    }

    changes
}

fn build_file_id_index(nodes: &[DirectoryNode], map: &mut HashMap<u64, String>) {
    for node in nodes {
        if let Some(file_id) = node.file_id {
            map.insert(file_id, node.path.clone());
        }
        build_file_id_index(&node.children, map);
    }
}

fn normalize_recursive_changed_dirs(root_path: &Path, candidates: HashSet<String>) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = candidates
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| path != root_path)
        .filter(|path| path_starts_with(path, root_path))
        .collect();

    paths.sort_by(|a, b| {
        b.components()
            .count()
            .cmp(&a.components().count())
            .then_with(|| a.cmp(b))
    });

    let mut normalized: Vec<PathBuf> = Vec::new();
    for path in paths {
        if normalized
            .iter()
            .any(|existing| path_starts_with(existing, &path))
        {
            continue;
        }
        normalized.push(path);
    }
    normalized
}

fn collect_direct_file_changed_dirs(root_path: &Path, candidates: HashSet<String>) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = candidates
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| path != root_path)
        .filter(|path| path_starts_with(path, root_path))
        .collect();

    paths.sort_by(|a, b| {
        a.components()
            .count()
            .cmp(&b.components().count())
            .then_with(|| a.cmp(b))
    });

    paths
}

/// `detect_changes_via_usn` 的结果摘要。
///
/// - `Available`：USN 拿到了完整的变化集，调用方按 changes / renames 走"快路径"。
/// - `JournalReset`：USN journal_id 变了或 first_usn 越过我们的 checkpoint，
///   走"USN 全量重建"路径——从 first_usn 起重读，把所有目录加入候选；如果
///   USN 全量重建仍拿不到东西，再退化到 mtime。
/// - `Unavailable`：没有 USN 路径可用，走 mtime 全量。
pub enum UsnDetection {
    Available {
        changes: Vec<ChangedDirectory>,
        root_files_changed: bool,
        renames: Vec<winfs::UsnRenameEvent>,
    },
    JournalReset {
        first_usn: i64,
        new_journal_id: u64,
    },
    Unavailable,
}

fn detect_changes_via_usn_outcome(path: &Path, cached_result: &ScanResult) -> UsnDetection {
    let total_timer = StageTimer::start(
        "incremental-usn",
        format!("detect_changes_via_usn path={}", path.display()),
    );
    let checkpoint = match (cached_result.usn_journal_id, cached_result.usn_next_usn) {
        (Some(journal_id), Some(next_usn)) => winfs::UsnJournalCheckpoint {
            journal_id,
            next_usn,
        },
        _ => {
            total_timer.finish_with("status=missing_checkpoint");
            return UsnDetection::Unavailable;
        }
    };

    let file_id_map_timer = StageTimer::start(
        "incremental-usn",
        format!("build_file_id_map path={}", path.display()),
    );
    let mut file_id_map = HashMap::new();
    if let Some(root_file_id) = cached_result.root_file_id {
        file_id_map.insert(root_file_id, path.to_string_lossy().to_string());
    }
    build_file_id_index(&cached_result.directories, &mut file_id_map);
    file_id_map_timer.finish_with(format!("entries={}", file_id_map.len()));

    let usn_collect_timer = StageTimer::start(
        "incremental-usn",
        format!("read_usn_journal_with_retry path={}", path.display()),
    );
    let change_set = match winfs_read_usn(path, checkpoint, cached_result.root_file_id, &file_id_map)
    {
        UsnReadShim::Records(set) => {
            usn_collect_timer.finish_with(format!(
                "recursive_dirs={} direct_file_dirs={} root_files_changed={} renames={}",
                set.recursive_dirs.len(),
                set.direct_file_dirs.len(),
                set.root_files_changed,
                set.renames.len()
            ));
            set
        }
        UsnReadShim::JournalReset {
            new_journal_id,
            first_usn,
        } => {
            usn_collect_timer.finish_with(format!(
                "status=journal_reset new_journal_id={} first_usn={}",
                new_journal_id, first_usn
            ));
            total_timer.finish_with(format!(
                "status=journal_reset new_journal_id={} first_usn={}",
                new_journal_id, first_usn
            ));
            tracing::info!(
                "[阶段1] USN journal 已被重建/回卷 path={} reason=usn_journal_reset new_journal_id={} first_usn={}",
                path.display(),
                new_journal_id,
                first_usn
            );
            return UsnDetection::JournalReset {
                first_usn,
                new_journal_id,
            };
        }
        UsnReadShim::StartUsnTooOld { first_usn } => {
            usn_collect_timer.finish_with(format!("status=start_usn_too_old first_usn={first_usn}"));
            total_timer.finish_with(format!("status=start_usn_too_old first_usn={first_usn}"));
            tracing::info!(
                "[阶段1] USN checkpoint 落后被回卷 path={} reason=usn_journal_reset first_usn={}",
                path.display(),
                first_usn
            );
            return UsnDetection::JournalReset {
                first_usn,
                new_journal_id: checkpoint.journal_id,
            };
        }
        UsnReadShim::HardError(err) => {
            usn_collect_timer.finish_with(format!("status=error err={err}"));
            total_timer.finish_with(format!("status=error err={err}"));
            return UsnDetection::Unavailable;
        }
    };

    let normalize_timer = StageTimer::start(
        "incremental-usn",
        format!("normalize_changed_dirs path={}", path.display()),
    );
    let recursive_paths = normalize_recursive_changed_dirs(path, change_set.recursive_dirs);
    let recursive_keys: HashSet<String> = recursive_paths
        .iter()
        .map(|candidate| normalized_path_key(candidate))
        .collect();

    let mut changes: Vec<ChangedDirectory> = recursive_paths
        .into_iter()
        .map(|candidate| ChangedDirectory {
            status: if candidate.exists() {
                ChangeStatus::Modified
            } else {
                ChangeStatus::Deleted
            },
            path: candidate,
            mode: RescanMode::Recursive,
        })
        .collect();

    // DirectFilesOnly 候选去重：祖先已经走 Recursive 重扫的，自动 dedupe 掉，
    // 否则会出现「父被替换为最新子树、子又把 clone 出来的旧 children 覆盖回去」
    // 的覆盖竞态。
    changes.extend(
        collect_direct_file_changed_dirs(path, change_set.direct_file_dirs)
            .into_iter()
            .filter(|candidate| {
                !recursive_keys
                    .iter()
                    .any(|existing| path_starts_with(candidate, Path::new(existing)))
            })
            .map(|candidate| ChangedDirectory {
                status: if candidate.exists() {
                    ChangeStatus::Modified
                } else {
                    ChangeStatus::Deleted
                },
                path: candidate,
                mode: RescanMode::DirectFilesOnly,
            }),
    );
    normalize_timer.finish_with(format!(
        "recursive_paths={} total_changes={}",
        recursive_keys.len(),
        changes.len() + usize::from(change_set.root_files_changed)
    ));
    total_timer.finish_with(format!(
        "status=complete total_changes={} root_files_changed={} renames={}",
        changes.len() + usize::from(change_set.root_files_changed),
        change_set.root_files_changed,
        change_set.renames.len()
    ));

    UsnDetection::Available {
        changes,
        root_files_changed: change_set.root_files_changed,
        renames: change_set.renames,
    }
}

/// 平台抽象：windows 走真实 USN，其它平台立刻返回 HardError，让上层走 mtime。
enum UsnReadShim {
    Records(winfs::UsnChangeSet),
    JournalReset { new_journal_id: u64, first_usn: i64 },
    StartUsnTooOld { first_usn: i64 },
    HardError(std::io::Error),
}

#[cfg(windows)]
fn winfs_read_usn(
    path: &Path,
    checkpoint: winfs::UsnJournalCheckpoint,
    root_file_id: Option<u64>,
    file_id_map: &HashMap<u64, String>,
) -> UsnReadShim {
    match winfs::read_usn_journal_with_retry(path, checkpoint, root_file_id, file_id_map) {
        winfs::UsnReadOutcome::Records(set) => UsnReadShim::Records(set),
        winfs::UsnReadOutcome::JournalReset {
            new_journal_id,
            first_usn,
        } => UsnReadShim::JournalReset {
            new_journal_id,
            first_usn,
        },
        winfs::UsnReadOutcome::StartUsnTooOld { first_usn } => {
            UsnReadShim::StartUsnTooOld { first_usn }
        }
        winfs::UsnReadOutcome::HardError(err) => UsnReadShim::HardError(err),
    }
}

#[cfg(not(windows))]
fn winfs_read_usn(
    _path: &Path,
    _checkpoint: winfs::UsnJournalCheckpoint,
    _root_file_id: Option<u64>,
    _file_id_map: &HashMap<u64, String>,
) -> UsnReadShim {
    UsnReadShim::HardError(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "USN journal is only available on Windows NTFS volumes",
    ))
}

/// 把整棵 cached 树的所有目录路径作为 USN 全量重建的"重扫候选"。
/// 用于 USN journal_id 变了 / checkpoint 被回卷的情况——我们不知道具体哪些
/// 子树变化了，但又不想退化到 mtime 全量递归。把缓存里所有目录都丢回阶段 2，
/// 让阶段 2 按 Recursive 模式重扫；阶段 1 的去重逻辑只会留下根级别的 Recursive，
/// 实际效果就是"以缓存为骨架做一次 USN 全量重建"。
pub fn collect_all_cached_dirs_as_recursive(
    cached_tree: &[DirectoryNode],
    root_path: &Path,
) -> Vec<ChangedDirectory> {
    fn walk(
        nodes: &[DirectoryNode],
        root_path: &Path,
        out: &mut Vec<ChangedDirectory>,
    ) {
        for node in nodes {
            let pb = PathBuf::from(&node.path);
            if pb != root_path && path_starts_with(&pb, root_path) {
                out.push(ChangedDirectory {
                    path: pb,
                    status: ChangeStatus::Modified,
                    mode: RescanMode::Recursive,
                });
            }
            walk(&node.children, root_path, out);
        }
    }
    let mut out = Vec::new();
    walk(cached_tree, root_path, &mut out);
    // 阶段 2 会按"祖先 Recursive 已经覆盖 → 后代不再单独重扫"去重；这里
    // 直接交付完整列表即可，让上层 normalize 路径做收敛。
    out
}

fn build_file_info(path: &Path, metadata: &std::fs::Metadata) -> FileInfo {
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();

    FileInfo {
        path: path.to_string_lossy().to_string(),
        name: path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string(),
        size: metadata.len(),
        extension: path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string(),
        modified_at,
        is_readonly: metadata.permissions().readonly(),
        is_symlink: false,
        link_target: None,
    }
}

fn sort_directory_tree(nodes: &mut [DirectoryNode]) {
    for node in nodes.iter_mut() {
        sort_directory_tree(&mut node.children);
        node.has_children = !node.children.is_empty();
    }
    nodes.sort_by_key(|n| std::cmp::Reverse(n.size));
}

pub(crate) fn scan_root_files(
    path: &Path,
    large_file_threshold: u64,
) -> (u64, usize, Vec<FileInfo>) {
    let started = std::time::Instant::now();
    let mut total_size = 0u64;
    let mut total_files = 0usize;
    let mut large_files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&entry_path) else {
                continue;
            };
            if is_link_entry(&metadata) {
                continue;
            }
            if !metadata.is_file() {
                continue;
            }

            total_size += metadata.len();
            total_files += 1;

            if metadata.len() >= large_file_threshold {
                large_files.push(build_file_info(&entry_path, &metadata));
            }
        }
    }

    large_files.sort_by_key(|f| std::cmp::Reverse(f.size));
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    if elapsed_ms >= 100.0 {
        tracing::debug!("[scan-timing][incremental] scan_root_files path={} took {:.2}ms | files={} size={} large_files={}",
            path.display(),
            elapsed_ms,
            total_files,
            total_size,
            large_files.len()
        );
    }
    (total_size, total_files, large_files)
}

fn index_cached_nodes<'a>(
    nodes: &'a [DirectoryNode],
    map: &mut HashMap<String, &'a DirectoryNode>,
) {
    for node in nodes {
        if node.dir_count > 0 {
            map.insert(normalized_path_key_str(&node.path), node);
        }
        index_cached_nodes(&node.children, map);
    }
}

/// 重新扫描单个目录，返回更新后的 DirectoryNode
fn rescan_directory(path: &Path, large_file_threshold: u64) -> Option<RescannedDirectory> {
    rescan_directory_tree(path, large_file_threshold).map(|mut result| {
        result.mode = RescanMode::Recursive;
        result
    })
}

pub(crate) fn rescan_directory_snapshot(
    path: &Path,
    large_file_threshold: u64,
) -> Option<(DirectoryNode, Vec<FileInfo>, u64, usize)> {
    rescan_directory_tree(path, large_file_threshold)
        .map(|result| (result.node, result.large_files, result.own_size, result.own_file_count))
}

fn refresh_directory_direct_files(
    path: &Path,
    cached_node: &DirectoryNode,
    large_file_threshold: u64,
) -> Option<RescannedDirectory> {
    let root_metadata = std::fs::symlink_metadata(path).ok()?;
    if is_link_entry(&root_metadata) || !root_metadata.is_dir() {
        return None;
    }

    let modified_time = root_metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let (direct_size, direct_files, large_files) = scan_root_files(path, large_file_threshold);
    let child_size: u64 = cached_node.children.iter().map(|child| child.size).sum();
    let child_files: usize = cached_node
        .children
        .iter()
        .map(|child| child.file_count)
        .sum();
    let child_dirs: usize = cached_node
        .children
        .iter()
        .map(|child| child.dir_count)
        .sum();

    let mut node = cached_node.clone();
    node.size = child_size + direct_size;
    node.file_count = child_files + direct_files;
    node.dir_count = 1 + child_dirs;
    node.modified_time = modified_time;
    node.file_id = winfs::get_path_file_id(path).or(node.file_id);
    node.has_children = !node.children.is_empty();

    Some(RescannedDirectory {
        node,
        large_files,
        mode: RescanMode::DirectFilesOnly,
        own_size: direct_size,
        own_file_count: direct_files,
    })
}

fn rescan_directory_tree(path: &Path, large_file_threshold: u64) -> Option<RescannedDirectory> {
    let root_metadata = std::fs::symlink_metadata(path).ok()?;
    if is_link_entry(&root_metadata) {
        if !path_points_to_directory(path) {
            return None;
        }

        let modified_time = root_metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        return Some(RescannedDirectory {
            node: DirectoryNode {
                path: path.to_string_lossy().to_string(),
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                size: 0,
                file_count: 0,
                dir_count: 1,
                children: vec![],
                has_children: false,
                is_symlink: true,
                link_target: winfs::resolve_link_target(path),
                safety: None,
                modified_time,
                file_id: None,
            },
            large_files: vec![],
            mode: RescanMode::Recursive,
            own_size: 0,
            own_file_count: 0,
        });
    }

    if !root_metadata.is_dir() {
        return None;
    }

    let mut dir_file_stats: HashMap<PathBuf, (u64, usize)> = HashMap::new();
    let mut dir_nodes: HashMap<PathBuf, DirectoryNode> = HashMap::new();
    let mut large_files = Vec::new();
    let root_modified_time = root_metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    dir_nodes.insert(
        path.to_path_buf(),
        DirectoryNode {
            path: path.to_string_lossy().to_string(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            size: 0,
            file_count: 0,
            dir_count: 1,
            children: vec![],
            has_children: false,
            is_symlink: false,
            link_target: None,
            safety: None,
            modified_time: root_modified_time,
            file_id: winfs::get_path_file_id(path),
        },
    );

    let mut pending_dirs = vec![path.to_path_buf()];
    while let Some(current_dir) = pending_dirs.pop() {
        let entries = match winfs::enumerate_directory(&current_dir, true) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries {
            if entry.is_symlink {
                if entry.is_dir {
                    dir_nodes.insert(
                        entry.path.clone(),
                        DirectoryNode {
                            path: entry.path.to_string_lossy().to_string(),
                            name: entry.name,
                            size: 0,
                            file_count: 0,
                            dir_count: 1,
                            children: vec![],
                            has_children: false,
                            is_symlink: true,
                            link_target: winfs::resolve_link_target(&entry.path),
                            safety: None,
                            modified_time: entry.modified_time,
                            file_id: None,
                        },
                    );
                }
                continue;
            }

            if entry.is_dir {
                pending_dirs.push(entry.path.clone());
                dir_nodes.insert(
                    entry.path.clone(),
                    DirectoryNode {
                        path: entry.path.to_string_lossy().to_string(),
                        name: entry.name,
                        size: 0,
                        file_count: 0,
                        dir_count: 1,
                        children: vec![],
                        has_children: false,
                        is_symlink: false,
                        link_target: None,
                        safety: None,
                        modified_time: entry.modified_time,
                        file_id: entry.file_id,
                    },
                );
                continue;
            }

            if let Some(parent) = entry.path.parent() {
                let stats = dir_file_stats.entry(parent.to_path_buf()).or_insert((0, 0));
                stats.0 += entry.size;
                stats.1 += 1;
            }
            if entry.size >= large_file_threshold {
                let modified_at = entry
                    .modified_time
                    .and_then(|secs| chrono::DateTime::from_timestamp(secs as i64, 0))
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default();
                large_files.push(FileInfo {
                    path: entry.path.to_string_lossy().to_string(),
                    name: entry.name,
                    size: entry.size,
                    extension: entry
                        .path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_string(),
                    modified_at,
                    is_readonly: entry.is_readonly,
                    is_symlink: false,
                    link_target: None,
                });
            }
        }
    }

    for (dir_path, (size, files)) in &dir_file_stats {
        if let Some(node) = dir_nodes.get_mut(dir_path) {
            node.size = *size;
            node.file_count = *files;
        }
    }

    // 在把子目录大小累加上来之前，先把"根目录直接文件"的统计冻结下来——
    // 它就是阶段 3 权威回填里需要的 own_size / own_file_count，不能再用
    // node.size - Σchild.size 反算（中间过程会被父子覆盖污染）。
    let (root_own_size, root_own_files) = dir_file_stats
        .get(path)
        .copied()
        .unwrap_or((0u64, 0usize));

    let mut all_paths: Vec<PathBuf> = dir_nodes.keys().cloned().collect();
    all_paths.sort_by_key(|p| std::cmp::Reverse(p.components().count()));

    for dir_path in &all_paths {
        if dir_path == path {
            continue;
        }
        if let Some(parent_path) = dir_path.parent() {
            if let Some(child) = dir_nodes.get(dir_path) {
                let child_size = child.size;
                let child_files = child.file_count;
                let child_dirs = child.dir_count;
                if let Some(parent) = dir_nodes.get_mut(parent_path) {
                    parent.size += child_size;
                    parent.file_count += child_files;
                    parent.dir_count += child_dirs;
                }
            }
        }
    }

    for dir_path in &all_paths {
        if dir_path == path {
            continue;
        }
        if let Some(parent_path) = dir_path.parent() {
            if let Some(child_node) = dir_nodes.remove(dir_path) {
                if let Some(parent_node) = dir_nodes.get_mut(parent_path) {
                    parent_node.children.push(child_node);
                }
            }
        }
    }

    let mut node = dir_nodes.remove(path)?;
    sort_directory_tree(&mut node.children);
    large_files.sort_by_key(|f| std::cmp::Reverse(f.size));

    Some(RescannedDirectory {
        node,
        large_files,
        mode: RescanMode::Recursive,
        own_size: root_own_size,
        own_file_count: root_own_files,
    })
}

pub async fn scan_incremental<R: Runtime>(
    path: &Path,
    cached_result: ScanResult,
    app: AppHandle<R>,
    scanner: &DiskScanner,
    cancellation: Option<&crate::session::CancellationToken>,
) -> Result<ScanResult> {
    let incremental_app = app.clone();
    let progress_emitter: IncrementalProgressEmitter =
        std::sync::Arc::new(move |progress: IncrementalScanProgress| {
            let _ = incremental_app.emit("incremental-scan-progress", progress);
        });
    let deep_progress_emitter: ScanProgressEmitter =
        std::sync::Arc::new(move |progress: ScanProgress| {
            let _ = app.emit("deep-scan-progress", progress);
        });
    scan_incremental_internal(
        path,
        cached_result,
        scanner,
        Some(progress_emitter),
        Some(deep_progress_emitter),
        cancellation.cloned(),
    )
    .await
}

pub async fn scan_incremental_silent(
    path: &Path,
    cached_result: ScanResult,
    scanner: &DiskScanner,
) -> Result<ScanResult> {
    scan_incremental_internal(path, cached_result, scanner, None, None, None).await
}

/// 与 `scan_incremental_silent` 一致，但额外接受一个 `CancellationToken`。
/// 任务 B / C 的集成测试和命令层"无 UI 调用增量"的路径都用这个入口。
pub async fn scan_incremental_with_token(
    path: &Path,
    cached_result: ScanResult,
    scanner: &DiskScanner,
    cancellation: Option<&crate::session::CancellationToken>,
) -> Result<ScanResult> {
    scan_incremental_internal(
        path,
        cached_result,
        scanner,
        None,
        None,
        cancellation.cloned(),
    )
    .await
}

fn emit_incremental_progress(
    progress_emitter: Option<&IncrementalProgressEmitter>,
    progress: IncrementalScanProgress,
) {
    if let Some(emitter) = progress_emitter {
        emitter(progress);
    }
}

fn emit_deep_scan_progress(
    deep_progress_emitter: Option<&ScanProgressEmitter>,
    progress: ScanProgress,
) {
    if let Some(emitter) = deep_progress_emitter {
        emitter(progress);
    }
}

async fn fallback_to_full_scan(
    path: &Path,
    scanner: &DiskScanner,
    estimated_files: usize,
    deep_progress_emitter: Option<&ScanProgressEmitter>,
    reason: &str,
    cancellation: Option<&crate::session::CancellationToken>,
) -> Result<ScanResult> {
    let fallback_timer = StageTimer::start(
        "incremental",
        format!(
            "fallback_to_full_scan path={} reason={reason}",
            path.display()
        ),
    );
    tracing::info!("[增量->全量] {reason}");
    emit_deep_scan_progress(
        deep_progress_emitter,
        ScanProgress {
            scanned_files: 0,
            scanned_dirs: 0,
            total_size: 0,
            current_path: format!("切换到全量深度扫描: {reason}"),
            elapsed_ms: 0,
            files_per_second: 0.0,
            progress_percent: 0.0,
        },
    );
    let result = scanner
        .scan_deep_with_progress_and_token(
            path,
            deep_progress_emitter.cloned(),
            estimated_files,
            cancellation,
        )
        .await;
    match &result {
        Ok(result) => fallback_timer.finish_with(format!(
            "backend={:?} files={} dirs={} size={} inaccessible={}",
            result.scan_backend,
            result.total_files,
            result.total_dirs,
            result.total_size,
            result.inaccessible_count
        )),
        Err(err) => fallback_timer.finish_with(format!("status=error err={err}")),
    }
    result
}

async fn scan_incremental_internal(
    path: &Path,
    cached_result: ScanResult,
    scanner: &DiskScanner,
    progress_emitter: Option<IncrementalProgressEmitter>,
    deep_progress_emitter: Option<ScanProgressEmitter>,
    cancellation: Option<crate::session::CancellationToken>,
) -> Result<ScanResult> {
    use std::time::Instant;

    let start = Instant::now();
    let total_timer = StageTimer::start(
        "incremental",
        format!("scan_incremental path={}", path.display()),
    );
    let large_file_threshold = 100 * 1024 * 1024u64;
    let root_path_str = path.to_string_lossy().to_string();

    tracing::info!("\n========== 增量扫描开始 ==========");
    tracing::info!("扫描路径: {}", path.display());
    tracing::info!(
        "[阶段0] 缓存摘要 backend={:?} files={} dirs={} size={} root_file_id={:?} usn_journal_id={:?} usn_next_usn={:?}",
        cached_result.scan_backend,
        cached_result.total_files,
        cached_result.total_dirs,
        cached_result.total_size,
        cached_result.root_file_id,
        cached_result.usn_journal_id,
        cached_result.usn_next_usn
    );

    let cached_count_timer = StageTimer::start(
        "incremental",
        format!("count_cached_directories path={}", path.display()),
    );
    let total_cached_dirs = count_directories(&cached_result.directories, &root_path_str);
    cached_count_timer.finish_with(format!("cached_dirs={}", total_cached_dirs));
    tracing::info!("[阶段1] 缓存目录总数: {}", total_cached_dirs);

    emit_incremental_progress(
        progress_emitter.as_ref(),
        IncrementalScanProgress {
            phase: "detecting".to_string(),
            total_dirs: total_cached_dirs,
            checked_dirs: 0,
            changed_dirs: 0,
            scanned_dirs: 0,
        },
    );

    // 阶段1: 检测变化
    let detect_timer = StageTimer::start(
        "incremental",
        format!("stage1_detect_changes path={}", path.display()),
    );
    let detect_start = Instant::now();
    let mut detected_renames: Vec<winfs::UsnRenameEvent> = Vec::new();
    let (changes, mut root_files_changed, detection_mode) =
        match detect_changes_via_usn_outcome(path, &cached_result) {
            UsnDetection::Available {
                changes,
                root_files_changed,
                renames,
            } => {
                detected_renames = renames;
                (changes, root_files_changed, "usn")
            }
            UsnDetection::JournalReset {
                first_usn,
                new_journal_id,
            } => {
                tracing::info!(
                    "[阶段1] reason=usn_journal_reset first_usn={} new_journal_id={} → 走 USN 全量重建",
                    first_usn,
                    new_journal_id
                );
                let candidates =
                    collect_all_cached_dirs_as_recursive(&cached_result.directories, path);
                if candidates.is_empty() {
                    // 缓存里啥也没有：那就只能 mtime 全量了。但我们仍然把
                    // strategy 标签写到 tracing，方便排查。
                    let (current_root_size, current_root_files, _) =
                        scan_root_files(path, large_file_threshold);
                    let (cached_root_size, cached_root_files) = cached_result
                        .directories
                        .iter()
                        .find(|node| path_matches(&node.path, path))
                        .map(|node| (node.size, node.file_count))
                        .unwrap_or((0, 0));
                    let rfc = cached_root_size != current_root_size
                        || cached_root_files != current_root_files;
                    (
                        detect_changes_recursive(&cached_result.directories, path),
                        rfc,
                        "usn_full_rebuild_fallback_mtime",
                    )
                } else {
                    (candidates, true, "usn_full_rebuild")
                }
            }
            UsnDetection::Unavailable => {
                let (current_root_size, current_root_files, _) =
                    scan_root_files(path, large_file_threshold);
                let (cached_root_size, cached_root_files) = cached_result
                    .directories
                    .iter()
                    .find(|node| path_matches(&node.path, path))
                    .map(|node| (node.size, node.file_count))
                    .unwrap_or((0, 0));
                let rfc = cached_root_size != current_root_size
                    || cached_root_files != current_root_files;
                (
                    detect_changes_recursive(&cached_result.directories, path),
                    rfc,
                    "mtime",
                )
            }
        };
    let (current_root_size, current_root_files, root_large_files) =
        scan_root_files(path, large_file_threshold);
    let (cached_root_size, cached_root_files) = cached_result
        .directories
        .iter()
        .find(|node| path_matches(&node.path, path))
        .map(|node| (node.size, node.file_count))
        .unwrap_or((0, 0));
    root_files_changed = root_files_changed
        || cached_root_size != current_root_size
        || cached_root_files != current_root_files;
    tracing::info!(
        "[阶段1] 变化检测完成，耗时: {:.2}ms | mode={} renames={}",
        detect_start.elapsed().as_secs_f64() * 1000.0,
        detection_mode,
        detected_renames.len()
    );
    match detection_mode {
        "mtime" => {
            if cached_result.usn_journal_id.is_some() && cached_result.usn_next_usn.is_some() {
                tracing::info!("[阶段1] USN checkpoint 存在，但本次未能直接使用，已回退到 mtime 递归检测");
            } else {
                tracing::info!("[阶段1] 缓存缺少 USN checkpoint，本次只能使用 mtime 递归检测");
            }
        }
        "usn_full_rebuild" => {
            tracing::info!(
                "[阶段1] 走 USN 全量重建路径，本次按 Recursive 重扫整个缓存范围"
            );
        }
        "usn_full_rebuild_fallback_mtime" => {
            tracing::info!(
                "[阶段1] 缓存为空，USN 全量重建退化到 mtime 全量递归"
            );
        }
        _ => {
            tracing::info!("[阶段1] 本次增量检测使用了 USN 日志");
        }
    }

    let change_dirs = changes
        .iter()
        .filter(|c| c.status != ChangeStatus::Deleted)
        .count();
    let delete_dirs = changes
        .iter()
        .filter(|c| c.status == ChangeStatus::Deleted)
        .count();
    let total_changes = changes.len() + usize::from(root_files_changed);
    detect_timer.finish_with(format!(
        "mode={} total_changes={} rescan_dirs={} deleted_dirs={} root_files_changed={}",
        detection_mode, total_changes, change_dirs, delete_dirs, root_files_changed
    ));
    tracing::info!(
        "[阶段1] 总变化: {} 个 (重扫: {}, 删除: {}, 根文件变化: {})",
        total_changes, change_dirs, delete_dirs, root_files_changed
    );
    emit_incremental_progress(
        progress_emitter.as_ref(),
        IncrementalScanProgress {
            phase: "detecting".to_string(),
            total_dirs: total_cached_dirs,
            checked_dirs: total_cached_dirs,
            changed_dirs: total_changes,
            scanned_dirs: 0,
        },
    );

    let index_timer = StageTimer::start(
        "incremental",
        format!("index_cached_nodes path={}", path.display()),
    );
    let mut cached_node_map = HashMap::new();
    index_cached_nodes(&cached_result.directories, &mut cached_node_map);
    index_timer.finish_with(format!("indexed_dirs={}", cached_node_map.len()));

    // 无变化，直接返回缓存
    if total_changes == 0 {
        tracing::info!("[结果] 无变化，直接使用缓存");
        emit_incremental_progress(
            progress_emitter.as_ref(),
            IncrementalScanProgress {
                phase: "completed".to_string(),
                total_dirs: total_cached_dirs,
                checked_dirs: total_cached_dirs,
                changed_dirs: 0,
                scanned_dirs: 0,
            },
        );
        tracing::info!(
            "========== 增量扫描完成 (耗时: {:.2}ms) ==========\n",
            start.elapsed().as_secs_f64() * 1000.0
        );
        total_timer.finish_with(format!(
            "status=no_changes files={} dirs={} size={}",
            cached_result.total_files, cached_result.total_dirs, cached_result.total_size
        ));
        return Ok(cached_result);
    }

    // 变化过多，回退到全量扫描
    let change_ratio = total_changes as f64 / (total_cached_dirs as f64).max(1.0);
    tracing::info!(
        "[阶段1] 变化比例: {:.2}% ({} / {})",
        change_ratio * 100.0,
        total_changes,
        total_cached_dirs.max(1)
    );
    if change_ratio > 0.3 {
        tracing::info!(
            "[阶段2] 变化超过30% ({:.1}%)，切换到全量扫描",
            change_ratio * 100.0
        );
        let result = fallback_to_full_scan(
            path,
            scanner,
            cached_result.total_files.max(1),
            deep_progress_emitter.as_ref(),
            &format!("变化比例 {:.1}% 超过 30% 阈值", change_ratio * 100.0),
            cancellation.as_ref(),
        )
        .await?;
        total_timer.finish_with(format!(
            "status=fallback_change_ratio backend={:?} files={} dirs={} size={}",
            result.scan_backend, result.total_files, result.total_dirs, result.total_size
        ));
        return Ok(result);
    }

    // 阶段2: 重新扫描修改过的目录
    if let Some(token) = cancellation.as_ref() {
        if token.is_cancelled() {
            total_timer.finish_with("status=cancelled stage=before_stage2");
            return Err(IncrementalScanError::Cancelled.into());
        }
    }
    let rescan_candidates: Vec<&ChangedDirectory> = changes
        .iter()
        .filter(|c| c.status != ChangeStatus::Deleted)
        .collect();
    let total_rescans = rescan_candidates.len() + usize::from(root_files_changed);
    tracing::info!(
        "[阶段2] 开始重扫 {} 个目录 | 删除 {} 个 | 根文件变化={}",
        rescan_candidates.len(),
        delete_dirs,
        root_files_changed
    );
    emit_incremental_progress(
        progress_emitter.as_ref(),
        IncrementalScanProgress {
            phase: "scanning".to_string(),
            total_dirs: total_rescans,
            checked_dirs: 0,
            changed_dirs: total_changes,
            scanned_dirs: 0,
        },
    );

    let deleted_paths: Vec<String> = changes
        .iter()
        .filter(|c| c.status == ChangeStatus::Deleted)
        .map(|c| c.path.to_string_lossy().to_string())
        .collect();

    // 重新扫描修改过的目录，得到最新数据
    let mut rescanned_dirs: Vec<RescannedDirectory> = Vec::new();
    let rescan_timer = StageTimer::start(
        "incremental",
        format!("stage2_rescan_changed_dirs path={}", path.display()),
    );
    let stage2_start = Instant::now();
    let mut last_stage2_progress_log = Instant::now();
    for (i, change) in rescan_candidates.iter().enumerate() {
        // 阶段 2 重扫循环：每完成一个目录检查一次取消，命中后立即返回。
        if let Some(token) = cancellation.as_ref() {
            if token.is_cancelled() {
                rescan_timer.finish_with(format!(
                    "status=cancelled completed={} of={}",
                    i,
                    rescan_candidates.len()
                ));
                total_timer.finish_with(format!(
                    "status=cancelled stage=stage2_rescan completed={}",
                    i
                ));
                return Err(IncrementalScanError::Cancelled.into());
            }
        }
        let rescan_start = Instant::now();
        let result = match change.mode {
            RescanMode::Recursive => rescan_directory(&change.path, large_file_threshold),
            RescanMode::DirectFilesOnly => cached_node_map
                .get(&normalized_path_key(&change.path))
                .and_then(|cached_node| {
                    refresh_directory_direct_files(&change.path, cached_node, large_file_threshold)
                }),
        };

        if let Some(node) = result {
            let elapsed_ms = rescan_start.elapsed().as_secs_f64() * 1000.0;
            if elapsed_ms >= 2000.0 {
                tracing::info!(
                    "[阶段2] 慢重扫 {:.2}ms mode={:?} path={}",
                    elapsed_ms,
                    change.mode,
                    change.path.display()
                );
            }
            rescanned_dirs.push(node);
        }
        emit_incremental_progress(
            progress_emitter.as_ref(),
            IncrementalScanProgress {
                phase: "scanning".to_string(),
                total_dirs: total_rescans,
                checked_dirs: i + 1,
                changed_dirs: total_changes,
                scanned_dirs: i + 1,
            },
        );

        let completed = i + 1;
        let should_log_progress = completed == 1
            || completed == rescan_candidates.len()
            || completed % 250 == 0
            || last_stage2_progress_log.elapsed().as_secs() >= 2;
        if should_log_progress {
            let pct = if rescan_candidates.is_empty() {
                100.0
            } else {
                completed as f64 / rescan_candidates.len() as f64 * 100.0
            };
            tracing::info!(
                "[阶段2] 重扫进度 {}/{} ({:.1}%) latest_mode={:?} path={}",
                completed,
                rescan_candidates.len(),
                pct,
                change.mode,
                change.path.display()
            );
            last_stage2_progress_log = Instant::now();
        }
    }

    if root_files_changed {
        tracing::info!("[阶段2] 根目录直接文件存在变化，已将根节点刷新纳入结果");
        emit_incremental_progress(
            progress_emitter.as_ref(),
            IncrementalScanProgress {
                phase: "scanning".to_string(),
                total_dirs: total_rescans,
                checked_dirs: total_rescans,
                changed_dirs: total_changes,
                scanned_dirs: total_rescans,
            },
        );
    }

    let recursive_rescans = rescanned_dirs
        .iter()
        .filter(|item| item.mode == RescanMode::Recursive)
        .count();
    let direct_only_rescans = rescanned_dirs
        .iter()
        .filter(|item| item.mode == RescanMode::DirectFilesOnly)
        .count();
    tracing::info!(
        "[阶段2] 重新扫描了 {} 个目录, 删除 {} 个 | recursive={} direct_only={} stage2_ms={:.2}",
        rescanned_dirs.len(),
        deleted_paths.len(),
        recursive_rescans,
        direct_only_rescans,
        stage2_start.elapsed().as_secs_f64() * 1000.0
    );
    rescan_timer.finish_with(format!(
        "rescanned_dirs={} deleted_paths={} recursive={} direct_only={} root_files_changed={}",
        rescanned_dirs.len(),
        deleted_paths.len(),
        recursive_rescans,
        direct_only_rescans,
        root_files_changed
    ));

    let ScanResult {
        root_path: cached_root_path,
        directories: cached_directories,
        large_files: cached_large_files,
        inaccessible_count: cached_inaccessible_count,
        root_file_id: cached_root_file_id,
        usn_journal_id: cached_usn_journal_id,
        usn_next_usn: cached_usn_next_usn,
        system_reserved_bytes: cached_system_reserved_bytes,
        total_files: cached_total_files,
        ..
    } = cached_result;

    // 阶段3: 合并数据
    let merge_timer = StageTimer::start(
        "incremental",
        format!("stage3_merge_tree path={}", path.display()),
    );
    let merge_start = Instant::now();
    emit_incremental_progress(
        progress_emitter.as_ref(),
        IncrementalScanProgress {
            phase: "merging".to_string(),
            total_dirs: total_changes,
            checked_dirs: total_changes,
            changed_dirs: total_changes,
            scanned_dirs: total_changes,
        },
    );

    let all_direct_only = rescanned_dirs
        .iter()
        .all(|item| item.mode == RescanMode::DirectFilesOnly);
    let merge_strategy = if deleted_paths.is_empty() && all_direct_only {
        "direct_only_in_place"
    } else {
        "full_tree_merge"
    };
    tracing::info!(
        "[阶段3] 开始合并 | strategy={} rescanned={} deleted={}",
        merge_strategy,
        rescanned_dirs.len(),
        deleted_paths.len()
    );

    let mut updated_tree = if deleted_paths.is_empty() && all_direct_only {
        apply_direct_file_refreshes(
            cached_directories,
            rescanned_dirs
                .iter()
                .map(|item| item.node.clone())
                .collect(),
            path,
        )
    } else {
        let merge_inner_timer = StageTimer::start(
            "incremental",
            format!("stage3_merge_inner path={}", path.display()),
        );
        let mut own_overrides: HashMap<String, (u64, usize)> =
            HashMap::with_capacity(rescanned_dirs.len());
        for item in &rescanned_dirs {
            own_overrides.insert(
                normalized_path_key_str(&item.node.path),
                (item.own_size, item.own_file_count),
            );
            // 阶段 3 合并循环：每隔一段检查一次取消信号。
            if let Some(token) = cancellation.as_ref() {
                if token.is_cancelled() {
                    return Err(IncrementalScanError::Cancelled.into());
                }
            }
        }

        // 重命名识别（best-effort）：USN 路径下，把 RENAME_OLD/NEW_NAME pair 起来
        // 后变成 (old_path, new_path)。在送进 keyed merge 之前，先把缓存树里 old_path
        // 的子树平移到 new_path 下，并把对应的 deleted_paths 干掉，让 keyed merge
        // 把 rename 看成"什么都没动"。识别失败时退回到原本的"删 old + 建 new"。
        let rescanned_keys: HashSet<String> = rescanned_dirs
            .iter()
            .map(|item| normalized_path_key_str(&item.node.path))
            .collect();
        let (deleted_paths, cached_directories, applied_renames) = apply_rename_pre_merge(
            cached_directories,
            deleted_paths,
            &detected_renames,
            &rescanned_keys,
        );
        if applied_renames > 0 {
            tracing::info!(
                "[阶段3] 识别到 {} 个 rename，已把缓存子树平移到新 path",
                applied_renames
            );
        }

        let result = match merge_scan_results_with_own_cancellable(
            cached_directories,
            rescanned_dirs
                .iter()
                .map(|item| item.node.clone())
                .collect(),
            &own_overrides,
            deleted_paths,
            path,
            cancellation.as_ref(),
        ) {
            Ok(tree) => tree,
            Err(_partial) => {
                merge_inner_timer
                    .finish_with(format!("status=cancelled renames={}", applied_renames));
                return Err(IncrementalScanError::Cancelled.into());
            }
        };
        merge_inner_timer.finish_with(format!(
            "nodes={} renames={}",
            result.len(),
            applied_renames
        ));
        result
    };
    upsert_root_files_node(
        &mut updated_tree,
        path,
        current_root_size,
        current_root_files,
    );

    tracing::info!(
        "[阶段3] 数据合并完成，耗时: {:.2}ms",
        merge_start.elapsed().as_secs_f64() * 1000.0
    );

    let totals_timer = StageTimer::start(
        "incremental",
        format!("stage3_compute_totals path={}", path.display()),
    );
    let (tree_size, tree_files) = sum_tree(&updated_tree);
    let (new_total_size, new_total_files) = (tree_size, tree_files);
    let new_total_dirs = count_directories(&updated_tree, &root_path_str);
    totals_timer.finish_with(format!(
        "files={} dirs={} size={}",
        new_total_files, new_total_dirs, new_total_size
    ));

    let health_timer = StageTimer::start(
        "incremental",
        format!("stage3_health_check path={}", path.display()),
    );
    let merge_health = inspect_tree_merge_health(&updated_tree, path);
    health_timer.finish_with(format!(
        "unique_dirs={} dup={} orphan={} inconsistent={}",
        merge_health.unique_dir_nodes,
        merge_health.duplicate_paths,
        merge_health.orphan_root_count,
        merge_health.inconsistent_node_count
    ));
    tracing::info!(
        "[阶段3] 合并后树校验 roots={} unique_dirs={} duplicate_paths={} orphan_roots={} inconsistent_nodes={}",
        updated_tree.len(),
        merge_health.unique_dir_nodes,
        merge_health.duplicate_paths,
        merge_health.orphan_root_count,
        merge_health.inconsistent_node_count
    );
    if !merge_health.orphan_root_samples.is_empty() {
        tracing::info!(
            "[阶段3] 异常根节点样本: {}",
            merge_health.orphan_root_samples.join(" | ")
        );
    }
    if !merge_health.inconsistent_node_samples.is_empty() {
        tracing::info!(
            "[阶段3] 统计异常样本: {}",
            merge_health.inconsistent_node_samples.join(" | ")
        );
    }
    let inconsistent_threshold = (merge_health.unique_dir_nodes / 100).max(50);
    let critical = merge_health.duplicate_paths > 0
        || merge_health.orphan_root_count > 0
        || merge_health.inconsistent_node_count > inconsistent_threshold
        || merge_health.unique_dir_nodes.abs_diff(new_total_dirs) > 10;
    if critical {
        merge_timer.finish_with(format!(
            "status=invalid_tree roots={} unique_dirs={} duplicate_paths={} orphan_roots={} inconsistent_nodes={}",
            updated_tree.len(),
            merge_health.unique_dir_nodes,
            merge_health.duplicate_paths,
            merge_health.orphan_root_count,
            merge_health.inconsistent_node_count
        ));
        tracing::info!("[阶段3] 检测到增量合并结构异常，放弃本次增量结果并切换到全量深度扫描重建缓存");
        let result = fallback_to_full_scan(
            path,
            scanner,
            cached_total_files.max(1),
            deep_progress_emitter.as_ref(),
            "增量合并后的树结构校验失败，需要全量重建缓存",
            cancellation.as_ref(),
        )
        .await?;
        total_timer.finish_with(format!(
            "status=fallback_invalid_tree backend={:?} files={} dirs={} size={}",
            result.scan_backend, result.total_files, result.total_dirs, result.total_size
        ));
        return Ok(result);
    }
    merge_timer.finish_with(format!(
        "strategy={} total_files={} total_dirs={} total_size={}",
        merge_strategy, new_total_files, new_total_dirs, new_total_size
    ));

    let large_files_timer = StageTimer::start(
        "incremental",
        format!("stage3_rebuild_large_files path={}", path.display()),
    );
    let deleted_path_keys: Vec<String> = changes
        .iter()
        .filter(|c| c.status == ChangeStatus::Deleted)
        .map(|c| normalized_path_key(&c.path))
        .collect();

    let rescanned_scopes: Vec<(String, RescanMode)> = rescanned_dirs
        .iter()
        .map(|item| (normalized_path_key_str(&item.node.path), item.mode))
        .collect();
    let root_key = normalized_path_key(path);

    let mut large_files: Vec<FileInfo> = cached_large_files
        .into_iter()
        .filter(|file| {
            let file_path = Path::new(&file.path);
            let file_key = normalized_path_key_str(&file.path);
            !deleted_path_keys
                .iter()
                .any(|deleted| normalized_path_starts_with(&file_key, deleted))
                && !rescanned_scopes
                    .iter()
                    .any(|(changed_path, mode)| match mode {
                        RescanMode::Recursive => normalized_path_starts_with(&file_key, changed_path),
                        RescanMode::DirectFilesOnly => {
                            file_path
                                .parent()
                                .map(|parent| normalized_path_key(parent) == *changed_path)
                                .unwrap_or(false)
                        }
                    })
                && (!root_files_changed
                    || !file_path
                        .parent()
                        .map(|parent| normalized_path_key(parent) == root_key)
                        .unwrap_or(false))
        })
        .collect();

    for rescanned in rescanned_dirs {
        large_files.extend(rescanned.large_files);
    }

    if root_files_changed {
        large_files.extend(root_large_files);
    }

    large_files.sort_by_key(|f| std::cmp::Reverse(f.size));
    large_files_timer.finish_with(format!("large_files={}", large_files.len()));
    let scan_duration_ms = start.elapsed().as_millis() as u64;

    emit_incremental_progress(
        progress_emitter.as_ref(),
        IncrementalScanProgress {
            phase: "completed".to_string(),
            total_dirs: total_cached_dirs,
            checked_dirs: total_cached_dirs,
            changed_dirs: total_changes,
            scanned_dirs: total_changes,
        },
    );

    tracing::info!("\n========== 增量扫描完成 ==========");
    tracing::info!("总耗时: {:.2}ms", scan_duration_ms as f64);
    tracing::info!(
        "更新后: {:.2} GB, {} 文件",
        new_total_size as f64 / 1024.0 / 1024.0 / 1024.0,
        new_total_files
    );
    tracing::info!("==================================\n");

    let checkpoint_timer = StageTimer::start(
        "incremental",
        format!("stage3_query_usn_checkpoint path={}", path.display()),
    );
    let journal = winfs::query_usn_checkpoint(path);
    match &journal {
        Some(checkpoint) => checkpoint_timer.finish_with(format!(
            "available=true journal_id={} next_usn={}",
            checkpoint.journal_id, checkpoint.next_usn
        )),
        None => checkpoint_timer.finish_with("available=false"),
    }
    if let Some(checkpoint) = &journal {
        tracing::info!(
            "[阶段3] 更新后的 USN checkpoint journal_id={} next_usn={}",
            checkpoint.journal_id, checkpoint.next_usn
        );
    } else {
        tracing::info!("[阶段3] 更新后仍未获取到 USN checkpoint");
    }

    total_timer.finish_with(format!(
        "status=complete files={} dirs={} size={} large_files={}",
        new_total_files,
        new_total_dirs,
        new_total_size,
        large_files.len()
    ));
    let system_reserved_bytes = get_disk_used_bytes(path)
        .map(|disk_used| disk_used.saturating_sub(new_total_size))
        .unwrap_or(cached_system_reserved_bytes);

    Ok(ScanResult {
        root_path: cached_root_path,
        total_size: new_total_size,
        system_reserved_bytes,
        total_files: new_total_files,
        total_dirs: new_total_dirs,
        scan_duration_ms,
        directories: updated_tree,
        large_files,
        inaccessible_count: cached_inaccessible_count,
        scan_backend: Some("incremental_usn".to_string()),
        root_file_id: winfs::get_path_file_id(path).or(cached_root_file_id),
        usn_journal_id: journal
            .map(|item| item.journal_id)
            .or(cached_usn_journal_id),
        usn_next_usn: journal.map(|item| item.next_usn).or(cached_usn_next_usn),
        cache_schema_version: 0,
        env_fingerprint: Default::default(),
        scan_completed: false,
    })
}

pub(crate) fn count_directories(dirs: &[DirectoryNode], root_path: &str) -> usize {
    dirs.iter()
        .map(|node| {
            if normalized_path_key_str(&node.path) == normalized_path_key_str(root_path) {
                0
            } else {
                1 + count_directories(&node.children, root_path)
            }
        })
        .sum()
}

pub(crate) fn sum_tree(dirs: &[DirectoryNode]) -> (u64, usize) {
    dirs.iter()
        .fold((0u64, 0usize), |(size_acc, file_acc), node| {
            let (child_size, child_files) = sum_tree(&node.children);
            let own_size = node.size.saturating_sub(child_size);
            let own_files = node.file_count.saturating_sub(child_files);
            (
                size_acc + child_size + own_size,
                file_acc + child_files + own_files,
            )
        })
}

pub(crate) fn upsert_root_files_node(
    nodes: &mut Vec<DirectoryNode>,
    root_path: &Path,
    root_size: u64,
    root_files: usize,
) {
    let root_path_str = root_path.to_string_lossy().to_string();
    nodes.retain(|node| !path_matches(&node.path, root_path));

    if root_size > 0 || root_files > 0 {
        nodes.push(DirectoryNode {
            path: root_path_str,
            name: "根目录文件".to_string(),
            size: root_size,
            file_count: root_files,
            dir_count: 0,
            children: vec![],
            has_children: false,
            is_symlink: false,
            link_target: None,
            safety: None,
            modified_time: None,
            file_id: None,
        });
    }

    for node in nodes.iter_mut() {
        node.has_children = !node.children.is_empty();
    }
    nodes.sort_by_key(|n| std::cmp::Reverse(n.size));
}

fn flatten_tree(nodes: Vec<DirectoryNode>, flat: &mut HashMap<String, DirectoryNode>) {
    for mut node in nodes {
        let children = std::mem::take(&mut node.children);
        flat.insert(normalized_path_key_str(&node.path), node);
        flatten_tree(children, flat);
    }
}

fn rebuild_tree(mut flat: HashMap<String, DirectoryNode>, root_path: &Path) -> Vec<DirectoryNode> {
    let root_key = normalized_path_key(root_path);
    let mut paths: Vec<String> = flat.keys().cloned().collect();
    paths.sort_by(|a, b| {
        let depth_b = flat
            .get(b)
            .map(|node| Path::new(&node.path).components().count())
            .unwrap_or(0);
        let depth_a = flat
            .get(a)
            .map(|node| Path::new(&node.path).components().count())
            .unwrap_or(0);
        depth_b.cmp(&depth_a)
    });

    let mut roots = Vec::new();
    for key in paths {
        let Some(node) = flat.remove(&key) else {
            continue;
        };

        if normalized_path_key_str(&node.path) != root_key {
            if let Some(parent_path) = Path::new(&node.path).parent() {
                if normalized_path_key(parent_path) != root_key {
                    let parent_key = normalized_path_key(parent_path);
                    if let Some(parent) = flat.get_mut(&parent_key) {
                        parent.children.push(node);
                        continue;
                    }
                }
            }
        }

        roots.push(node);
    }

    sort_directory_tree(&mut roots);
    roots
}

fn apply_direct_file_refreshes(
    mut tree: Vec<DirectoryNode>,
    refreshed_nodes: Vec<DirectoryNode>,
    root_path: &Path,
) -> Vec<DirectoryNode> {
    fn child_totals(children: &[DirectoryNode]) -> (u64, usize, usize) {
        children.iter().fold(
            (0u64, 0usize, 0usize),
            |(size_acc, file_acc, dir_acc), child| {
                (
                    size_acc + child.size,
                    file_acc + child.file_count,
                    dir_acc + child.dir_count,
                )
            },
        )
    }

    fn walk(
        nodes: &mut [DirectoryNode],
        updates: &mut HashMap<String, DirectoryNode>,
        root_path: &Path,
    ) -> bool {
        let mut any_changed = false;

        for node in nodes.iter_mut() {
            if path_matches(&node.path, root_path) && node.dir_count == 0 {
                continue;
            }

            let (old_child_size, old_child_files, _) = child_totals(&node.children);
            let child_changed = walk(&mut node.children, updates, root_path);
            let (new_child_size, new_child_files, new_child_dirs) = child_totals(&node.children);

            let mut own_size = node.size.saturating_sub(old_child_size);
            let mut own_files = node.file_count.saturating_sub(old_child_files);
            let mut node_changed = child_changed;

            if let Some(update) = updates.remove(&normalized_path_key_str(&node.path)) {
                let (update_child_size, update_child_files, _) = child_totals(&update.children);
                own_size = update.size.saturating_sub(update_child_size);
                own_files = update.file_count.saturating_sub(update_child_files);
                node.modified_time = update.modified_time;
                node.file_id = update.file_id.or(node.file_id);
                node.has_children = !node.children.is_empty();
                node_changed = true;
            }

            if node_changed {
                node.size = own_size + new_child_size;
                node.file_count = own_files + new_child_files;
                node.dir_count = 1 + new_child_dirs;
                node.has_children = !node.children.is_empty();
                any_changed = true;
            }
        }

        if any_changed {
            nodes.sort_by_key(|n| std::cmp::Reverse(n.size));
        }

        any_changed
    }

    let mut updates = refreshed_nodes
        .into_iter()
        .map(|node| (normalized_path_key_str(&node.path), node))
        .collect::<HashMap<_, _>>();

    let _ = walk(&mut tree, &mut updates, root_path);
    tree
}
fn apply_rename_pre_merge(
    mut tree: Vec<DirectoryNode>,
    mut deleted_paths: Vec<String>,
    renames: &[winfs::UsnRenameEvent],
    rescanned_keys: &HashSet<String>,
) -> (Vec<String>, Vec<DirectoryNode>, usize) {
    if renames.is_empty() {
        return (deleted_paths, tree, 0);
    }

    let mut applied = 0usize;
    let deleted_set: HashSet<String> = deleted_paths
        .iter()
        .map(|p| normalized_path_key_str(p))
        .collect();

    for rename in renames {
        // 只搬目录，文件 rename 由 direct_file_dirs 的重扫负责。
        if !rename.is_dir {
            continue;
        }
        let old_key = normalized_path_key_str(&rename.old_path);
        let new_key = normalized_path_key_str(&rename.new_path);
        if old_key == new_key {
            continue;
        }
        // 新 path 已经在本次 Recursive 重扫范围内 → 不要碰，让重扫覆盖。
        if rescanned_keys
            .iter()
            .any(|k| key_starts_with(&new_key, k))
        {
            continue;
        }
        // 老 path 也在重扫范围内 → 同理，让重扫处理。
        if rescanned_keys
            .iter()
            .any(|k| key_starts_with(&old_key, k))
        {
            continue;
        }
        // 新 path 已经存在在树里 → 让 keyed merge 走原本路径。
        if find_node_mut(&mut tree, &new_key).is_some() {
            continue;
        }
        // 把 old_key 子树取出来。
        let Some(subtree) = extract_subtree(&mut tree, &old_key) else {
            continue;
        };
        let mut moved = subtree;
        relocate_subtree_path(&mut moved, &old_key, &rename.new_path);
        if !attach_under_parent(&mut tree, &rename.new_path, moved) {
            // 父节点找不到（常见于 new_path 的 parent 不在 cache 里）：
            // 按"删旧 + 建新"语义留给 keyed merge 处理。但因为我们刚刚 extract
            // 掉了 old_key，如果再走删旧路径会找不到节点 → 这里直接把删除项也
            // 干掉，让 keyed merge 看到的就是"什么都没动"，新 path 的 Recursive
            // 重扫节点（如果有）会负责挂上来。
        } else {
            applied += 1;
        }
    }

    if applied > 0 {
        // 把已经被 rename 处理过的 old_path 从 deleted_paths 里移除。
        let renamed_old_keys: HashSet<String> = renames
            .iter()
            .filter(|r| r.is_dir)
            .map(|r| normalized_path_key_str(&r.old_path))
            .collect();
        deleted_paths.retain(|p| !renamed_old_keys.contains(&normalized_path_key_str(p)));
        let _ = deleted_set; // suppress unused
    }

    (deleted_paths, tree, applied)
}

fn find_node_mut<'a>(tree: &'a mut [DirectoryNode], key: &str) -> Option<&'a mut DirectoryNode> {
    for node in tree.iter_mut() {
        if normalized_path_key_str(&node.path) == key {
            return Some(node);
        }
        if let Some(found) = find_node_mut(&mut node.children, key) {
            return Some(found);
        }
    }
    None
}

fn extract_subtree(tree: &mut Vec<DirectoryNode>, key: &str) -> Option<DirectoryNode> {
    if let Some(pos) = tree
        .iter()
        .position(|n| normalized_path_key_str(&n.path) == key)
    {
        return Some(tree.remove(pos));
    }
    for node in tree.iter_mut() {
        if let Some(found) = extract_subtree(&mut node.children, key) {
            node.has_children = !node.children.is_empty();
            return Some(found);
        }
    }
    None
}

fn attach_under_parent(
    tree: &mut [DirectoryNode],
    new_path: &str,
    new_node: DirectoryNode,
) -> bool {
    let parent = match Path::new(new_path).parent() {
        Some(p) => p,
        None => return false,
    };
    let parent_key = normalized_path_key(parent);
    if let Some(parent_node) = find_node_mut(tree, &parent_key) {
        parent_node.children.push(new_node);
        parent_node.has_children = true;
        return true;
    }
    false
}

/// 把整棵子树里的 path 字符串从 old_prefix 平移到 new_prefix。
/// 用 normalized key 做匹配但保留原本的大小写格式（在 Windows 上能通过
/// path_matches 做大小写不敏感的查找）。
fn relocate_subtree_path(node: &mut DirectoryNode, old_key_prefix: &str, new_path: &str) {
    let old_path_key = normalized_path_key_str(&node.path);
    let new_path_for_node = if old_path_key == old_key_prefix {
        new_path.to_string()
    } else if let Some(suffix) = old_path_key.strip_prefix(old_key_prefix) {
        // suffix 包含分隔符，拼接到 new_path 上
        format!("{}{}", new_path, suffix)
    } else {
        node.path.clone()
    };
    node.name = Path::new(&new_path_for_node)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| node.name.clone());
    node.path = new_path_for_node;
    for child in node.children.iter_mut() {
        relocate_subtree_path(child, old_key_prefix, new_path);
    }
}

pub fn merge_scan_results(
    old_tree: Vec<DirectoryNode>,
    changed_dirs: Vec<DirectoryNode>,
    deleted_paths: Vec<String>,
    root_path: &Path,
) -> Vec<DirectoryNode> {
    merge_scan_results_with_own(old_tree, changed_dirs, &HashMap::new(), deleted_paths, root_path)
}
/// 增量合并的"权威"入口。和 [`merge_scan_results`] 的区别在于：
/// - 把每个本次重扫节点的 `(own_size, own_file_count)` 也透传进来；
/// - 合并完成后会自底向上做一次"权威回填"，确保父节点的 size/file_count/dir_count
///   = 自己直接持有的值 + Σ 子节点。这样能修掉「父被 Recursive 替换、子又走
///   DirectFilesOnly 把 clone 出来的旧 children 覆盖回去」造成的统计漂移。
pub fn merge_scan_results_with_own(
    old_tree: Vec<DirectoryNode>,
    changed_dirs: Vec<DirectoryNode>,
    own_overrides: &HashMap<String, (u64, usize)>,
    deleted_paths: Vec<String>,
    root_path: &Path,
) -> Vec<DirectoryNode> {
    merge_scan_results_with_own_cancellable::<NoCancel>(
        old_tree,
        changed_dirs,
        own_overrides,
        deleted_paths,
        root_path,
        None,
    )
    .unwrap_or_else(|partial| partial)
}

/// 与 `merge_scan_results_with_own` 一致，但允许在权威回填阶段每 1024 节点
/// 检查一次取消信号，命中后立刻返回半成品树（封装在 Err 里）。生产路径
/// 拿到 Err 时应当立刻退出整个增量扫描。
pub(crate) fn merge_scan_results_with_own_cancellable<C: CancellationLike>(
    old_tree: Vec<DirectoryNode>,
    changed_dirs: Vec<DirectoryNode>,
    own_overrides: &HashMap<String, (u64, usize)>,
    deleted_paths: Vec<String>,
    root_path: &Path,
    cancellation: Option<&C>,
) -> Result<Vec<DirectoryNode>, Vec<DirectoryNode>> {
    // Step 0: 清掉旧树之前可能积累下来的统计偏差。
    //
    // 旧树里若有节点 size != own + Σchild.size（因为之前合并算法的累积漂移），
    // 会让本轮采到的 cached_own = old_size - Σold_child.size 也带偏差。把
    // 旧树先做一次自下而上的 enforce：own 用 saturating_sub 抓一下，然后强制
    // node.size = own + Σchild.size，file_count / dir_count 同理。这样进入
    // collect_cached_own 时拿到的就是已经一致的快照，own 也就对了。
    //
    // changed_dirs 里的节点马上要被替换，没必要清；root 的 size/file_count 由
    // sum_tree 在 reconcile 末尾重算，这里也不强求清到根。
    let mut old_tree = old_tree;
    sanitize_tree_subtotals(&mut old_tree, root_path);

    let mut cached_own: HashMap<String, (u64, usize)> = HashMap::new();
    collect_cached_own(&old_tree, &mut cached_own);

    // 同时把"changed_dirs 自带的 own"也合进 cached_own —— 这是兼容旧 API 的关键：
    // 对每个被 upsert 替换的节点，size - Σchild.size 就是它本次扫描里"自己持有"
    // 的部分，比 cached_own 留下的老值更新。`own_overrides`（外部显式传入）
    // 优先级最高，依然可以覆盖这一步。
    //
    // 注意要递归到 changed_dirs 内部的所有子节点：rescan_directory_tree 返回
    // 的子树里每个节点的 size/file_count 都是新扫到的真实值，应当一并覆盖
    // cached_own 里同 path 的旧值。只浅扫一层会让 changed_dirs 子节点继续
    // 沿用旧 cached_own 值，权威回填阶段就会用到陈旧 own。
    //
    // 重要：当 changed_dirs 同时包含 parent 和 child 的独立变更时（overlap），
    // child 的值应该优先于 parent 中嵌套的同路径节点。先收集顶层 key 集合，
    // 递归时遇到已有独立变更的 path 就跳过，最后由其自己的顶层 ingest 写入。
    let top_level_keys: HashSet<String> = changed_dirs
        .iter()
        .map(|n| normalized_path_key_str(&n.path))
        .collect();

    fn ingest_changed_own(
        nodes: &[DirectoryNode],
        cached_own: &mut HashMap<String, (u64, usize)>,
        top_level_keys: &HashSet<String>,
        is_top_level: bool,
    ) {
        for node in nodes {
            let key = normalized_path_key_str(&node.path);
            // 如果这个 child 在顶层有独立变更，跳过——让它自己的顶层遍历写入正确值
            if !is_top_level && top_level_keys.contains(&key) {
                continue;
            }
            let child_size: u64 = node.children.iter().map(|c| c.size).sum();
            let child_files: usize = node.children.iter().map(|c| c.file_count).sum();
            let own_size = node.size.saturating_sub(child_size);
            let own_files = node.file_count.saturating_sub(child_files);
            cached_own.insert(key, (own_size, own_files));
            ingest_changed_own(&node.children, cached_own, top_level_keys, false);
        }
    }
    ingest_changed_own(&changed_dirs, &mut cached_own, &top_level_keys, true);

    let mut tree = match merge_scan_results_keyed(old_tree, changed_dirs, deleted_paths, root_path)
    {
        Ok(tree) => tree,
        Err(fallback_input) => merge_scan_results_flat(
            fallback_input.old_tree,
            fallback_input.changed_dirs,
            fallback_input.deleted_paths,
            root_path,
        ),
    };
    let root_key = normalized_path_key(root_path);
    let mut counter: usize = 0;
    let mut cancelled = false;
    for node in tree.iter_mut() {
        if normalized_path_key_str(&node.path) == root_key {
            continue;
        }
        recalc_subtotals_authoritative_cancellable(
            node,
            own_overrides,
            &cached_own,
            &mut counter,
            cancellation,
            &mut cancelled,
        );
        if cancelled {
            return Err(tree);
        }
    }

    sort_directory_tree(&mut tree);
    Ok(tree)
}

/// 占位类型：提供一个 `CancellationLike` 但永不取消，用来给
/// `merge_scan_results_with_own` 的非可取消入口填类型参数。
struct NoCancel;
impl CancellationLike for NoCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

fn recalc_subtotals_authoritative_cancellable<C: CancellationLike>(
    node: &mut DirectoryNode,
    own_overrides: &HashMap<String, (u64, usize)>,
    cached_own: &HashMap<String, (u64, usize)>,
    counter: &mut usize,
    cancellation: Option<&C>,
    cancelled: &mut bool,
) {
    if *cancelled {
        return;
    }
    for child in node.children.iter_mut() {
        recalc_subtotals_authoritative_cancellable(
            child,
            own_overrides,
            cached_own,
            counter,
            cancellation,
            cancelled,
        );
        if *cancelled {
            return;
        }
    }
    *counter += 1;
    if (*counter).is_multiple_of(MERGE_CANCEL_CHECK_STRIDE) {
        if let Some(token) = cancellation {
            if token.is_cancelled() {
                *cancelled = true;
                return;
            }
        }
    }
    recalc_subtotals_authoritative(node, own_overrides, cached_own);
}

fn collect_cached_own(nodes: &[DirectoryNode], out: &mut HashMap<String, (u64, usize)>) {
    for node in nodes {
        let child_size: u64 = node.children.iter().map(|c| c.size).sum();
        let child_files: usize = node.children.iter().map(|c| c.file_count).sum();
        let own_size = node.size.saturating_sub(child_size);
        let own_files = node.file_count.saturating_sub(child_files);
        out.insert(normalized_path_key_str(&node.path), (own_size, own_files));
        collect_cached_own(&node.children, out);
    }
}

/// 自下而上地把每棵旧树修整成 size = own + Σchild.size、file_count = own + Σchild.file_count、
/// dir_count = 1 + Σchild.dir_count。
///
/// 这是历史包袱清理：之前几轮合并算法可能在节点上累积了 size/file_count/dir_count
/// 与子节点和不一致的偏差。直接从旧树读 cached_own 时，这种偏差会被错误地"持有"
/// 到 own 上。先 sanitize 一次，确保拿到的 own 至少满足 own + Σchild = node 当前
/// 持有值，把累积偏差冻结成"old 时刻已经存在的差"，不再被传到下一轮。
///
/// root 节点也走同样规则；调用方稍后会用 sum_tree 在更上层重算 root 的总量。
fn sanitize_tree_subtotals(nodes: &mut [DirectoryNode], _root_path: &Path) {
    for node in nodes.iter_mut() {
        sanitize_tree_subtotals(&mut node.children, _root_path);
        let child_size: u64 = node.children.iter().map(|c| c.size).sum();
        let child_files: usize = node.children.iter().map(|c| c.file_count).sum();
        let child_dirs: usize = node.children.iter().map(|c| c.dir_count).sum();
        let own_size = node.size.saturating_sub(child_size);
        let own_files = node.file_count.saturating_sub(child_files);
        node.size = own_size + child_size;
        node.file_count = own_files + child_files;
        node.dir_count = 1 + child_dirs;
        node.has_children = !node.children.is_empty();
    }
}
fn recalc_subtotals_authoritative(
    node: &mut DirectoryNode,
    own_overrides: &HashMap<String, (u64, usize)>,
    cached_own: &HashMap<String, (u64, usize)>,
) {
    for child in node.children.iter_mut() {
        recalc_subtotals_authoritative(child, own_overrides, cached_own);
    }

    let key = normalized_path_key_str(&node.path);
    let child_size: u64 = node.children.iter().map(|c| c.size).sum();
    let child_files: usize = node.children.iter().map(|c| c.file_count).sum();
    let child_dirs: usize = node.children.iter().map(|c| c.dir_count).sum();

    let (own_size, own_files) = if let Some(&value) = own_overrides.get(&key) {
        value
    } else if let Some(&value) = cached_own.get(&key) {
        value
    } else {
        (
            node.size.saturating_sub(child_size),
            node.file_count.saturating_sub(child_files),
        )
    };

    node.size = own_size + child_size;
    node.file_count = own_files + child_files;
    node.dir_count = 1 + child_dirs;
    node.has_children = !node.children.is_empty();
}

struct KeyedMergeFallback {
    old_tree: Vec<DirectoryNode>,
    changed_dirs: Vec<DirectoryNode>,
    deleted_paths: Vec<String>,
}

fn merge_scan_results_keyed(
    old_tree: Vec<DirectoryNode>,
    changed_dirs: Vec<DirectoryNode>,
    deleted_paths: Vec<String>,
    root_path: &Path,
) -> Result<Vec<DirectoryNode>, KeyedMergeFallback> {
    let root_key = normalized_path_key(root_path);

    let mut delete_keys: Vec<String> = Vec::with_capacity(deleted_paths.len());
    for raw in &deleted_paths {
        let key = normalized_path_key_str(raw);
        if key == root_key {
            return Err(KeyedMergeFallback {
                old_tree,
                changed_dirs,
                deleted_paths,
            });
        }
        delete_keys.push(key);
    }

    let mut change_keys: Vec<String> = Vec::with_capacity(changed_dirs.len());
    for node in &changed_dirs {
        let key = normalized_path_key_str(&node.path);
        if key == root_key {
            return Err(KeyedMergeFallback {
                old_tree,
                changed_dirs,
                deleted_paths,
            });
        }
        change_keys.push(key);
    }

    let mut tree = old_tree;

    let mut delete_pairs: Vec<(String, String)> = deleted_paths
        .iter()
        .zip(delete_keys.iter())
        .map(|(p, k)| (p.clone(), k.clone()))
        .collect();
    delete_pairs.sort_by_key(|pair| std::cmp::Reverse(pair.1.matches('\\').count()));

    for (raw, key) in &delete_pairs {
        let target = Path::new(raw);
        let parent = match target.parent() {
            Some(p) => p,
            None => {
                return Err(KeyedMergeFallback {
                    old_tree: tree,
                    changed_dirs,
                    deleted_paths,
                });
            }
        };
        let parent_key = normalized_path_key(parent);
        if !apply_delete(&mut tree, &parent_key, key, &root_key) {
            return Err(KeyedMergeFallback {
                old_tree: tree,
                changed_dirs,
                deleted_paths,
            });
        }
    }

    let mut indexed: Vec<(usize, String, DirectoryNode)> = changed_dirs
        .into_iter()
        .enumerate()
        .map(|(idx, node)| (idx, change_keys[idx].clone(), node))
        .collect();
    indexed.sort_by_key(|item| item.1.matches('\\').count());

    for (_, key, node) in indexed {
        let target = PathBuf::from(&node.path);
        let parent = match target.parent() {
            Some(p) => p.to_path_buf(),
            None => {
                return Err(KeyedMergeFallback {
                    old_tree: tree,
                    changed_dirs: vec![node],
                    deleted_paths: Vec::new(),
                });
            }
        };
        let parent_key = normalized_path_key(&parent);
        apply_upsert(&mut tree, &parent_key, &key, &root_key, node);
    }

    sort_directory_tree(&mut tree);
    Ok(tree)
}

fn apply_delete(
    tree: &mut Vec<DirectoryNode>,
    parent_key: &str,
    target_key: &str,
    root_key: &str,
) -> bool {
    if parent_key == root_key {
        if let Some(pos) = tree
            .iter()
            .position(|n| normalized_path_key_str(&n.path) == target_key)
        {
            tree.remove(pos);
            return true;
        }
        return false;
    }
    for node in tree.iter_mut() {
        if let Some((removed_size, removed_files, removed_dirs)) =
            delete_in_subtree(node, parent_key, target_key)
        {
            apply_subtotal_delta(
                node,
                -(removed_size as i128),
                -(removed_files as isize),
                -(removed_dirs as isize),
            );
            return true;
        }
    }
    false
}

fn delete_in_subtree(
    node: &mut DirectoryNode,
    parent_key: &str,
    target_key: &str,
) -> Option<(u64, usize, usize)> {
    let node_key = normalized_path_key_str(&node.path);
    if node_key == parent_key {
        if let Some(pos) = node
            .children
            .iter()
            .position(|c| normalized_path_key_str(&c.path) == target_key)
        {
            let removed = node.children.remove(pos);
            if node.children.is_empty() {
                node.has_children = false;
            }
            return Some((removed.size, removed.file_count, removed.dir_count));
        }
        return None;
    }
    if !key_starts_with(parent_key, &node_key) {
        return None;
    }
    for child in node.children.iter_mut() {
        if let Some(delta) = delete_in_subtree(child, parent_key, target_key) {
            apply_subtotal_delta(
                child,
                -(delta.0 as i128),
                -(delta.1 as isize),
                -(delta.2 as isize),
            );
            return Some(delta);
        }
    }
    None
}

fn apply_upsert(
    tree: &mut Vec<DirectoryNode>,
    parent_key: &str,
    target_key: &str,
    root_key: &str,
    new_node: DirectoryNode,
) {
    if parent_key == root_key {
        if let Some(pos) = tree
            .iter()
            .position(|n| normalized_path_key_str(&n.path) == target_key)
        {
            tree[pos] = new_node;
        } else {
            tree.push(new_node);
        }
        return;
    }
    for node in tree.iter_mut() {
        if let Some((size_delta, file_delta, dir_delta)) =
            upsert_in_subtree(node, parent_key, target_key, &new_node)
        {
            apply_subtotal_delta(node, size_delta, file_delta, dir_delta);
            return;
        }
    }
    tree.push(new_node);
}

fn upsert_in_subtree(
    node: &mut DirectoryNode,
    parent_key: &str,
    target_key: &str,
    new_node: &DirectoryNode,
) -> Option<(i128, isize, isize)> {
    let node_key = normalized_path_key_str(&node.path);
    if node_key == parent_key {
        if let Some(pos) = node
            .children
            .iter()
            .position(|c| normalized_path_key_str(&c.path) == target_key)
        {
            let prev = node.children[pos].clone();
            node.children[pos] = new_node.clone();
            let size_delta = new_node.size as i128 - prev.size as i128;
            let file_delta = new_node.file_count as isize - prev.file_count as isize;
            let dir_delta = new_node.dir_count as isize - prev.dir_count as isize;
            return Some((size_delta, file_delta, dir_delta));
        }
        node.children.push(new_node.clone());
        node.has_children = true;
        let size_delta = new_node.size as i128;
        let file_delta = new_node.file_count as isize;
        let dir_delta = new_node.dir_count as isize;
        return Some((size_delta, file_delta, dir_delta));
    }
    if !key_starts_with(parent_key, &node_key) {
        return None;
    }
    for child in node.children.iter_mut() {
        if let Some(delta) = upsert_in_subtree(child, parent_key, target_key, new_node) {
            apply_subtotal_delta(child, delta.0, delta.1, delta.2);
            return Some(delta);
        }
    }
    None
}

fn apply_subtotal_delta(
    node: &mut DirectoryNode,
    size_delta: i128,
    file_delta: isize,
    dir_delta: isize,
) {
    node.size = if size_delta >= 0 {
        node.size.saturating_add(size_delta as u64)
    } else {
        node.size.saturating_sub(size_delta.unsigned_abs() as u64)
    };
    node.file_count = node.file_count.saturating_add_signed(file_delta);
    node.dir_count = node.dir_count.saturating_add_signed(dir_delta);
}

fn key_starts_with(child_key: &str, ancestor_key: &str) -> bool {
    if child_key == ancestor_key {
        return true;
    }
    if child_key.len() <= ancestor_key.len() {
        return false;
    }
    if !child_key.starts_with(ancestor_key) {
        return false;
    }
    let next = &child_key[ancestor_key.len()..];
    next.starts_with('\\') || next.starts_with('/')
}

fn merge_scan_results_flat(
    old_tree: Vec<DirectoryNode>,
    changed_dirs: Vec<DirectoryNode>,
    deleted_paths: Vec<String>,
    root_path: &Path,
) -> Vec<DirectoryNode> {
    let deleted_paths: Vec<PathBuf> = deleted_paths.into_iter().map(PathBuf::from).collect();
    let mut flat = HashMap::new();
    flatten_tree(old_tree, &mut flat);

    fn apply_delta_to_ancestors(
        flat: &mut HashMap<String, DirectoryNode>,
        path: &Path,
        root_path: &Path,
        size_delta: i128,
        file_delta: isize,
        dir_delta: isize,
    ) {
        fn apply_u64_delta(value: u64, delta: i128) -> u64 {
            if delta >= 0 {
                value.saturating_add(delta as u64)
            } else {
                value.saturating_sub(delta.unsigned_abs() as u64)
            }
        }

        let mut current = path.parent();
        while let Some(parent) = current {
            if normalized_path_key(parent) == normalized_path_key(root_path) {
                break;
            }

            let key = normalized_path_key(parent);
            if let Some(node) = flat.get_mut(&key) {
                node.size = apply_u64_delta(node.size, size_delta);
                node.file_count = node.file_count.saturating_add_signed(file_delta);
                node.dir_count = node.dir_count.saturating_add_signed(dir_delta);
            }
            current = parent.parent();
        }
    }

    let mut subtree_prefixes: Vec<PathBuf> = Vec::with_capacity(deleted_paths.len() + changed_dirs.len());

    for deleted_path in &deleted_paths {
        let deleted_key = normalized_path_key(deleted_path);
        if let Some(old_node) = flat.get(&deleted_key).cloned() {
            apply_delta_to_ancestors(
                &mut flat,
                deleted_path,
                root_path,
                -(old_node.size as i128),
                -(old_node.file_count as isize),
                -(old_node.dir_count as isize),
            );
        }
        subtree_prefixes.push(deleted_path.clone());
    }

    let mut to_flatten: Vec<DirectoryNode> = Vec::with_capacity(changed_dirs.len());
    for changed_dir in changed_dirs {
        let changed_path = PathBuf::from(&changed_dir.path);
        let previous = flat
            .get(&normalized_path_key_str(&changed_dir.path))
            .cloned();
        let size_delta =
            changed_dir.size as i128 - previous.as_ref().map(|node| node.size as i128).unwrap_or(0);
        let file_delta = changed_dir.file_count as isize
            - previous
                .as_ref()
                .map(|node| node.file_count as isize)
                .unwrap_or(0);
        let dir_delta = changed_dir.dir_count as isize
            - previous
                .as_ref()
                .map(|node| node.dir_count as isize)
                .unwrap_or(0);

        apply_delta_to_ancestors(
            &mut flat,
            &changed_path,
            root_path,
            size_delta,
            file_delta,
            dir_delta,
        );
        subtree_prefixes.push(changed_path);
        to_flatten.push(changed_dir);
    }

    if !subtree_prefixes.is_empty() {
        let normalized_prefixes: Vec<String> = subtree_prefixes
            .iter()
            .map(|p| {
                let mut s = normalized_path_key(p);
                if !s.ends_with('\\') {
                    s.push('\\');
                }
                s
            })
            .collect();
        let exact_keys: std::collections::HashSet<String> = subtree_prefixes
            .iter()
            .map(|p| normalized_path_key(p))
            .collect();

        flat.retain(|key, _node| {
            if exact_keys.contains(key) {
                return false;
            }
            !normalized_prefixes
                .iter()
                .any(|prefix| key.starts_with(prefix.as_str()))
        });
    }

    if !to_flatten.is_empty() {
        flatten_tree(to_flatten, &mut flat);
    }

    rebuild_tree(flat, root_path)
}

pub fn inspect_tree_merge_health(nodes: &[DirectoryNode], root_path: &Path) -> TreeMergeHealth {
    fn walk(
        node: &DirectoryNode,
        root_path: &Path,
        seen: &mut HashSet<String>,
        health: &mut TreeMergeHealth,
    ) -> (u64, usize, usize) {
        let node_key = normalized_path_key_str(&node.path);
        if !seen.insert(node_key) {
            health.duplicate_paths += 1;
            return (0, 0, 0);
        }

        let mut child_size_sum = 0u64;
        let mut child_file_sum = 0usize;
        let mut child_dir_sum = 0usize;
        for child in &node.children {
            let (size, files, dirs) = walk(child, root_path, seen, health);
            child_size_sum += size;
            child_file_sum += files;
            child_dir_sum += dirs;
        }

        if !path_matches(&node.path, root_path) {
            health.unique_dir_nodes += 1;
            let expected_dir_count = 1 + child_dir_sum;
            let dir_diff = node.dir_count.abs_diff(expected_dir_count);
            let dir_tolerance = (expected_dir_count / 50).max(2);
            if dir_diff > dir_tolerance {
                health.inconsistent_node_count += 1;
                if health.inconsistent_node_samples.len() < 5 {
                    health.inconsistent_node_samples.push(format!(
                        "{} dir_count={} expected={}",
                        node.path, node.dir_count, expected_dir_count
                    ));
                }
            }
        }

        if node.size < child_size_sum {
            let diff = child_size_sum - node.size;
            let tolerance = (child_size_sum / 50).max(1024 * 1024);
            if diff > tolerance {
                health.inconsistent_node_count += 1;
                if health.inconsistent_node_samples.len() < 5 {
                    health.inconsistent_node_samples.push(format!(
                        "{} size={} child_sum={}",
                        node.path, node.size, child_size_sum
                    ));
                }
            }
        }
        if node.file_count < child_file_sum {
            let diff = child_file_sum - node.file_count;
            let tolerance = (child_file_sum / 50).max(8);
            if diff > tolerance {
                health.inconsistent_node_count += 1;
                if health.inconsistent_node_samples.len() < 5 {
                    health.inconsistent_node_samples.push(format!(
                        "{} file_count={} child_sum={}",
                        node.path, node.file_count, child_file_sum
                    ));
                }
            }
        }

        let dir_total = if path_matches(&node.path, root_path) {
            0
        } else {
            1 + child_dir_sum
        };

        (node.size, node.file_count, dir_total)
    }

    let root_key = normalized_path_key(root_path);
    let mut health = TreeMergeHealth::default();
    let mut seen = HashSet::new();

    for node in nodes {
        if !path_matches(&node.path, root_path) {
            let is_direct_child = Path::new(&node.path)
                .parent()
                .map(|parent| normalized_path_key(parent) == root_key)
                .unwrap_or(false);
            if !is_direct_child {
                health.orphan_root_count += 1;
                if health.orphan_root_samples.len() < 5 {
                    health.orphan_root_samples.push(node.path.clone());
                }
            }
        }
        let _ = walk(node, root_path, &mut seen, &mut health);
    }

    health
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(
        path: &Path,
        size: u64,
        file_count: usize,
        dir_count: usize,
        children: Vec<DirectoryNode>,
    ) -> DirectoryNode {
        DirectoryNode {
            path: path.to_string_lossy().to_string(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            size,
            file_count,
            dir_count,
            has_children: !children.is_empty(),
            children,
            is_symlink: false,
            link_target: None,
            safety: None,
            modified_time: None,
            file_id: None,
        }
    }

    #[test]
    fn merge_scan_results_updates_ancestor_totals_for_changed_subtree() {
        let root = PathBuf::from("root");
        let dir_a = root.join("a");
        let dir_b = dir_a.join("b");

        let old_tree = vec![make_node(
            &dir_a,
            100,
            2,
            2,
            vec![make_node(&dir_b, 40, 1, 1, vec![])],
        )];

        let merged = merge_scan_results(
            old_tree,
            vec![make_node(&dir_b, 80, 2, 1, vec![])],
            vec![],
            &root,
        );

        assert_eq!(merged.len(), 1);
        let merged_a = &merged[0];
        assert_eq!(merged_a.path, dir_a.to_string_lossy());
        assert_eq!(merged_a.size, 140);
        assert_eq!(merged_a.file_count, 3);
        assert_eq!(merged_a.dir_count, 2);
        assert_eq!(merged_a.children.len(), 1);
        assert_eq!(merged_a.children[0].path, dir_b.to_string_lossy());
        assert_eq!(merged_a.children[0].size, 80);
        assert_eq!(merged_a.children[0].file_count, 2);
    }

    #[test]
    fn merge_scan_results_removes_deleted_subtree_and_updates_parent() {
        let root = PathBuf::from("root");
        let dir_a = root.join("a");
        let dir_b = dir_a.join("b");
        let dir_c = dir_a.join("c");

        let old_tree = vec![make_node(
            &dir_a,
            150,
            3,
            3,
            vec![
                make_node(&dir_b, 50, 1, 1, vec![]),
                make_node(&dir_c, 70, 2, 1, vec![]),
            ],
        )];

        let merged = merge_scan_results(
            old_tree,
            vec![],
            vec![dir_b.to_string_lossy().to_string()],
            &root,
        );

        assert_eq!(merged.len(), 1);
        let merged_a = &merged[0];
        assert_eq!(merged_a.size, 100);
        assert_eq!(merged_a.file_count, 2);
        assert_eq!(merged_a.dir_count, 2);
        assert_eq!(merged_a.children.len(), 1);
        assert_eq!(merged_a.children[0].path, dir_c.to_string_lossy());
    }

    #[cfg(windows)]
    #[test]
    fn merge_scan_results_matches_paths_case_insensitively() {
        let root = PathBuf::from(r"C:\");
        let dir_a = PathBuf::from(r"C:\Data");
        let dir_b = PathBuf::from(r"C:\Data\Logs");

        let old_tree = vec![make_node(
            &dir_a,
            100,
            2,
            2,
            vec![make_node(&dir_b, 40, 1, 1, vec![])],
        )];

        let merged = merge_scan_results(
            old_tree,
            vec![make_node(Path::new(r"c:\data\logs"), 80, 2, 1, vec![])],
            vec![],
            &root,
        );

        assert_eq!(merged.len(), 1);
        let merged_a = &merged[0];
        assert_eq!(
            normalized_path_key_str(&merged_a.path),
            normalized_path_key(&dir_a)
        );
        assert_eq!(merged_a.size, 140);
        assert_eq!(merged_a.file_count, 3);
        assert_eq!(merged_a.dir_count, 2);
        assert_eq!(merged_a.children.len(), 1);
        assert_eq!(
            normalized_path_key_str(&merged_a.children[0].path),
            normalized_path_key(&dir_b)
        );
        assert_eq!(merged_a.children[0].size, 80);
        assert_eq!(merged_a.children[0].file_count, 2);
    }

    #[test]
    fn apply_direct_file_refreshes_updates_ancestors_without_rebuilding_tree() {
        let root = PathBuf::from("root");
        let dir_a = root.join("a");
        let dir_b = dir_a.join("b");

        let old_tree = vec![make_node(
            &dir_a,
            140,
            3,
            2,
            vec![make_node(&dir_b, 40, 1, 1, vec![])],
        )];

        let refreshed = vec![make_node(
            &dir_a,
            180,
            4,
            2,
            vec![make_node(&dir_b, 40, 1, 1, vec![])],
        )];
        let merged = apply_direct_file_refreshes(old_tree, refreshed, &root);

        assert_eq!(merged.len(), 1);
        let merged_a = &merged[0];
        assert_eq!(merged_a.size, 180);
        assert_eq!(merged_a.file_count, 4);
        assert_eq!(merged_a.dir_count, 2);
        assert_eq!(merged_a.children.len(), 1);
        assert_eq!(merged_a.children[0].size, 40);
        assert_eq!(merged_a.children[0].file_count, 1);
    }

    #[test]
    fn merge_scan_results_inserts_new_subtree_under_existing_parent() {
        let root = PathBuf::from("root");
        let dir_a = root.join("a");
        let dir_b = dir_a.join("b");

        let old_tree = vec![make_node(&dir_a, 100, 2, 1, vec![])];

        let merged = merge_scan_results(
            old_tree,
            vec![make_node(&dir_b, 30, 1, 1, vec![])],
            vec![],
            &root,
        );

        assert_eq!(merged.len(), 1);
        let merged_a = &merged[0];
        assert_eq!(merged_a.size, 130);
        assert_eq!(merged_a.file_count, 3);
        assert_eq!(merged_a.dir_count, 2);
        assert_eq!(merged_a.children.len(), 1);
        assert_eq!(merged_a.children[0].path, dir_b.to_string_lossy());
        assert!(merged_a.has_children);
    }

    #[test]
    fn merge_scan_results_handles_deep_nesting_with_multiple_changes() {
        let root = PathBuf::from("root");
        let dir_a = root.join("a");
        let dir_ab = dir_a.join("b");
        let dir_abc = dir_ab.join("c");
        let dir_abc_x = dir_abc.join("x");
        let dir_abc_y = dir_abc.join("y");

        let old_tree = vec![make_node(
            &dir_a,
            300,
            6,
            4,
            vec![make_node(
                &dir_ab,
                300,
                6,
                3,
                vec![make_node(
                    &dir_abc,
                    300,
                    6,
                    2,
                    vec![
                        make_node(&dir_abc_x, 100, 2, 1, vec![]),
                        make_node(&dir_abc_y, 200, 4, 1, vec![]),
                    ],
                )],
            )],
        )];

        let merged = merge_scan_results(
            old_tree,
            vec![
                make_node(&dir_abc_x, 50, 1, 1, vec![]),
                make_node(&dir_abc_y, 250, 5, 1, vec![]),
            ],
            vec![],
            &root,
        );

        let merged_a = &merged[0];
        assert_eq!(merged_a.size, 300);
        assert_eq!(merged_a.file_count, 6);

        let merged_ab = &merged_a.children[0];
        assert_eq!(merged_ab.size, 300);
        assert_eq!(merged_ab.file_count, 6);

        let merged_abc = &merged_ab.children[0];
        assert_eq!(merged_abc.size, 300);
        assert_eq!(merged_abc.file_count, 6);
        assert_eq!(merged_abc.children.len(), 2);
    }

    #[test]
    fn merge_scan_results_combines_delete_and_upsert_in_one_pass() {
        let root = PathBuf::from("root");
        let dir_a = root.join("a");
        let dir_old = dir_a.join("old");
        let dir_new = dir_a.join("new");

        let old_tree = vec![make_node(
            &dir_a,
            150,
            4,
            2,
            vec![make_node(&dir_old, 80, 2, 1, vec![])],
        )];

        let merged = merge_scan_results(
            old_tree,
            vec![make_node(&dir_new, 200, 5, 1, vec![])],
            vec![dir_old.to_string_lossy().to_string()],
            &root,
        );

        let merged_a = &merged[0];
        assert_eq!(merged_a.children.len(), 1);
        assert_eq!(merged_a.children[0].path, dir_new.to_string_lossy());
        assert_eq!(merged_a.size, 270);
        assert_eq!(merged_a.file_count, 7);
        assert_eq!(merged_a.dir_count, 2);
    }

    #[test]
    fn merge_scan_results_falls_back_when_changing_root_path() {
        let root = PathBuf::from("root");
        let dir_a = root.join("a");

        let old_tree = vec![make_node(&dir_a, 100, 2, 1, vec![])];

        let merged = merge_scan_results(
            old_tree,
            vec![make_node(&root, 999, 999, 999, vec![])],
            vec![],
            &root,
        );

        assert!(!merged.is_empty());
    }

    #[test]
    fn merge_scan_results_handles_overlapping_parent_and_child_changes() {
        let root = PathBuf::from("root");
        let dir_a = root.join("a");
        let dir_b = dir_a.join("b");

        let old_tree = vec![make_node(
            &dir_a,
            300,
            6,
            2,
            vec![make_node(&dir_b, 200, 4, 1, vec![])],
        )];

        let parent_change = make_node(
            &dir_a,
            350,
            7,
            2,
            vec![make_node(&dir_b, 220, 5, 1, vec![])],
        );
        let child_change = make_node(&dir_b, 240, 6, 1, vec![]);

        let merged = merge_scan_results(
            old_tree,
            vec![child_change, parent_change],
            vec![],
            &root,
        );

        let merged_a = &merged[0];
        let merged_b = &merged_a.children[0];
        assert_eq!(merged_b.size, 240);
        assert_eq!(merged_b.file_count, 6);
        assert!(merged_a.size >= merged_b.size);
        assert!(merged_a.file_count >= merged_b.file_count);
    }
}
