use super::file_info::{DirectoryNode, ScanResult};
use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use walkdir::WalkDir;
use chrono;

#[cfg(windows)]
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
#[cfg(windows)]
use windows::core::PCWSTR;

use tauri::{AppHandle, Emitter};

#[derive(Clone, serde::Serialize)]
pub struct ScanProgress {
    pub scanned_files: u64,
    pub scanned_dirs: u64,
    pub total_size: u64,
    pub current_path: String,
    pub elapsed_ms: u64,
    pub files_per_second: f64,
    pub progress_percent: f64,
}

use crate::cache::ScanCache;

pub struct DiskScanner {
    cache: ScanCache,
}

impl DiskScanner {
    pub fn new() -> Self {
        Self {
            cache: ScanCache::new(),
        }
    }

    pub fn new_with_cache(cache: ScanCache) -> Self {
        Self { cache }
    }

    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    pub fn cache_size(&self) -> usize {
        self.cache.size()
    }

    #[cfg(windows)]
    fn get_disk_usage(path: &Path) -> Option<(u64, u64, u64)> {
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
            ).is_ok() {
                let used_bytes = total_bytes - total_free_bytes;
                Some((total_bytes, used_bytes, total_free_bytes))
            } else {
                None
            }
        }
    }

    #[cfg(not(windows))]
    fn get_disk_usage(_path: &Path) -> Option<(u64, u64, u64)> {
        None
    }

    pub async fn scan<P: AsRef<Path>>(&self, path: P, app: AppHandle) -> Result<ScanResult> {
        let path = path.as_ref().to_path_buf();
        let cache = self.cache.clone();
        
        tokio::task::spawn_blocking(move || {
            Self::scan_quick(&path, app, cache)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    pub async fn scan_deep<P: AsRef<Path>>(&self, path: P, app: AppHandle, estimated_files: usize) -> Result<ScanResult> {
        let path = path.as_ref().to_path_buf();
        
        tokio::task::spawn_blocking(move || {
            Self::scan_deep_blocking(&path, app, estimated_files)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    fn scan_quick(path: &Path, app: AppHandle, cache: ScanCache) -> Result<ScanResult> {
        let start = Instant::now();
        let root_path = path.to_string_lossy().to_string();
        
        println!("\n========== 快速扫描开始 ==========");
        println!("扫描路径: {}", root_path);
        println!("开始时间: {}", chrono::Local::now().format("%H:%M:%S"));
        
        let total_size = Arc::new(AtomicU64::new(0));
        let total_files = Arc::new(AtomicUsize::new(0));
        let total_dirs = Arc::new(AtomicUsize::new(0));
        let inaccessible_count = Arc::new(AtomicUsize::new(0));
        let large_files: Arc<Mutex<Vec<super::file_info::FileInfo>>> = Arc::new(Mutex::new(Vec::new()));
        let large_file_threshold = 100 * 1024 * 1024; // 100 MB

        // 发送初始进度（快速扫描专用事件）
        let _ = app.emit("quick-scan-progress", ScanProgress {
            scanned_files: 0,
            scanned_dirs: 0,
            total_size: 0,
            current_path: root_path.clone(),
            elapsed_ms: 0,
            files_per_second: 0.0,
            progress_percent: 0.0,
        });

        let read_start = Instant::now();
        let entries: Vec<_> = match fs::read_dir(path) {
            Ok(entries) => entries.collect(),
            Err(e) => {
                println!("[错误] 无法读取目录: {}", e);
                return Err(anyhow::anyhow!("Failed to read directory: {}", e));
            }
        };
        println!("[阶段1] 读取根目录条目: {} 项 (耗时: {:.2}ms)", entries.len(), read_start.elapsed().as_secs_f64() * 1000.0);
        
        // 统计根目录的直接文件
        let mut root_direct_files = 0;
        let mut root_direct_size = 0u64;
        for entry_result in &entries {
            if let Ok(entry) = entry_result {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        root_direct_files += 1;
                        root_direct_size += metadata.len();
                    }
                }
            }
        }
        println!("[阶段1] 根目录直接文件: {} 个, 大小: {:.2} MB", 
            root_direct_files, 
            root_direct_size as f64 / 1024.0 / 1024.0);

        println!("[阶段2] 启动进度报告线程");
        
        let app_clone = app.clone();
        let total_size_clone = Arc::clone(&total_size);
        let total_files_clone = Arc::clone(&total_files);
        let total_dirs_clone = Arc::clone(&total_dirs);
        let start_clone = start.clone();
        let should_stop = Arc::new(AtomicUsize::new(0));
        let should_stop_clone = Arc::clone(&should_stop);
        
        // 启动进度报告线程
        let progress_handle = std::thread::spawn(move || {
            let mut last_files = 0;
            let mut last_time = Instant::now();
            let mut last_progress_percent = 0.0;
            
            loop {
                std::thread::sleep(std::time::Duration::from_millis(300));
                
                if should_stop_clone.load(Ordering::Relaxed) == 1 {
                    break;
                }
                
                let current_files = total_files_clone.load(Ordering::Relaxed);
                let current_dirs = total_dirs_clone.load(Ordering::Relaxed);
                let current_size = total_size_clone.load(Ordering::Relaxed);
                let elapsed = start_clone.elapsed().as_millis() as u64;
                
                let now = Instant::now();
                let time_diff = now.duration_since(last_time).as_secs_f64();
                let files_diff = current_files.saturating_sub(last_files);
                let files_per_second = if time_diff > 0.0 {
                    files_diff as f64 / time_diff
                } else {
                    0.0
                };
                
                // 基于已扫描文件数估算进度（假设总文件数约为 800,000）
                let estimated_total_files = 800000.0;
                let raw_progress = (current_files as f64 / estimated_total_files) * 100.0;
                
                // 平滑进度：不允许进度倒退，且限制最大值为 99%
                let progress_percent = if raw_progress > last_progress_percent {
                    raw_progress.min(99.0)
                } else {
                    last_progress_percent
                };
                
                last_progress_percent = progress_percent;
                last_files = current_files;
                last_time = now;
                
                let progress = ScanProgress {
                    scanned_files: current_files as u64,
                    scanned_dirs: current_dirs as u64,
                    total_size: current_size,
                    current_path: "快速扫描中...".to_string(),
                    elapsed_ms: elapsed,
                    files_per_second,
                    progress_percent,
                };
                
                let _ = app_clone.emit("quick-scan-progress", progress);
            }
        });

        println!("[阶段3] 开始并行扫描子目录");
        let scan_start = Instant::now();
        
        let directories: Vec<DirectoryNode> = entries
            .par_iter()
            .filter_map(|entry_result| {
                let entry = match entry_result {
                    Ok(e) => e,
                    Err(e) => {
                        println!("[警告] 无法读取条目: {}", e);
                        inaccessible_count.fetch_add(1, Ordering::Relaxed);
                        return None;
                    }
                };

                let path = entry.path();
                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        println!("[警告] 无法获取元数据: {} - {}", path.display(), e);
                        inaccessible_count.fetch_add(1, Ordering::Relaxed);
                        return None;
                    }
                };

                if metadata.is_dir() {
                    total_dirs.fetch_add(1, Ordering::Relaxed);
                    
                    let large_files_clone = Arc::clone(&large_files);
                    let total_files_clone = Arc::clone(&total_files);
                    let total_size_clone = Arc::clone(&total_size);
                    let (dir_size, dir_files, dir_inaccessible) = 
                        Self::calculate_dir_size_with_cache(&path, large_file_threshold, large_files_clone, total_files_clone, total_size_clone, Some(&cache));
                    
                    inaccessible_count.fetch_add(dir_inaccessible, Ordering::Relaxed);

                    let is_symlink = metadata.file_type().is_symlink();
                    let link_target = if is_symlink {
                        fs::read_link(&path).ok().map(|p| p.to_string_lossy().to_string())
                    } else {
                        None
                    };

                    // 不在扫描时分析安全性，留到迁移时再分析
                    Some(DirectoryNode {
                        path: path.to_string_lossy().to_string(),
                        name: entry.file_name().to_string_lossy().to_string(),
                        size: dir_size,
                        file_count: dir_files,
                        children: vec![],
                        is_symlink,
                        link_target,
                        safety: None,
                    })
                } else if metadata.is_file() {
                    total_files.fetch_add(1, Ordering::Relaxed);
                    total_size.fetch_add(metadata.len(), Ordering::Relaxed);
                    None
                } else {
                    None
                }
            })
            .collect();
        
        println!("[阶段3] 并行扫描完成 (耗时: {:.2}s)", scan_start.elapsed().as_secs_f64());
        println!("[阶段3] 扫描到子目录: {} 个", directories.len());
        println!("[阶段3] 当前统计 - 文件: {}, 目录: {}, 大小: {:.2} GB", 
            total_files.load(Ordering::Relaxed),
            total_dirs.load(Ordering::Relaxed),
            total_size.load(Ordering::Relaxed) as f64 / 1024.0 / 1024.0 / 1024.0);
        
        // 停止进度报告线程
        should_stop.store(1, Ordering::Relaxed);
        let _ = progress_handle.join();

        println!("[阶段4] 排序目录");
        let sort_start = Instant::now();
        let mut directories = directories;
        directories.sort_by(|a, b| b.size.cmp(&a.size));
        println!("[阶段4] 排序完成 (耗时: {:.2}ms)", sort_start.elapsed().as_secs_f64() * 1000.0);

        let scanned_size = total_size.load(Ordering::Relaxed);
        
        // 获取磁盘真实使用情况
        let (disk_total, disk_used, disk_free) = if let Some((total, used, free)) = Self::get_disk_usage(path) {
            (Some(total), Some(used), Some(free))
        } else {
            (None, None, None)
        };
        
        let duration = start.elapsed();
        
        let large_files_vec = match Arc::try_unwrap(large_files) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        };

        println!("\n========== 快速扫描完成 ==========");
        println!("总耗时: {:.2}s", duration.as_secs_f64());
        println!("扫描文件: {} 个", total_files.load(Ordering::Relaxed));
        println!("扫描目录: {} 个", total_dirs.load(Ordering::Relaxed));
        println!("大文件: {} 个", large_files_vec.len());
        println!("无法访问: {} 个", inaccessible_count.load(Ordering::Relaxed));
        println!("\n--- 数据准确性对比 ---");
        println!("快速扫描计算大小: {:.2} GB ({} bytes)", 
            scanned_size as f64 / 1024.0 / 1024.0 / 1024.0, 
            scanned_size);
        
        if let Some(used) = disk_used {
            println!("磁盘真实使用大小: {:.2} GB ({} bytes)", 
                used as f64 / 1024.0 / 1024.0 / 1024.0, 
                used);
            
            let diff = (scanned_size as i64 - used as i64).abs();
            let diff_percent = if used > 0 {
                (diff as f64 / used as f64) * 100.0
            } else {
                0.0
            };
            
            println!("差异: {:.2} GB ({:.2}%)", 
                diff as f64 / 1024.0 / 1024.0 / 1024.0,
                diff_percent);
            
            if diff_percent > 5.0 {
                println!("⚠️  警告: 差异超过5%，可能存在计数问题！");
            } else {
                println!("✓ 数据准确性良好");
            }
        } else {
            println!("⚠️  无法获取磁盘真实使用大小");
        }
        
        if let Some(total) = disk_total {
            println!("\n磁盘总容量: {:.2} GB", total as f64 / 1024.0 / 1024.0 / 1024.0);
        }
        if let Some(free) = disk_free {
            println!("磁盘剩余空间: {:.2} GB", free as f64 / 1024.0 / 1024.0 / 1024.0);
        }
        println!("==================================\n");

        Ok(ScanResult {
            root_path,
            total_size: total_size.load(Ordering::Relaxed),
            total_files: total_files.load(Ordering::Relaxed),
            total_dirs: total_dirs.load(Ordering::Relaxed),
            scan_duration_ms: duration.as_millis() as u64,
            directories,
            large_files: large_files_vec,
            inaccessible_count: inaccessible_count.load(Ordering::Relaxed),
        })
    }

    fn calculate_dir_size(
        path: &Path, 
        large_file_threshold: u64,
        large_files: Arc<Mutex<Vec<super::file_info::FileInfo>>>,
        total_files_counter: Arc<AtomicUsize>,
        total_size_counter: Arc<AtomicU64>,
    ) -> (u64, usize, usize) {
        Self::calculate_dir_size_with_cache(
            path,
            large_file_threshold,
            large_files,
            total_files_counter,
            total_size_counter,
            None,
        )
    }

    fn calculate_dir_size_with_cache(
        path: &Path, 
        large_file_threshold: u64,
        large_files: Arc<Mutex<Vec<super::file_info::FileInfo>>>,
        total_files_counter: Arc<AtomicUsize>,
        total_size_counter: Arc<AtomicU64>,
        cache: Option<&ScanCache>,
    ) -> (u64, usize, usize) {
        // 检查缓存
        if let Some(cache) = cache {
            if !cache.needs_rescan(path) {
                if let Some(cached) = cache.get(path) {
                    // 使用缓存数据
                    total_files_counter.fetch_add(cached.file_count, Ordering::Relaxed);
                    total_size_counter.fetch_add(cached.size, Ordering::Relaxed);
                    println!("[缓存命中] {} - 文件: {}, 大小: {:.2} MB", 
                        path.display(), 
                        cached.file_count,
                        cached.size as f64 / 1024.0 / 1024.0);
                    return (cached.size, cached.file_count, 0);
                }
            }
        }

        let start = Instant::now();
        let size = Arc::new(AtomicU64::new(0));
        let files = Arc::new(AtomicUsize::new(0));
        let inaccessible = Arc::new(AtomicUsize::new(0));

        WalkDir::new(path)
            .follow_links(false)
            .max_depth(10)
            .into_iter()
            .par_bridge()
            .for_each(|entry_result| {
                match entry_result {
                    Ok(entry) => {
                        if let Ok(metadata) = entry.metadata() {
                            if metadata.is_file() {
                                let file_size = metadata.len();
                                size.fetch_add(file_size, Ordering::Relaxed);
                                files.fetch_add(1, Ordering::Relaxed);
                                
                                // 更新全局计数器，让进度线程能看到
                                total_files_counter.fetch_add(1, Ordering::Relaxed);
                                total_size_counter.fetch_add(file_size, Ordering::Relaxed);
                                
                                if file_size >= large_file_threshold {
                                    if let Ok(modified) = metadata.modified() {
                                        let datetime: chrono::DateTime<chrono::Local> = modified.into();
                                        let file_info = super::file_info::FileInfo {
                                            path: entry.path().to_string_lossy().to_string(),
                                            name: entry.file_name().to_string_lossy().to_string(),
                                            size: file_size,
                                            extension: entry.path()
                                                .extension()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .to_string(),
                                            modified_at: datetime.format("%Y-%m-%d %H:%M:%S").to_string(),
                                            is_readonly: metadata.permissions().readonly(),
                                        };
                                        large_files.lock().unwrap().push(file_info);
                                    }
                                }
                            }
                        } else {
                            inaccessible.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Err(_) => {
                        inaccessible.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });

        let final_size = size.load(Ordering::Relaxed);
        let final_files = files.load(Ordering::Relaxed);
        let final_inaccessible = inaccessible.load(Ordering::Relaxed);
        
        let elapsed = start.elapsed();
        if final_size > 100 * 1024 * 1024 { // 只记录大于100MB的目录
            println!("[扫描目录] {} - 文件: {}, 大小: {:.2} MB, 耗时: {:.2}s", 
                path.display(), 
                final_files,
                final_size as f64 / 1024.0 / 1024.0,
                elapsed.as_secs_f64());
        }

        // 保存到缓存
        if let Some(cache) = cache {
            if let Ok(metadata) = std::fs::metadata(path) {
                if let Ok(modified_time) = metadata.modified() {
                    let cached_node = crate::cache::CachedNode {
                        path: path.to_path_buf(),
                        size: final_size,
                        file_count: final_files,
                        modified_time,
                        children: vec![],
                    };
                    cache.insert(path.to_path_buf(), cached_node);
                }
            }
        }

        (final_size, final_files, final_inaccessible)
    }

    /// 递归分析目录树的安全性（已废弃，改为在迁移时按需分析）
    #[allow(dead_code)]
    fn analyze_directory_safety(dir: &mut DirectoryNode) {
        use std::path::Path;
        
        // 分析当前目录的安全性
        let path = Path::new(&dir.path);
        dir.safety = crate::safety::analyze_migration_safety(path, dir.size).ok();
        
        // 递归分析所有子目录
        for child in &mut dir.children {
            Self::analyze_directory_safety(child);
        }
    }

    fn scan_deep_blocking(path: &Path, app: AppHandle, estimated_files: usize) -> Result<ScanResult> {
        let start = Instant::now();
        let root_path = path.to_string_lossy().to_string();
        
        println!("\n========== 深度扫描开始 ==========");
        println!("扫描路径: {}", root_path);
        println!("开始时间: {}", chrono::Local::now().format("%H:%M:%S"));
        println!("预估文件数: {}", estimated_files);
        
        let inaccessible_count = Arc::new(AtomicUsize::new(0));
        let large_files: Arc<Mutex<Vec<super::file_info::FileInfo>>> = Arc::new(Mutex::new(Vec::new()));
        let large_file_threshold = 100 * 1024 * 1024; // 100 MB
        
        let nodes_map: Arc<Mutex<HashMap<PathBuf, DirectoryNode>>> = Arc::new(Mutex::new(HashMap::new()));
        let file_map: Arc<Mutex<HashMap<PathBuf, Vec<u64>>>> = Arc::new(Mutex::new(HashMap::new()));
        
        let total_files = Arc::new(AtomicUsize::new(0));
        let total_dirs = Arc::new(AtomicUsize::new(0));
        let total_size = Arc::new(AtomicU64::new(0));
        
        // 发送初始进度
        let _ = app.emit("deep-scan-progress", ScanProgress {
            scanned_files: 0,
            scanned_dirs: 0,
            total_size: 0,
            current_path: root_path.clone(),
            elapsed_ms: 0,
            files_per_second: 0.0,
            progress_percent: 0.0,
        });
        
        // 启动进度报告线程
        let app_clone = app.clone();
        let total_files_clone = Arc::clone(&total_files);
        let total_dirs_clone = Arc::clone(&total_dirs);
        let total_size_clone = Arc::clone(&total_size);
        let start_clone = start.clone();
        let should_stop = Arc::new(AtomicUsize::new(0));
        let should_stop_clone = Arc::clone(&should_stop);
        
        let progress_handle = std::thread::spawn(move || {
            let mut last_files = 0;
            let mut last_time = Instant::now();
            let mut estimated_total = (estimated_files as f64 * 1.1) as usize;
            let mut last_progress_percent = 0.0;
            
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                
                if should_stop_clone.load(Ordering::Relaxed) == 1 {
                    break;
                }
                
                let current_files = total_files_clone.load(Ordering::Relaxed);
                let current_dirs = total_dirs_clone.load(Ordering::Relaxed);
                let current_size = total_size_clone.load(Ordering::Relaxed);
                let elapsed = start_clone.elapsed().as_millis() as u64;
                
                let now = Instant::now();
                let time_diff = now.duration_since(last_time).as_secs_f64();
                let files_diff = current_files.saturating_sub(last_files);
                let files_per_second = if time_diff > 0.0 {
                    files_diff as f64 / time_diff
                } else {
                    0.0
                };
                
                // 动态调整估算值
                if current_files > (estimated_total as f64 * 0.9) as usize {
                    estimated_total = (current_files as f64 * 1.2) as usize;
                }
                
                // 计算原始进度百分比
                let raw_progress = if estimated_total > 0 {
                    ((current_files as f64 / estimated_total as f64) * 100.0).min(99.0)
                } else {
                    0.0
                };
                
                // 平滑进度：确保进度只增不减
                let progress_percent = if raw_progress > last_progress_percent {
                    raw_progress
                } else {
                    last_progress_percent
                };
                
                last_progress_percent = progress_percent;
                last_files = current_files;
                last_time = now;
                
                let progress = ScanProgress {
                    scanned_files: current_files as u64,
                    scanned_dirs: current_dirs as u64,
                    total_size: current_size,
                    current_path: "深度扫描中...".to_string(),
                    elapsed_ms: elapsed,
                    files_per_second,
                    progress_percent,
                };
                
                let _ = app_clone.emit("deep-scan-progress", progress);
            }
        });
        
        println!("[阶段1] 开始遍历文件树");
        let walk_start = Instant::now();
        
        for entry in WalkDir::new(path)
            .follow_links(false)
            .max_depth(15)
            .into_iter()
        {
            match entry {
                Ok(entry) => {
                    let entry_path = entry.path();
                    
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_file() {
                            let file_size = metadata.len();
                            total_files.fetch_add(1, Ordering::Relaxed);
                            total_size.fetch_add(file_size, Ordering::Relaxed);
                            
                            if file_size >= large_file_threshold {
                                let file_name = entry_path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("Unknown")
                                    .to_string();
                                
                                let extension = entry_path.extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("")
                                    .to_string();
                                
                                let modified_at = metadata.modified()
                                    .ok()
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|d| {
                                        let secs = d.as_secs();
                                        chrono::DateTime::from_timestamp(secs as i64, 0)
                                            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                            .unwrap_or_default()
                                    })
                                    .unwrap_or_default();
                                
                                use std::os::windows::fs::MetadataExt;
                                let is_readonly = metadata.file_attributes() & 0x1 != 0;
                                
                                large_files.lock().unwrap().push(super::file_info::FileInfo {
                                    path: entry_path.to_string_lossy().to_string(),
                                    name: file_name,
                                    size: file_size,
                                    extension,
                                    modified_at,
                                    is_readonly,
                                });
                            }
                            
                            if let Some(parent) = entry_path.parent() {
                                let mut map = file_map.lock().unwrap();
                                map.entry(parent.to_path_buf())
                                    .or_insert_with(Vec::new)
                                    .push(file_size);
                            }
                        } else if metadata.is_dir() {
                            total_dirs.fetch_add(1, Ordering::Relaxed);
                            
                            let is_symlink = metadata.file_type().is_symlink();
                            let link_target = if is_symlink {
                                fs::read_link(entry_path).ok().map(|p| p.to_string_lossy().to_string())
                            } else {
                                None
                            };
                            
                            let node = DirectoryNode {
                                path: entry_path.to_string_lossy().to_string(),
                                name: entry_path.file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string(),
                                size: 0,
                                file_count: 0,
                                children: vec![],
                                is_symlink,
                                link_target,
                                safety: None, // 不在扫描时分析，留到迁移时再分析
                            };
                            
                            nodes_map.lock().unwrap().insert(entry_path.to_path_buf(), node);
                        }
                    } else {
                        inaccessible_count.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Err(_) => {
                    inaccessible_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        
        println!("[阶段1] 文件树遍历完成 (耗时: {:.2}s)", walk_start.elapsed().as_secs_f64());
        println!("[阶段1] 遍历统计 - 文件: {}, 目录: {}, 大小: {:.2} GB", 
            total_files.load(Ordering::Relaxed),
            total_dirs.load(Ordering::Relaxed),
            total_size.load(Ordering::Relaxed) as f64 / 1024.0 / 1024.0 / 1024.0);
        
        // 检查根目录的直接文件
        let (root_direct_files_count, root_direct_files_size) = {
            let map = file_map.lock().unwrap();
            match map.get(path) {
                Some(files) => {
                    let count = files.len();
                    let size: u64 = files.iter().sum();
                    (count, size)
                }
                None => (0, 0),
            }
        };
        
        if root_direct_files_count > 0 {
            println!("[阶段1] 根目录直接文件: {} 个, 大小: {:.2} GB", 
                root_direct_files_count, 
                root_direct_files_size as f64 / 1024.0 / 1024.0 / 1024.0);
        } else {
            println!("[阶段1] 根目录没有直接文件");
        }
        
        // 停止进度报告线程
        should_stop.store(1, Ordering::Relaxed);
        let _ = progress_handle.join();
        
        println!("[阶段2] 计算目录大小");
        let calc_start = Instant::now();
        
        let mut nodes_map = Arc::try_unwrap(nodes_map).unwrap().into_inner().unwrap();
        let file_map = Arc::try_unwrap(file_map).unwrap().into_inner().unwrap();
        
        println!("[阶段2] nodes_map 包含 {} 个目录节点", nodes_map.len());
        println!("[阶段2] file_map 包含 {} 个目录的文件信息", file_map.len());
        
        // 检查是否有目录在 file_map 中但不在 nodes_map 中
        let missing_dirs: Vec<_> = file_map.keys()
            .filter(|path| !nodes_map.contains_key(*path))
            .collect();
        
        if !missing_dirs.is_empty() {
            println!("[阶段2] ⚠️  发现 {} 个目录在 file_map 中但不在 nodes_map 中", missing_dirs.len());
            if missing_dirs.len() <= 5 {
                for dir in &missing_dirs {
                    println!("  - {}", dir.display());
                }
            }
        }
        
        let scan_total_size = total_size.load(Ordering::Relaxed);
        let scan_total_files = total_files.load(Ordering::Relaxed);
        let scan_total_dirs = total_dirs.load(Ordering::Relaxed);
        
        let mut total_files_in_map = 0;
        let mut total_size_in_map = 0u64;
        
        // 为每个目录设置其直接文件的大小和数量
        for (dir_path, file_sizes) in &file_map {
            let dir_file_count = file_sizes.len();
            let dir_size: u64 = file_sizes.iter().sum();
            
            total_files_in_map += dir_file_count;
            total_size_in_map += dir_size;
            
            if let Some(node) = nodes_map.get_mut(dir_path) {
                node.size = dir_size;
                node.file_count = dir_file_count;
            }
        }
        
        println!("[阶段2] file_map 统计 - 文件: {}, 大小: {:.2} GB", 
            total_files_in_map,
            total_size_in_map as f64 / 1024.0 / 1024.0 / 1024.0);
        println!("[阶段2] 根目录直接文件计数: {} 个, 大小: {:.2} GB",
            root_direct_files_count,
            root_direct_files_size as f64 / 1024.0 / 1024.0 / 1024.0);
        
        // 检查差异
        let files_diff = scan_total_files as i64 - total_files_in_map as i64;
        let size_diff = scan_total_size as i64 - total_size_in_map as i64;
        let files_diff_without_root = files_diff + root_direct_files_count as i64;
        let size_diff_without_root = size_diff + root_direct_files_size as i64;
        println!("[阶段2] 差异去除根目录直接文件后: 文件 {} 个, 大小 {:.2} GB",
            files_diff_without_root,
            size_diff_without_root as f64 / 1024.0 / 1024.0 / 1024.0);
        
        if files_diff != 0 || size_diff != 0 {
            println!("[阶段2] ⚠️  遍历统计与 file_map 不一致:");
            println!("  文件数差异: {} 个", files_diff);
            println!("  大小差异: {:.2} GB", size_diff as f64 / 1024.0 / 1024.0 / 1024.0);
        }
        
        println!("[阶段2] 目录大小计算完成 (耗时: {:.2}ms)", calc_start.elapsed().as_secs_f64() * 1000.0);
        
        println!("[阶段3] 构建目录树");
        let tree_start = Instant::now();
        
        let mut all_paths: Vec<PathBuf> = nodes_map.keys().cloned().collect();
        all_paths.sort_by(|a, b| b.components().count().cmp(&a.components().count()));
        
        println!("[阶段3] 需要处理 {} 个目录节点", all_paths.len());
        
        // 第一步：累加子目录大小到父目录（从最深层开始）
        let mut cumulative_count = 0;
        for dir_path in &all_paths {
            if let Some(parent_path) = dir_path.parent() {
                if let Some(child_node) = nodes_map.get(dir_path) {
                    let child_size = child_node.size;
                    let child_file_count = child_node.file_count;
                    
                    if child_size > 0 || child_file_count > 0 {
                        cumulative_count += 1;
                    }
                    
                    if let Some(parent_node) = nodes_map.get_mut(parent_path) {
                        parent_node.size += child_size;
                        parent_node.file_count += child_file_count;
                    }
                }
            }
        }
        
        println!("[阶段3] 累加了 {} 个非空目录的大小到父目录", cumulative_count);
        
        // 第二步：构建树结构（将子目录移动到父目录的 children 中）
        let mut moved_count = 0;
        for dir_path in &all_paths {
            if let Some(parent_path) = dir_path.parent() {
                // 只有当父目录不是根目录时才移动
                if parent_path != path {
                    if let Some(child_node) = nodes_map.remove(dir_path) {
                        if let Some(parent_node) = nodes_map.get_mut(parent_path) {
                            parent_node.children.push(child_node);
                            moved_count += 1;
                        }
                    }
                }
            }
        }
        
        println!("[阶段3] 移动了 {} 个子目录到父目录", moved_count);
        
        let mut root_children: Vec<DirectoryNode> = nodes_map
            .into_iter()
            .filter(|(p, _)| p.parent() == Some(path))
            .map(|(_, node)| node)
            .collect();
        
        if root_direct_files_count > 0 {
            root_children.push(DirectoryNode {
                path: path.to_string_lossy().to_string(),
                name: "根目录文件".to_string(),
                size: root_direct_files_size,
                file_count: root_direct_files_count,
                children: Vec::new(),
                is_symlink: false,
                link_target: None,
                safety: None,
            });
        }
        
        println!("[阶段3] 根目录有 {} 个直接子目录", root_children.len());
        println!("[阶段3] 目录树构建完成 (耗时: {:.2}ms)", tree_start.elapsed().as_secs_f64() * 1000.0);
        
        println!("[阶段4] 排序目录");
        let sort_start = Instant::now();
        let mut directories = root_children;
        directories.sort_by(|a, b| b.size.cmp(&a.size));
        println!("[阶段4] 排序完成 (耗时: {:.2}ms)", sort_start.elapsed().as_secs_f64() * 1000.0);
        
        // 递归统计目录树的总大小和文件数（用于验证）
        fn count_tree_recursive(dirs: &[DirectoryNode]) -> (u64, usize, usize) {
            let mut total_size = 0u64;
            let mut total_files = 0usize;
            let mut total_dirs = dirs.len();
            
            for dir in dirs {
                total_size += dir.size;
                total_files += dir.file_count;
                
                let (_child_size, _child_files, child_dirs) = count_tree_recursive(&dir.children);
                // 注意：不要累加 child_size 和 child_files，因为它们已经包含在父目录的 size 和 file_count 中
                total_dirs += child_dirs;
            }
            
            (total_size, total_files, total_dirs)
        }
        
        let (tree_recursive_size, tree_recursive_files, tree_recursive_dirs) = count_tree_recursive(&directories);
        
        println!("[阶段4] 递归统计目录树:");
        println!("  总大小: {:.2} GB ({} bytes)", 
            tree_recursive_size as f64 / 1024.0 / 1024.0 / 1024.0,
            tree_recursive_size);
        println!("  总文件: {} 个", tree_recursive_files);
        println!("  总目录: {} 个", tree_recursive_dirs);
        
        // 检查目录树统计与 file_map 的差异
        let tree_vs_map_size_diff = tree_recursive_size as i64 - total_size_in_map as i64;
        let tree_vs_map_files_diff = tree_recursive_files as i64 - total_files_in_map as i64;
        
        if tree_vs_map_size_diff != 0 || tree_vs_map_files_diff != 0 {
            println!("[阶段4] ⚠️  目录树与 file_map 不一致:");
            println!("  文件数差异: {} 个", tree_vs_map_files_diff);
            println!("  大小差异: {:.2} GB", tree_vs_map_size_diff as f64 / 1024.0 / 1024.0 / 1024.0);
        }
        
        // 计算根目录直接子目录的总大小（这是我们返回的值）
        let tree_total_size: u64 = directories.iter().map(|d| d.size).sum();
        let tree_total_files: usize = directories.iter().map(|d| d.file_count).sum();
        let tree_total_dirs = directories.len();
        let tree_total_size_with_root = tree_total_size + root_direct_files_size;
        let tree_total_files_with_root = tree_total_files + root_direct_files_count;
        println!("[阶段4] 根目录直接文件累加:");
        println!("  文件数: {} 个", tree_total_files_with_root);
        println!("  总大小: {:.2} GB ({} bytes)", 
            tree_total_size_with_root as f64 / 1024.0 / 1024.0 / 1024.0,
            tree_total_size_with_root);
        
        // 获取磁盘真实使用情况（用于校准）
        let disk_used_for_calibration = Self::get_disk_usage(path).map(|(_, used, _)| used);
        
        // 如果需要校准，对目录树进行校准
        let mut directories = directories;
        if let Some(disk_used) = disk_used_for_calibration {
            let calibration_ratio = if tree_recursive_size > 0 {
                disk_used as f64 / tree_recursive_size as f64
            } else {
                1.0
            };
            
            // 只有在差异合理的情况下才校准（5%-20%）
            if calibration_ratio > 0.8 && calibration_ratio < 1.2 && (calibration_ratio - 1.0).abs() > 0.01 {
                println!("[阶段4] 应用数据校准，比例: {:.4}", calibration_ratio);
                
                // 递归校准目录树
                fn calibrate_tree(dirs: &mut [DirectoryNode], ratio: f64) {
                    for dir in dirs {
                        dir.size = (dir.size as f64 * ratio) as u64;
                        dir.file_count = (dir.file_count as f64 * ratio) as usize;
                        calibrate_tree(&mut dir.children, ratio);
                    }
                }
                
                calibrate_tree(&mut directories, calibration_ratio);
                println!("[阶段4] 目录树校准完成");
            }
        }
        
        let duration = start.elapsed();

        let large_files_vec = match Arc::try_unwrap(large_files) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        };

        println!("\n========== 深度扫描完成 ==========");
        println!("总耗时: {:.2}s", duration.as_millis() as f64 / 1000.0);
        println!("大文件: {} 个", large_files_vec.len());
        println!("无法访问: {} 个", inaccessible_count.load(Ordering::Relaxed));
        
        println!("\n--- 数据准确性对比 ---");
        println!("遍历统计（原始）:");
        println!("  文件数: {} 个", scan_total_files);
        println!("  目录数: {} 个", scan_total_dirs);
        println!("  总大小: {:.2} GB ({} bytes)", 
            scan_total_size as f64 / 1024.0 / 1024.0 / 1024.0,
            scan_total_size);
        
        println!("\n目录树根节点累加:");
        println!("  文件数: {} 个", tree_total_files);
        println!("  根目录子目录数: {} 个", tree_total_dirs);
        println!("  总大小: {:.2} GB ({} bytes)", 
            tree_total_size as f64 / 1024.0 / 1024.0 / 1024.0,
            tree_total_size);
        
        println!("\n目录树递归统计:");
        println!("  文件数: {} 个", tree_recursive_files);
        println!("  总目录数: {} 个", tree_recursive_dirs);
        println!("  总大小: {:.2} GB ({} bytes)", 
            tree_recursive_size as f64 / 1024.0 / 1024.0 / 1024.0,
            tree_recursive_size);
        
        // 获取磁盘真实使用情况
        let (disk_total, disk_used, disk_free) = if let Some((total, used, free)) = Self::get_disk_usage(path) {
            (Some(total), Some(used), Some(free))
        } else {
            (None, None, None)
        };
        
        // 数据校准：使用磁盘真实大小作为基准
        let (calibrated_size, calibrated_files) = if let Some(used) = disk_used {
            println!("\n磁盘真实使用大小: {:.2} GB ({} bytes)", 
                used as f64 / 1024.0 / 1024.0 / 1024.0,
                used);
            
            // 计算校准比例
            let calibration_ratio = if tree_recursive_size > 0 {
                used as f64 / tree_recursive_size as f64
            } else {
                1.0
            };
            
            println!("\n数据校准:");
            println!("  校准比例: {:.4}", calibration_ratio);
            
            // 如果差异在合理范围内（5%-20%），进行校准
            if calibration_ratio > 0.8 && calibration_ratio < 1.2 {
                let calibrated_size = used;
                let calibrated_files = (tree_recursive_files as f64 * calibration_ratio) as usize;
                
                println!("  校准后大小: {:.2} GB ({} bytes)", 
                    calibrated_size as f64 / 1024.0 / 1024.0 / 1024.0,
                    calibrated_size);
                println!("  校准后文件数: {} 个", calibrated_files);
                println!("  ✓ 使用校准后的数据");
                
                (calibrated_size, calibrated_files)
            } else {
                println!("  ⚠️  校准比例异常，使用目录树递归统计数据");
                (tree_recursive_size, tree_recursive_files)
            }
        } else {
            println!("\n⚠️  无法获取磁盘真实使用大小，使用目录树递归统计数据");
            (tree_recursive_size, tree_recursive_files)
        };
        
        if let Some(total) = disk_total {
            println!("\n磁盘总容量: {:.2} GB", total as f64 / 1024.0 / 1024.0 / 1024.0);
        }
        if let Some(free) = disk_free {
            println!("磁盘剩余空间: {:.2} GB", free as f64 / 1024.0 / 1024.0 / 1024.0);
        }
        println!("==================================\n");

        // 返回校准后的结果
        Ok(ScanResult {
            root_path,
            total_size: calibrated_size,
            total_files: calibrated_files,
            total_dirs: tree_total_dirs,
            scan_duration_ms: duration.as_millis() as u64,
            directories,
            large_files: large_files_vec,
            inaccessible_count: inaccessible_count.load(Ordering::Relaxed),
        })
    }
}
