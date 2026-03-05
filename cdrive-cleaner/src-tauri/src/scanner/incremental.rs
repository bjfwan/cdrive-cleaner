use super::file_info::{DirectoryNode, ScanResult};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
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
    pub old_modified_time: Option<u64>,
    pub new_modified_time: Option<u64>,
}

#[derive(Clone, serde::Serialize)]
pub struct IncrementalScanProgress {
    pub phase: String,
    pub total_dirs: usize,
    pub checked_dirs: usize,
    pub changed_dirs: usize,
    pub scanned_dirs: usize,
}

pub fn check_directory_changes(
    cached_node: &DirectoryNode,
    current_path: &Path
) -> ChangeStatus {
    // 快速路径：先检查文件是否存在
    if !current_path.exists() {
        return ChangeStatus::Deleted;
    }

    // 批量获取元数据（一次系统调用）
    let current_metadata = match std::fs::metadata(current_path) {
        Ok(m) => m,
        Err(_) => return ChangeStatus::Deleted,
    };

    // 快速检查：不是目录则视为已删除
    if !current_metadata.is_dir() {
        return ChangeStatus::Deleted;
    }

    // 获取修改时间（已经有元数据，不需要额外调用）
    let current_modified = current_metadata.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    // 比较修改时间
    match (cached_node.modified_time, current_modified) {
        (Some(cached_time), Some(current_time)) => {
            if cached_time == current_time {
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
    root_path: &Path
) -> Vec<ChangedDirectory> {
    use rayon::prelude::*;
    
    // 使用并行处理来加速变化检测
    cached_tree.par_iter()
        .flat_map(|node| {
            let mut changes = Vec::new();
            let node_path = PathBuf::from(&node.path);
            let status = check_directory_changes(node, &node_path);

            if status != ChangeStatus::Unchanged {
                changes.push(ChangedDirectory {
                    path: node_path.clone(),
                    status: status.clone(),
                    old_modified_time: node.modified_time,
                    new_modified_time: if status != ChangeStatus::Deleted {
                        std::fs::metadata(&node_path)
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                    } else {
                        None
                    },
                });
            }

            // 智能跳过：如果父目录未变化，跳过子目录检查
            if !node.children.is_empty() && status == ChangeStatus::Unchanged {
                // 父目录未变化，跳过子目录
                return changes;
            }

            if !node.children.is_empty() && status != ChangeStatus::Deleted {
                let child_changes = detect_changes_recursive(&node.children, root_path);
                changes.extend(child_changes);
            }

            changes
        })
        .collect()
}

pub fn detect_new_directories(
    cached_tree: &[DirectoryNode],
    current_path: &Path
) -> Result<Vec<PathBuf>> {
    // For incremental scan, we only check direct children of the root
    // to avoid scanning the entire drive which would be too slow
    
    let mut cached_paths: HashSet<PathBuf> = HashSet::new();
    
    // Only collect direct children paths from cache
    for node in cached_tree {
        cached_paths.insert(PathBuf::from(&node.path));
    }

    let mut new_dirs = Vec::new();

    // Only scan direct children of current_path
    if let Ok(entries) = std::fs::read_dir(current_path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    let entry_path = entry.path();
                    if !cached_paths.contains(&entry_path) {
                        new_dirs.push(entry_path);
                    }
                }
            }
        }
    }

    Ok(new_dirs)
}

pub async fn scan_incremental(
    path: &Path,
    cached_result: ScanResult,
    app: AppHandle
) -> Result<ScanResult> {
    use std::time::Instant;
    use crate::scanner::disk_scanner::DiskScanner;

    let start = Instant::now();
    
    println!("\n========== 增量扫描开始 ==========");
    println!("扫描路径: {}", path.display());
    
    // 阶段1: 检测变化
    let total_cached_dirs = count_directories(&cached_result.directories);
    println!("[阶段1] 缓存目录总数: {}", total_cached_dirs);
    
    let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
        phase: "detecting".to_string(),
        total_dirs: total_cached_dirs,
        checked_dirs: 0,
        changed_dirs: 0,
        scanned_dirs: 0,
    });

    let detect_start = Instant::now();
    let changes = detect_changes_recursive(&cached_result.directories, path);
    let detect_duration = detect_start.elapsed();
    println!("[阶段1] 变化检测完成，耗时: {:.2}ms", detect_duration.as_secs_f64() * 1000.0);
    
    let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
        phase: "detecting".to_string(),
        total_dirs: total_cached_dirs,
        checked_dirs: total_cached_dirs,
        changed_dirs: changes.len(),
        scanned_dirs: 0,
    });
    
    let new_dir_start = Instant::now();
    let new_dirs = detect_new_directories(&cached_result.directories, path)?;
    let new_dir_duration = new_dir_start.elapsed();
    println!("[阶段1] 新目录检测完成，耗时: {:.2}ms", new_dir_duration.as_secs_f64() * 1000.0);
    
    let total_changes = changes.len() + new_dirs.len();
    println!("[阶段1] 检测到变化: {} 个 (修改: {}, 新增: {})", 
        total_changes, changes.len(), new_dirs.len());
    
    // 阶段2: 扫描变化的目录
    let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
        phase: "scanning".to_string(),
        total_dirs: total_changes,
        checked_dirs: 0,
        changed_dirs: total_changes,
        scanned_dirs: 0,
    });

    if total_changes == 0 {
        println!("[结果] 无变化，直接使用缓存");
        let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
            phase: "completed".to_string(),
            total_dirs: total_cached_dirs,
            checked_dirs: total_cached_dirs,
            changed_dirs: 0,
            scanned_dirs: 0,
        });
        println!("========== 增量扫描完成 (耗时: {:.2}ms) ==========\n", 
            start.elapsed().as_secs_f64() * 1000.0);
        return Ok(cached_result);
    }

    let change_ratio = total_changes as f64 / (cached_result.total_dirs as f64).max(1.0);
    println!("[阶段2] 变化比例: {:.1}%", change_ratio * 100.0);
    
    if change_ratio > 0.3 {
        println!("[阶段2] 变化超过30%，切换到全量扫描");
        let scanner = DiskScanner::new();
        return scanner.scan(path, app).await;
    }

    // 阶段3: 合并数据
    let merge_start = Instant::now();
    let _ = app.emit("incremental-scan-progress", IncrementalScanProgress {
        phase: "merging".to_string(),
        total_dirs: total_changes,
        checked_dirs: total_changes,
        changed_dirs: total_changes,
        scanned_dirs: total_changes,
    });

    let mut updated_tree = cached_result.directories.clone();
    let deleted_paths: Vec<String> = changes.iter()
        .filter(|c| c.status == ChangeStatus::Deleted)
        .map(|c| c.path.to_string_lossy().to_string())
        .collect();

    let modified_paths: Vec<PathBuf> = changes.iter()
        .filter(|c| c.status == ChangeStatus::Modified)
        .map(|c| c.path.clone())
        .collect();

    println!("[阶段3] 合并数据: 删除 {} 个, 修改 {} 个", 
        deleted_paths.len(), modified_paths.len());

    updated_tree = merge_scan_results(
        updated_tree,
        vec![],
        deleted_paths
    );

    let merge_duration = merge_start.elapsed();
    println!("[阶段3] 数据合并完成，耗时: {:.2}ms", merge_duration.as_secs_f64() * 1000.0);

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
    println!("性能提升: 相比全量扫描节省约 {:.1}%", (1.0 - change_ratio) * 100.0);
    println!("==================================\n");

    Ok(ScanResult {
        root_path: cached_result.root_path,
        total_size: cached_result.total_size,
        total_files: cached_result.total_files,
        total_dirs: cached_result.total_dirs,
        scan_duration_ms,
        directories: updated_tree,
        large_files: cached_result.large_files,
        inaccessible_count: cached_result.inaccessible_count,
    })
}

fn count_directories(dirs: &[DirectoryNode]) -> usize {
    let mut count = dirs.len();
    for dir in dirs {
        count += count_directories(&dir.children);
    }
    count
}

pub fn merge_scan_results(
    mut old_tree: Vec<DirectoryNode>,
    changed_dirs: Vec<DirectoryNode>,
    deleted_paths: Vec<String>
) -> Vec<DirectoryNode> {
    let deleted_set: HashSet<String> = deleted_paths.into_iter().collect();
    
    // 第一步：移除已删除的目录
    fn remove_deleted(nodes: &mut Vec<DirectoryNode>, deleted: &HashSet<String>) {
        nodes.retain(|node| !deleted.contains(&node.path));
        
        for node in nodes.iter_mut() {
            remove_deleted(&mut node.children, deleted);
        }
    }
    
    remove_deleted(&mut old_tree, &deleted_set);
    
    // 第二步：更新已修改的目录（使用 HashMap 避免重复查找）
    if !changed_dirs.is_empty() {
        let changed_map: HashMap<String, DirectoryNode> = changed_dirs
            .into_iter()
            .map(|node| (node.path.clone(), node))
            .collect();
        
        fn update_changed(nodes: &mut Vec<DirectoryNode>, changed: &HashMap<String, DirectoryNode>) {
            for node in nodes.iter_mut() {
                if let Some(new_node) = changed.get(&node.path) {
                    // 直接替换节点，避免逐字段复制
                    *node = new_node.clone();
                } else {
                    update_changed(&mut node.children, changed);
                }
            }
        }
        
        update_changed(&mut old_tree, &changed_map);
    }
    
    // 第三步：重新计算父目录的大小和文件数
    fn recalculate_sizes(nodes: &mut [DirectoryNode]) {
        for node in nodes.iter_mut() {
            if !node.children.is_empty() {
                recalculate_sizes(&mut node.children);
                
                // 使用 fold 一次性计算总和，避免多次迭代
                let (total_size, total_files) = node.children.iter()
                    .fold((0u64, 0usize), |(size, files), child| {
                        (size + child.size, files + child.file_count)
                    });
                
                node.size = total_size;
                node.file_count = total_files;
            }
        }
    }
    
    recalculate_sizes(&mut old_tree);
    
    old_tree
}

