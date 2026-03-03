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
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, GetDiskFreeSpaceExW};
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

pub struct DiskScanner;

impl DiskScanner {
    pub fn new() -> Self {
        Self
    }

    #[cfg(windows)]
    fn get_file_id(path: &Path) -> Option<(u32, u64)> {
        use std::fs::File;
        
        let file = File::open(path).ok()?;
        let handle = HANDLE(file.as_raw_handle() as isize);
        
        let mut file_info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe {
            if GetFileInformationByHandle(handle, &mut file_info).is_ok() {
                let file_index = ((file_info.nFileIndexHigh as u64) << 32) | (file_info.nFileIndexLow as u64);
                Some((file_info.dwVolumeSerialNumber, file_index))
            } else {
                None
            }
        }
    }

    #[cfg(not(windows))]
    fn get_file_id(_path: &Path) -> Option<(u32, u64)> {
        None
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
        
        tokio::task::spawn_blocking(move || {
            Self::scan_quick(&path, app)
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

    fn scan_quick(path: &Path, app: AppHandle) -> Result<ScanResult> {
        let start = Instant::now();
        let root_path = path.to_string_lossy().to_string();
        
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

        let entries: Vec<_> = match fs::read_dir(path) {
            Ok(entries) => entries.collect(),
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to read directory: {}", e));
            }
        };

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
                        Self::calculate_dir_size(&path, large_file_threshold, large_files_clone, total_files_clone, total_size_clone);
                    
                    inaccessible_count.fetch_add(dir_inaccessible, Ordering::Relaxed);

                    let is_symlink = metadata.file_type().is_symlink();
                    let link_target = if is_symlink {
                        fs::read_link(&path).ok().map(|p| p.to_string_lossy().to_string())
                    } else {
                        None
                    };

                    Some(DirectoryNode {
                        path: path.to_string_lossy().to_string(),
                        name: entry.file_name().to_string_lossy().to_string(),
                        size: dir_size,
                        file_count: dir_files,
                        children: vec![],
                        is_symlink,
                        link_target,
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
        
        // 停止进度报告线程
        should_stop.store(1, Ordering::Relaxed);
        let _ = progress_handle.join();

        let mut directories = directories;
        directories.sort_by(|a, b| b.size.cmp(&a.size));

        let scanned_size = total_size.load(Ordering::Relaxed);
        
        if let Some((total, used, free)) = Self::get_disk_usage(path) {
            // 磁盘使用情况已获取，但不输出日志
            let _ = (total, used, free);
        }
        
        let duration = start.elapsed();
        
        let large_files_vec = match Arc::try_unwrap(large_files) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        };

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

        (final_size, final_files, final_inaccessible)
    }

    fn scan_deep_blocking(path: &Path, app: AppHandle, estimated_files: usize) -> Result<ScanResult> {
        let start = Instant::now();
        let root_path = path.to_string_lossy().to_string();
        
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
        
        // 停止进度报告线程
        should_stop.store(1, Ordering::Relaxed);
        let _ = progress_handle.join();
        
        let mut nodes_map = Arc::try_unwrap(nodes_map).unwrap().into_inner().unwrap();
        let file_map = Arc::try_unwrap(file_map).unwrap().into_inner().unwrap();
        
        for (dir_path, file_sizes) in &file_map {
            if let Some(node) = nodes_map.get_mut(dir_path) {
                node.size = file_sizes.iter().sum();
                node.file_count = file_sizes.len();
            }
        }
        
        let mut all_paths: Vec<PathBuf> = nodes_map.keys().cloned().collect();
        all_paths.sort_by(|a, b| b.components().count().cmp(&a.components().count()));
        
        for dir_path in &all_paths {
            if let Some(parent_path) = dir_path.parent() {
                if let Some(child_node) = nodes_map.get(dir_path) {
                    let child_size = child_node.size;
                    let child_file_count = child_node.file_count;
                    
                    if let Some(parent_node) = nodes_map.get_mut(parent_path) {
                        parent_node.size += child_size;
                        parent_node.file_count += child_file_count;
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
        
        let root_children: Vec<DirectoryNode> = nodes_map
            .into_iter()
            .filter(|(p, _)| p.parent() == Some(path))
            .map(|(_, node)| node)
            .collect();
        
        let mut directories = root_children;
        directories.sort_by(|a, b| b.size.cmp(&a.size));
        
        let total_size: u64 = directories.iter().map(|d| d.size).sum();
        let total_files: usize = directories.iter().map(|d| d.file_count).sum();
        let total_dirs = directories.len();
        
        let duration = start.elapsed();

        let large_files_vec = match Arc::try_unwrap(large_files) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        };

        Ok(ScanResult {
            root_path,
            total_size,
            total_files,
            total_dirs,
            scan_duration_ms: duration.as_millis() as u64,
            directories,
            large_files: large_files_vec,
            inaccessible_count: inaccessible_count.load(Ordering::Relaxed),
        })
    }
}
