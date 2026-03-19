use super::file_info::{DirectoryNode, FileInfo, ScanResult};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use anyhow::Result;
use tauri::{AppHandle, Emitter};

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
}

#[derive(Debug, Clone)]
struct RescannedDirectory {
    node: DirectoryNode,
    large_files: Vec<FileInfo>,
}

#[derive(Clone, serde::Serialize)]
pub struct IncrementalScanProgress {
    pub phase: String,
    pub total_dirs: usize,
    pub checked_dirs: usize,
    pub changed_dirs: usize,
    pub scanned_dirs: usize,
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
    std::fs::metadata(path).map(|metadata| metadata.is_dir()).unwrap_or(false)
}

fn resolve_link_target(path: &Path) -> Option<String> {
    let resolved = if let Ok(target) = std::fs::read_link(path) {
        if target.is_absolute() {
            target
        } else if let Some(parent) = path.parent() {
            parent.join(&target)
        } else {
            target
        }
    } else {
        let canonical = std::fs::canonicalize(path).ok()?;
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

    let current_modified = current_metadata.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    match (cached_node.modified_time, current_modified) {
        (Some(cached_time), Some(current_time)) if cached_time == current_time => ChangeStatus::Unchanged,
        _ => ChangeStatus::Modified,
    }
}

pub fn detect_changes_recursive(cached_tree: &[DirectoryNode], current_path: &Path) -> Vec<ChangedDirectory> {
    let current_path_str = current_path.to_string_lossy().to_string();
    let cached_nodes: Vec<&DirectoryNode> = cached_tree.iter()
        .filter(|node| node.path != current_path_str)
        .collect();
    let cached_map: HashMap<String, &DirectoryNode> = cached_nodes.iter()
        .map(|node| (node.path.clone(), *node))
        .collect();
    let mut current_dirs = HashSet::new();
    let mut changes = Vec::new();

    if let Ok(entries) = std::fs::read_dir(current_path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&entry_path) else { continue; };
            let is_directory = if is_link_entry(&metadata) {
                path_points_to_directory(&entry_path)
            } else {
                metadata.is_dir()
            };

            if !is_directory {
                continue;
            }

            let entry_str = entry_path.to_string_lossy().to_string();
            current_dirs.insert(entry_str.clone());

            if let Some(node) = cached_map.get(&entry_str) {
                match check_directory_changes(node, &entry_path) {
                    ChangeStatus::Deleted => changes.push(ChangedDirectory {
                        path: entry_path,
                        status: ChangeStatus::Deleted,
                    }),
                    ChangeStatus::Modified => changes.push(ChangedDirectory {
                        path: entry_path,
                        status: ChangeStatus::Modified,
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
                });
            }
        }
    }

    for node in cached_nodes {
        if !current_dirs.contains(&node.path) {
            changes.push(ChangedDirectory {
                path: PathBuf::from(&node.path),
                status: ChangeStatus::Deleted,
            });
        }
    }

    changes
}

fn build_file_info(path: &Path, metadata: &std::fs::Metadata) -> FileInfo {
    let modified_at = metadata.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();

    FileInfo {
        path: path.to_string_lossy().to_string(),
        name: path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown").to_string(),
        size: metadata.len(),
        extension: path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string(),
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

fn scan_root_files(path: &Path, large_file_threshold: u64) -> (u64, usize, Vec<FileInfo>) {
    let mut total_size = 0u64;
    let mut total_files = 0usize;
    let mut large_files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&entry_path) else { continue; };
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
    (total_size, total_files, large_files)
}

/// 重新扫描单个目录，返回更新后的 DirectoryNode
fn rescan_directory(path: &Path, large_file_threshold: u64) -> Option<RescannedDirectory> {
    rescan_directory_tree(path, large_file_threshold)
}

fn rescan_directory_tree(path: &Path, large_file_threshold: u64) -> Option<RescannedDirectory> {
    let root_metadata = std::fs::symlink_metadata(path).ok()?;
    if is_link_entry(&root_metadata) {
        if !path_points_to_directory(path) {
            return None;
        }

        let modified_time = root_metadata.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        return Some(RescannedDirectory {
            node: DirectoryNode {
                path: path.to_string_lossy().to_string(),
                name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                size: 0,
                file_count: 0,
                dir_count: 1,
                children: vec![],
                has_children: false,
                is_symlink: true,
                link_target: resolve_link_target(path),
                safety: None,
                modified_time,
            },
            large_files: vec![],
        });
    }

    if !root_metadata.is_dir() {
        return None;
    }

    let mut dir_file_stats: HashMap<PathBuf, (u64, usize)> = HashMap::new();
    let mut dir_nodes: HashMap<PathBuf, DirectoryNode> = HashMap::new();
    let mut large_files = Vec::new();

    for entry in jwalk::WalkDir::new(path).skip_hidden(false).follow_links(false) {
        let Ok(entry) = entry else { continue; };
        let entry_path = entry.path();
        let Ok(link_metadata) = std::fs::symlink_metadata(&entry_path) else { continue; };

        if is_link_entry(&link_metadata) {
            if path_points_to_directory(&entry_path) {
                let modified_time = link_metadata.modified().ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());

                dir_nodes.insert(entry_path.to_path_buf(), DirectoryNode {
                    path: entry_path.to_string_lossy().to_string(),
                    name: entry_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                    size: 0,
                    file_count: 0,
                    dir_count: 1,
                    children: vec![],
                    has_children: false,
                    is_symlink: true,
                    link_target: resolve_link_target(&entry_path),
                    safety: None,
                    modified_time,
                });
            }
            continue;
        }

        let Ok(metadata) = entry.metadata() else { continue; };

        if metadata.is_file() {
            if let Some(parent) = entry_path.parent() {
                let stats = dir_file_stats.entry(parent.to_path_buf()).or_insert((0, 0));
                stats.0 += metadata.len();
                stats.1 += 1;
            }
            if metadata.len() >= large_file_threshold {
                large_files.push(build_file_info(&entry_path, &metadata));
            }
        } else if metadata.is_dir() {
            let modified_time = metadata.modified().ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());

            dir_nodes.insert(entry_path.to_path_buf(), DirectoryNode {
                path: entry_path.to_string_lossy().to_string(),
                name: entry_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                size: 0,
                file_count: 0,
                dir_count: 1,
                children: vec![],
                has_children: false,
                is_symlink: false,
                link_target: None,
                safety: None,
                modified_time,
            });
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

    Some(RescannedDirectory { node, large_files })
}

pub async fn scan_incremental(
    path: &Path,
    cached_result: ScanResult,
    app: AppHandle,
) -> Result<ScanResult> {
    use std::time::Instant;
    use crate::scanner::disk_scanner::DiskScanner;

    let start = Instant::now();
    let large_file_threshold = 100 * 1024 * 1024u64;
    let root_path_str = path.to_string_lossy().to_string();

    println!("\n========== 增量扫描开始 ==========");
    println!("扫描路径: {}", path.display());

    let total_cached_dirs = count_directories(&cached_result.directories, &root_path_str);
    println!("[阶段1] 缓存目录总数: {}", total_cached_dirs);

    let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
        phase: "detecting".to_string(),
        total_dirs: total_cached_dirs,
        checked_dirs: 0,
        changed_dirs: 0,
        scanned_dirs: 0,
    });

    // 阶段1: 检测变化
    let detect_start = Instant::now();
    let changes = detect_changes_recursive(&cached_result.directories, path);
    let (current_root_size, current_root_files, root_large_files) = scan_root_files(path, large_file_threshold);
    let (cached_root_size, cached_root_files) = cached_result.directories.iter()
        .find(|node| node.path == root_path_str)
        .map(|node| (node.size, node.file_count))
        .unwrap_or((0, 0));
    let root_files_changed = cached_root_size != current_root_size || cached_root_files != current_root_files;
    println!("[阶段1] 变化检测完成，耗时: {:.2}ms", detect_start.elapsed().as_secs_f64() * 1000.0);

    let change_dirs = changes.iter().filter(|c| c.status != ChangeStatus::Deleted).count();
    let delete_dirs = changes.iter().filter(|c| c.status == ChangeStatus::Deleted).count();
    let total_changes = changes.len() + usize::from(root_files_changed);
    let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
        phase: "detecting".to_string(),
        total_dirs: total_cached_dirs,
        checked_dirs: total_cached_dirs,
        changed_dirs: total_changes,
        scanned_dirs: 0,
    });

    println!("[阶段1] 总变化: {} 个 (重扫: {}, 删除: {}, 根文件变化: {})", total_changes, change_dirs, delete_dirs, root_files_changed);

    // 无变化，直接返回缓存
    if total_changes == 0 {
        println!("[结果] 无变化，直接使用缓存");
        let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
            phase: "completed".to_string(),
            total_dirs: total_cached_dirs,
            checked_dirs: total_cached_dirs,
            changed_dirs: 0,
            scanned_dirs: 0,
        });
        println!("========== 增量扫描完成 (耗时: {:.2}ms) ==========\n", start.elapsed().as_secs_f64() * 1000.0);
        return Ok(cached_result);
    }

    // 变化过多，回退到全量扫描
    let change_ratio = total_changes as f64 / (total_cached_dirs as f64).max(1.0);
    if change_ratio > 0.3 {
        println!("[阶段2] 变化超过30% ({:.1}%)，切换到全量扫描", change_ratio * 100.0);
        let scanner = DiskScanner::new();
        return scanner.scan_deep(path, app, cached_result.total_files.max(1)).await;
    }

    // 阶段2: 重新扫描修改过的目录
    let rescan_paths: Vec<&PathBuf> = changes.iter()
        .filter(|c| c.status != ChangeStatus::Deleted)
        .map(|c| &c.path)
        .collect();
    let total_rescans = rescan_paths.len() + usize::from(root_files_changed);
    let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
        phase: "scanning".to_string(),
        total_dirs: total_rescans,
        checked_dirs: 0,
        changed_dirs: total_changes,
        scanned_dirs: 0,
    });

    let deleted_paths: Vec<String> = changes.iter()
        .filter(|c| c.status == ChangeStatus::Deleted)
        .map(|c| c.path.to_string_lossy().to_string())
        .collect();

    // 重新扫描修改过的目录，得到最新数据
    let mut rescanned_dirs: Vec<RescannedDirectory> = Vec::new();
    for (i, rescan_path) in rescan_paths.iter().enumerate() {
        if let Some(node) = rescan_directory(rescan_path, large_file_threshold) {
            rescanned_dirs.push(node);
        }
        let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
            phase: "scanning".to_string(),
            total_dirs: total_rescans,
            checked_dirs: i + 1,
            changed_dirs: total_changes,
            scanned_dirs: i + 1,
        });
    }

    if root_files_changed {
        let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
            phase: "scanning".to_string(),
            total_dirs: total_rescans,
            checked_dirs: total_rescans,
            changed_dirs: total_changes,
            scanned_dirs: total_rescans,
        });
    }

    println!("[阶段2] 重新扫描了 {} 个目录, 删除 {} 个", rescanned_dirs.len(), deleted_paths.len());

    // 阶段3: 合并数据
    let merge_start = Instant::now();
    let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
        phase: "merging".to_string(),
        total_dirs: total_changes,
        checked_dirs: total_changes,
        changed_dirs: total_changes,
        scanned_dirs: total_changes,
    });

    let mut updated_tree = merge_scan_results(
        cached_result.directories.clone(),
        rescanned_dirs.iter().map(|item| item.node.clone()).collect(),
        deleted_paths,
        path,
    );
    upsert_root_files_node(&mut updated_tree, path, current_root_size, current_root_files);

    println!("[阶段3] 数据合并完成，耗时: {:.2}ms", merge_start.elapsed().as_secs_f64() * 1000.0);

    // 重新计算总数
    let (tree_size, tree_files) = sum_tree(&updated_tree);
    let (new_total_size, new_total_files) = (tree_size, tree_files);
    let new_total_dirs = count_directories(&updated_tree, &root_path_str);

    let deleted_pathbufs: Vec<PathBuf> = changes.iter()
        .filter(|c| c.status == ChangeStatus::Deleted)
        .map(|c| c.path.clone())
        .collect();

    let mut large_files: Vec<FileInfo> = cached_result.large_files.into_iter()
        .filter(|file| {
            let file_path = Path::new(&file.path);
            !deleted_pathbufs.iter().any(|deleted| file_path.starts_with(deleted))
                && !rescan_paths.iter().any(|changed| file_path.starts_with(changed.as_path()))
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
    let scan_duration_ms = start.elapsed().as_millis() as u64;

    let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
        phase: "completed".to_string(),
        total_dirs: total_cached_dirs,
        checked_dirs: total_cached_dirs,
        changed_dirs: total_changes,
        scanned_dirs: total_changes,
    });

    println!("\n========== 增量扫描完成 ==========");
    println!("总耗时: {:.2}ms", scan_duration_ms as f64);
    println!("更新后: {:.2} GB, {} 文件", new_total_size as f64 / 1024.0 / 1024.0 / 1024.0, new_total_files);
    println!("==================================\n");

    Ok(ScanResult {
        root_path: cached_result.root_path,
        total_size: new_total_size,
        total_files: new_total_files,
        total_dirs: new_total_dirs,
        scan_duration_ms,
        directories: updated_tree,
        large_files,
        inaccessible_count: cached_result.inaccessible_count,
    })
}

fn count_directories(dirs: &[DirectoryNode], root_path: &str) -> usize {
    dirs.iter().map(|node| {
        if node.path == root_path {
            0
        } else if node.dir_count > 0 {
            node.dir_count
        } else {
            1 + count_directories(&node.children, root_path)
        }
    }).sum()
}

fn sum_tree(dirs: &[DirectoryNode]) -> (u64, usize) {
    dirs.iter().fold((0u64, 0usize), |(s, f), d| (s + d.size, f + d.file_count))
}

fn upsert_root_files_node(nodes: &mut Vec<DirectoryNode>, root_path: &Path, root_size: u64, root_files: usize) {
    let root_path_str = root_path.to_string_lossy().to_string();
    nodes.retain(|node| node.path != root_path_str);

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
        });
    }

    sort_directory_tree(nodes);
}

fn flatten_tree(nodes: Vec<DirectoryNode>, flat: &mut HashMap<String, DirectoryNode>) {
    for mut node in nodes {
        let children = std::mem::take(&mut node.children);
        flat.insert(node.path.clone(), node);
        flatten_tree(children, flat);
    }
}

fn rebuild_tree(mut flat: HashMap<String, DirectoryNode>, root_path: &Path) -> Vec<DirectoryNode> {
    let root_path_str = root_path.to_string_lossy().to_string();
    let mut paths: Vec<String> = flat.keys().cloned().collect();
    paths.sort_by(|a, b| Path::new(b).components().count().cmp(&Path::new(a).components().count()));

    let mut roots = Vec::new();
    for path in paths {
        let Some(node) = flat.remove(&path) else { continue; };

        if path != root_path_str {
            if let Some(parent_path) = Path::new(&path).parent() {
                if parent_path != root_path {
                    let parent_key = parent_path.to_string_lossy().to_string();
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

pub fn merge_scan_results(
    mut old_tree: Vec<DirectoryNode>,
    changed_dirs: Vec<DirectoryNode>,
    deleted_paths: Vec<String>,
    root_path: &Path,
) -> Vec<DirectoryNode> {
    let deleted_paths: Vec<PathBuf> = deleted_paths.into_iter().map(PathBuf::from).collect();

    // 移除已删除的目录
    fn remove_deleted(nodes: &mut Vec<DirectoryNode>, deleted: &[PathBuf]) {
        nodes.retain(|node| !deleted.iter().any(|deleted_path| Path::new(&node.path).starts_with(deleted_path)));
        for node in nodes.iter_mut() {
            remove_deleted(&mut node.children, deleted);
        }
    }
    remove_deleted(&mut old_tree, &deleted_paths);

    // 更新已修改的目录 + 添加新目录
    let mut flat = HashMap::new();
    flatten_tree(old_tree, &mut flat);

    for changed_dir in changed_dirs {
        let changed_path = PathBuf::from(&changed_dir.path);
        flat.retain(|path, _| !Path::new(path).starts_with(&changed_path));
        flatten_tree(vec![changed_dir], &mut flat);
    }

    rebuild_tree(flat, root_path)
}
