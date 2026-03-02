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

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, GetDiskFreeSpaceExW};
#[cfg(windows)]
use windows::core::PCWSTR;

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

    pub async fn scan<P: AsRef<Path>>(&self, path: P) -> Result<ScanResult> {
        let path = path.as_ref().to_path_buf();
        
        tokio::task::spawn_blocking(move || {
            Self::scan_quick(&path)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    pub async fn scan_deep<P: AsRef<Path>>(&self, path: P) -> Result<ScanResult> {
        let path = path.as_ref().to_path_buf();
        
        tokio::task::spawn_blocking(move || {
            Self::scan_deep_blocking(&path)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    fn scan_quick(path: &Path) -> Result<ScanResult> {
        let start = Instant::now();
        let root_path = path.to_string_lossy().to_string();
        
        println!("\n=== 快速扫描开始 ===");
        println!("路径: {}", root_path);
        
        let total_size = Arc::new(AtomicU64::new(0));
        let total_files = Arc::new(AtomicUsize::new(0));
        let total_dirs = Arc::new(AtomicUsize::new(0));
        let inaccessible_count = Arc::new(AtomicUsize::new(0));

        let step1 = Instant::now();
        let entries: Vec<_> = match fs::read_dir(path) {
            Ok(entries) => entries.collect(),
            Err(e) => return Err(anyhow::anyhow!("Failed to read directory: {}", e)),
        };
        println!("[性能] 读取根目录条目: {:.3} 秒", step1.elapsed().as_secs_f64());

        let step2 = Instant::now();
        let directories: Vec<DirectoryNode> = entries
            .par_iter()
            .filter_map(|entry_result| {
                let entry = match entry_result {
                    Ok(e) => e,
                    Err(_) => {
                        inaccessible_count.fetch_add(1, Ordering::Relaxed);
                        return None;
                    }
                };

                let path = entry.path();
                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => {
                        inaccessible_count.fetch_add(1, Ordering::Relaxed);
                        return None;
                    }
                };

                if metadata.is_dir() {
                    total_dirs.fetch_add(1, Ordering::Relaxed);
                    
                    let (dir_size, dir_files, dir_inaccessible) = 
                        Self::calculate_dir_size(&path);
                    
                    total_size.fetch_add(dir_size, Ordering::Relaxed);
                    total_files.fetch_add(dir_files, Ordering::Relaxed);
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
        println!("[性能] 并行扫描目录: {:.3} 秒", step2.elapsed().as_secs_f64());

        let step3 = Instant::now();
        let mut directories = directories;
        directories.sort_by(|a, b| b.size.cmp(&a.size));
        println!("[性能] 排序目录: {:.3} 秒", step3.elapsed().as_secs_f64());

        let step4 = Instant::now();
        let scanned_size = total_size.load(Ordering::Relaxed);
        
        if let Some((total, used, free)) = Self::get_disk_usage(path) {
            println!("=== 磁盘真实使用情况（Windows API）===");
            println!("磁盘总容量: {:.1} GB", total as f64 / 1024.0 / 1024.0 / 1024.0);
            println!("已使用空间: {:.1} GB", used as f64 / 1024.0 / 1024.0 / 1024.0);
            println!("可用空间: {:.1} GB", free as f64 / 1024.0 / 1024.0 / 1024.0);
            println!("扫描统计大小: {:.1} GB", scanned_size as f64 / 1024.0 / 1024.0 / 1024.0);
            println!("差异: {:.1} GB", (used as i64 - scanned_size as i64).abs() as f64 / 1024.0 / 1024.0 / 1024.0);
        } else {
            println!("总大小: {:.1} GB", scanned_size as f64 / 1024.0 / 1024.0 / 1024.0);
        }
        println!("[性能] 获取磁盘使用情况: {:.3} 秒", step4.elapsed().as_secs_f64());
        
        println!("文件数: {}", total_files.load(Ordering::Relaxed));
        println!("目录数: {}", total_dirs.load(Ordering::Relaxed));
        println!("返回的顶层目录数: {}", directories.len());
        
        let duration = start.elapsed();
        println!("扫描时间: {:.1} 秒", duration.as_secs_f64());
        println!("无法访问: {}", inaccessible_count.load(Ordering::Relaxed));
        
        for (i, dir) in directories.iter().take(5).enumerate() {
            println!("  {}. {} - {:.1} GB", i + 1, dir.name, dir.size as f64 / 1024.0 / 1024.0 / 1024.0);
        }
        
        println!("=== 快速扫描结束 ===\n");

        Ok(ScanResult {
            root_path,
            total_size: total_size.load(Ordering::Relaxed),
            total_files: total_files.load(Ordering::Relaxed),
            total_dirs: total_dirs.load(Ordering::Relaxed),
            scan_duration_ms: duration.as_millis() as u64,
            directories,
            inaccessible_count: inaccessible_count.load(Ordering::Relaxed),
        })
    }

    fn calculate_dir_size(path: &Path) -> (u64, usize, usize) {
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
                                size.fetch_add(metadata.len(), Ordering::Relaxed);
                                files.fetch_add(1, Ordering::Relaxed);
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
        
        let elapsed = start.elapsed().as_secs_f64();
        if elapsed > 1.0 {
            println!("[性能] calculate_dir_size({}) 耗时: {:.3} 秒, 文件数: {}", 
                path.file_name().unwrap_or_default().to_string_lossy(), 
                elapsed, 
                final_files
            );
        }

        (final_size, final_files, final_inaccessible)
    }

    fn scan_deep_blocking(path: &Path) -> Result<ScanResult> {
        let start = Instant::now();
        let root_path = path.to_string_lossy().to_string();
        
        println!("\n=== 深度扫描开始 ===");
        println!("路径: {}", root_path);
        
        let inaccessible_count = Arc::new(AtomicUsize::new(0));
        
        let nodes_map: Arc<Mutex<HashMap<PathBuf, DirectoryNode>>> = Arc::new(Mutex::new(HashMap::new()));
        let file_map: Arc<Mutex<HashMap<PathBuf, Vec<u64>>>> = Arc::new(Mutex::new(HashMap::new()));
        
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
                            
                            if let Some(parent) = entry_path.parent() {
                                let mut map = file_map.lock().unwrap();
                                map.entry(parent.to_path_buf())
                                    .or_insert_with(Vec::new)
                                    .push(file_size);
                            }
                        } else if metadata.is_dir() {
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
        
        if let Some((total, used, free)) = Self::get_disk_usage(path) {
            println!("=== 磁盘真实使用情况（Windows API）===");
            println!("磁盘总容量: {:.1} GB", total as f64 / 1024.0 / 1024.0 / 1024.0);
            println!("已使用空间: {:.1} GB", used as f64 / 1024.0 / 1024.0 / 1024.0);
            println!("可用空间: {:.1} GB", free as f64 / 1024.0 / 1024.0 / 1024.0);
            println!("扫描统计大小: {:.1} GB", total_size as f64 / 1024.0 / 1024.0 / 1024.0);
            println!("差异: {:.1} GB", (used as i64 - total_size as i64).abs() as f64 / 1024.0 / 1024.0 / 1024.0);
        } else {
            println!("总大小: {:.1} GB", total_size as f64 / 1024.0 / 1024.0 / 1024.0);
        }
        
        println!("文件数: {}", total_files);
        println!("目录数: {}", total_dirs);
        println!("返回的顶层目录数: {}", directories.len());
        println!("扫描时间: {:.1} 秒", duration.as_secs_f64());
        println!("无法访问: {}", inaccessible_count.load(Ordering::Relaxed));
        
        for (i, dir) in directories.iter().take(5).enumerate() {
            println!("  {}. {} - {:.1} GB ({} 个文件)", i + 1, dir.name, dir.size as f64 / 1024.0 / 1024.0 / 1024.0, dir.file_count);
        }
        
        println!("=== 深度扫描结束 ===\n");

        Ok(ScanResult {
            root_path,
            total_size,
            total_files,
            total_dirs,
            scan_duration_ms: duration.as_millis() as u64,
            directories,
            inaccessible_count: inaccessible_count.load(Ordering::Relaxed),
        })
    }
}
