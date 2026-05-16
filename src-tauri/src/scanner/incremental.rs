use super::disk_scanner::{DiskScanner, ScanProgressEmitter};
use super::file_info::{DirectoryNode, FileInfo, ScanResult};
use super::progress::ScanProgress;
use super::timing::StageTimer;
use crate::winfs;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Runtime};

type IncrementalProgressEmitter = std::sync::Arc<dyn Fn(IncrementalScanProgress) + Send + Sync>;

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
struct TreeMergeHealth {
    unique_dir_nodes: usize,
    duplicate_paths: usize,
    orphan_root_count: usize,
    orphan_root_samples: Vec<String>,
    inconsistent_node_count: usize,
    inconsistent_node_samples: Vec<String>,
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
fn normalized_path_key(path: &Path) -> String {
    let mut text = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    while text.ends_with('\\') && text.len() > 3 {
        text.pop();
    }
    text
}

#[cfg(not(windows))]
fn normalized_path_key(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn normalized_path_key_str(path: &str) -> String {
    normalized_path_key(Path::new(path))
}

fn path_matches(node_path: &str, other: &Path) -> bool {
    normalized_path_key_str(node_path) == normalized_path_key(other)
}

fn path_starts_with(candidate: &Path, prefix: &Path) -> bool {
    let candidate_components: Vec<String> = candidate
        .components()
        .map(|component| {
            #[cfg(windows)]
            {
                component.as_os_str().to_string_lossy().to_ascii_lowercase()
            }
            #[cfg(not(windows))]
            {
                component.as_os_str().to_string_lossy().to_string()
            }
        })
        .collect();
    let prefix_components: Vec<String> = prefix
        .components()
        .map(|component| {
            #[cfg(windows)]
            {
                component.as_os_str().to_string_lossy().to_ascii_lowercase()
            }
            #[cfg(not(windows))]
            {
                component.as_os_str().to_string_lossy().to_string()
            }
        })
        .collect();

    candidate_components.starts_with(&prefix_components)
}

#[allow(dead_code)]
fn path_starts_with_str(candidate: &str, prefix: &Path) -> bool {
    path_starts_with(Path::new(candidate), prefix)
}

pub fn check_directory_changes(cached_node: &DirectoryNode, current_path: &Path) -> ChangeStatus {
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

    match (cached_node.modified_time, current_modified) {
        (Some(cached_time), Some(current_time)) if cached_time == current_time => {
            ChangeStatus::Unchanged
        }
        _ => ChangeStatus::Modified,
    }
}

pub fn detect_changes_recursive(
    cached_tree: &[DirectoryNode],
    current_path: &Path,
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
                match check_directory_changes(node, &entry_path) {
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
                        changes.extend(detect_changes_recursive(&node.children, &entry_path));
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

fn detect_changes_via_usn(
    path: &Path,
    cached_result: &ScanResult,
) -> Option<(Vec<ChangedDirectory>, bool)> {
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
            return None;
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
        format!("collect_usn_changed_dirs path={}", path.display()),
    );
    let change_set = match winfs::collect_usn_changed_dirs(
        path,
        checkpoint,
        cached_result.root_file_id,
        &file_id_map,
    ) {
        Ok(Some(changes)) => {
            usn_collect_timer.finish_with(format!(
                "recursive_dirs={} direct_file_dirs={} root_files_changed={}",
                changes.recursive_dirs.len(),
                changes.direct_file_dirs.len(),
                changes.root_files_changed
            ));
            changes
        }
        Ok(None) => {
            usn_collect_timer.finish_with("status=unavailable");
            total_timer.finish_with("status=unavailable");
            return None;
        }
        Err(err) => {
            usn_collect_timer.finish_with(format!("status=error err={err}"));
            total_timer.finish_with(format!("status=error err={err}"));
            return None;
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
        "status=complete total_changes={} root_files_changed={}",
        changes.len() + usize::from(change_set.root_files_changed),
        change_set.root_files_changed
    ));

    Some((changes, change_set.root_files_changed))
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
    nodes.sort_by(|a, b| b.size.cmp(&a.size));
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

    large_files.sort_by(|a, b| b.size.cmp(&a.size));
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
) -> Option<(DirectoryNode, Vec<FileInfo>)> {
    rescan_directory_tree(path, large_file_threshold)
        .map(|result| (result.node, result.large_files))
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

    let mut all_paths: Vec<PathBuf> = dir_nodes.keys().cloned().collect();
    all_paths.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

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
    large_files.sort_by(|a, b| b.size.cmp(&a.size));

    Some(RescannedDirectory {
        node,
        large_files,
        mode: RescanMode::Recursive,
    })
}

pub async fn scan_incremental<R: Runtime>(
    path: &Path,
    cached_result: ScanResult,
    app: AppHandle<R>,
    scanner: &DiskScanner,
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
    )
    .await
}

pub async fn scan_incremental_silent(
    path: &Path,
    cached_result: ScanResult,
    scanner: &DiskScanner,
) -> Result<ScanResult> {
    scan_incremental_internal(path, cached_result, scanner, None, None).await
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
        .scan_deep_with_progress(path, deep_progress_emitter.cloned(), estimated_files)
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
    let (changes, mut root_files_changed, detection_mode) =
        if let Some((changes, root_files_changed)) = detect_changes_via_usn(path, &cached_result) {
            (changes, root_files_changed, "usn")
        } else {
            let (current_root_size, current_root_files, _) =
                scan_root_files(path, large_file_threshold);
            let (cached_root_size, cached_root_files) = cached_result
                .directories
                .iter()
                .find(|node| path_matches(&node.path, path))
                .map(|node| (node.size, node.file_count))
                .unwrap_or((0, 0));
            let root_files_changed =
                cached_root_size != current_root_size || cached_root_files != current_root_files;
            (
                detect_changes_recursive(&cached_result.directories, path),
                root_files_changed,
                "mtime",
            )
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
        "[阶段1] 变化检测完成，耗时: {:.2}ms | mode={}",
        detect_start.elapsed().as_secs_f64() * 1000.0,
        detection_mode
    );
    if detection_mode == "mtime" {
        if cached_result.usn_journal_id.is_some() && cached_result.usn_next_usn.is_some() {
            tracing::info!("[阶段1] USN checkpoint 存在，但本次未能直接使用，已回退到 mtime 递归检测");
        } else {
            tracing::info!("[阶段1] 缓存缺少 USN checkpoint，本次只能使用 mtime 递归检测");
        }
    } else {
        tracing::info!("[阶段1] 本次增量检测使用了 USN 日志");
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
        )
        .await?;
        total_timer.finish_with(format!(
            "status=fallback_change_ratio backend={:?} files={} dirs={} size={}",
            result.scan_backend, result.total_files, result.total_dirs, result.total_size
        ));
        return Ok(result);
    }

    // 阶段2: 重新扫描修改过的目录
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
        let result = merge_scan_results(
            cached_directories,
            rescanned_dirs
                .iter()
                .map(|item| item.node.clone())
                .collect(),
            deleted_paths,
            path,
        );
        merge_inner_timer.finish_with(format!("nodes={}", result.len()));
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
    if merge_health.duplicate_paths > 0
        || merge_health.orphan_root_count > 0
        || merge_health.inconsistent_node_count > 0
        || merge_health.unique_dir_nodes != new_total_dirs
    {
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
    let deleted_pathbufs: Vec<PathBuf> = changes
        .iter()
        .filter(|c| c.status == ChangeStatus::Deleted)
        .map(|c| c.path.clone())
        .collect();

    let rescanned_scopes: Vec<(PathBuf, RescanMode)> = rescanned_dirs
        .iter()
        .map(|item| (PathBuf::from(&item.node.path), item.mode))
        .collect();

    let mut large_files: Vec<FileInfo> = cached_large_files
        .into_iter()
        .filter(|file| {
            let file_path = Path::new(&file.path);
            !deleted_pathbufs
                .iter()
                .any(|deleted| path_starts_with(file_path, deleted))
                && !rescanned_scopes
                    .iter()
                    .any(|(changed_path, mode)| match mode {
                        RescanMode::Recursive => path_starts_with(file_path, changed_path),
                        RescanMode::DirectFilesOnly => {
                            file_path.parent() == Some(changed_path.as_path())
                        }
                    })
                && !(root_files_changed && file_path.parent() == Some(path))
        })
        .collect();

    for rescanned in rescanned_dirs {
        large_files.extend(rescanned.large_files);
    }

    if root_files_changed {
        large_files.extend(root_large_files);
    }

    large_files.sort_by(|a, b| b.size.cmp(&a.size));
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

    Ok(ScanResult {
        root_path: cached_root_path,
        total_size: new_total_size,
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
    nodes.sort_by(|a, b| b.size.cmp(&a.size));
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
        nodes: &mut Vec<DirectoryNode>,
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
            nodes.sort_by(|a, b| b.size.cmp(&a.size));
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

pub fn merge_scan_results(
    old_tree: Vec<DirectoryNode>,
    changed_dirs: Vec<DirectoryNode>,
    deleted_paths: Vec<String>,
    root_path: &Path,
) -> Vec<DirectoryNode> {
    merge_scan_results_flat(old_tree, changed_dirs, deleted_paths, root_path)
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

fn inspect_tree_merge_health(nodes: &[DirectoryNode], root_path: &Path) -> TreeMergeHealth {
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
            if node.dir_count != expected_dir_count {
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
            health.inconsistent_node_count += 1;
            if health.inconsistent_node_samples.len() < 5 {
                health.inconsistent_node_samples.push(format!(
                    "{} size={} child_sum={}",
                    node.path, node.size, child_size_sum
                ));
            }
        }
        if node.file_count < child_file_sum {
            health.inconsistent_node_count += 1;
            if health.inconsistent_node_samples.len() < 5 {
                health.inconsistent_node_samples.push(format!(
                    "{} file_count={} child_sum={}",
                    node.path, node.file_count, child_file_sum
                ));
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
}
