use super::file_info::{DirectoryNode, FileInfo, ScanResult};
use super::scan_index::IndexedScanResult;
use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use chrono;

#[cfg(windows)]
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
#[cfg(windows)]
use windows::core::PCWSTR;

use tauri::{AppHandle, Emitter};

use crate::cache::ScanCache;
use crate::winfs;

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

pub struct DiskScanner {
    cache: ScanCache,
    cancelled: Arc<AtomicBool>,
    sessions: Arc<Mutex<HashMap<String, IndexedScanResult>>>,
}

impl DiskScanner {
    pub fn new() -> Self {
        Self {
            cache: ScanCache::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[allow(dead_code)]
    pub fn new_with_cache(cache: ScanCache) -> Self {
        Self {
            cache,
            cancelled: Arc::new(AtomicBool::new(false)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn reset_cancel(&self) {
        self.cancelled.store(false, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    #[allow(dead_code)]
    pub fn cache_size(&self) -> usize {
        self.cache.size()
    }

    pub fn store_indexed_scan_result(&self, result: &ScanResult) {
        let indexed = IndexedScanResult::from_scan_result(result);
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(result.root_path.clone(), indexed);
        }
    }

    pub fn get_directory_snapshot(&self, root_path: &str, path: &str) -> Option<ScanResult> {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(root_path).cloned())
            .and_then(|indexed| indexed.snapshot_for_path(path))
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
        let cancelled = Arc::clone(&self.cancelled);
        self.reset_cancel();

        tokio::task::spawn_blocking(move || {
            Self::scan_quick(&path, app, cache, cancelled)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    pub async fn scan_deep<P: AsRef<Path>>(&self, path: P, app: AppHandle, estimated_files: usize) -> Result<ScanResult> {
        let path = path.as_ref().to_path_buf();
        let cancelled = Arc::clone(&self.cancelled);
        self.reset_cancel();

        tokio::task::spawn_blocking(move || {
            Self::scan_deep_blocking(&path, app, estimated_files, cancelled)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    // ======================== 快速扫描 ========================

    fn scan_quick(path: &Path, app: AppHandle, cache: ScanCache, cancelled: Arc<AtomicBool>) -> Result<ScanResult> {
        let start = Instant::now();
        let root_path = path.to_string_lossy().to_string();

        println!("\n========== 快速扫描开始 ==========");
        println!("扫描路径: {}", root_path);
        let mut step_time = Instant::now();

        let total_size = Arc::new(AtomicU64::new(0));
        let total_files = Arc::new(AtomicUsize::new(0));
        let total_dirs = Arc::new(AtomicUsize::new(0));
        let large_file_threshold = 100 * 1024 * 1024u64;

        let _ = app.emit("quick-scan-progress", ScanProgress {
            scanned_files: 0, scanned_dirs: 0, total_size: 0,
            current_path: root_path.clone(), elapsed_ms: 0,
            files_per_second: 0.0, progress_percent: 0.0,
        });

        println!("[{:.3}s] 初始化完成", step_time.elapsed().as_secs_f64());
        step_time = Instant::now();

        let entries = winfs::enumerate_directory(path, false)
            .map_err(|e| anyhow::anyhow!("Failed to read directory: {}", e))?;
        println!("[{:.3}s] 原生枚举完成, {} 个条目", step_time.elapsed().as_secs_f64(), entries.len());
        step_time = Instant::now();

        // 从上次扫描缓存获取估算文件数，否则使用默认值
        let last_file_count = cache.last_total_files();
        let estimated_total = if last_file_count > 0 { last_file_count as f64 } else { 500000.0 };

        // 进度报告线程
        let app_clone = app.clone();
        let ts = Arc::clone(&total_size);
        let tf = Arc::clone(&total_files);
        let td = Arc::clone(&total_dirs);
        let start_c = start.clone();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_c = Arc::clone(&should_stop);
        let cancelled_c = Arc::clone(&cancelled);

        let progress_handle = std::thread::spawn(move || {
            let mut last_files = 0usize;
            let mut last_time = Instant::now();
            let mut last_pct = 0.0f64;

            loop {
                std::thread::sleep(std::time::Duration::from_millis(300));
                if should_stop_c.load(Ordering::Relaxed) || cancelled_c.load(Ordering::Relaxed) {
                    break;
                }
                let cur_files = tf.load(Ordering::Relaxed);
                let cur_dirs = td.load(Ordering::Relaxed);
                let cur_size = ts.load(Ordering::Relaxed);
                let elapsed = start_c.elapsed().as_millis() as u64;

                let now = Instant::now();
                let dt = now.duration_since(last_time).as_secs_f64();
                let fps = if dt > 0.0 { (cur_files - last_files) as f64 / dt } else { 0.0 };

                let raw_pct = (cur_files as f64 / estimated_total * 100.0).min(99.0);
                let pct = raw_pct.max(last_pct);
                last_pct = pct;
                last_files = cur_files;
                last_time = now;

                let _ = app_clone.emit("quick-scan-progress", ScanProgress {
                    scanned_files: cur_files as u64, scanned_dirs: cur_dirs as u64,
                    total_size: cur_size, current_path: "快速扫描中...".to_string(),
                    elapsed_ms: elapsed, files_per_second: fps, progress_percent: pct,
                });
            }
        });

        println!("[{:.3}s] 进度线程已启动", step_time.elapsed().as_secs_f64());
        step_time = Instant::now();

        let mut directories_map: HashMap<PathBuf, DirectoryNode> = entries
            .par_iter()
            .filter(|entry| entry.is_dir)
            .map(|entry| {
                (
                    entry.path.clone(),
                    DirectoryNode {
                        path: entry.path.to_string_lossy().to_string(),
                        name: entry.name.clone(),
                        size: 0,
                        file_count: 0,
                        dir_count: 1,
                        children: vec![],
                        has_children: false,
                        is_symlink: entry.is_symlink,
                        link_target: if entry.is_symlink {
                            Self::resolve_link_target(&entry.path)
                        } else {
                            None
                        },
                        safety: None,
                        modified_time: entry.modified_time,
                        file_id: None,
                    },
                )
            })
            .collect();

        println!("[{:.3}s] rayon 构建 directories_map 完成, {} 个目录", step_time.elapsed().as_secs_f64(), directories_map.len());
        step_time = Instant::now();

        let mut root_file_size = 0u64;
        let mut root_file_count = 0usize;
        let mut root_large_files: Vec<FileInfo> = Vec::new();
        for entry in &entries {
            if entry.is_symlink || entry.is_dir {
                continue;
            }

            root_file_size += entry.size;
            root_file_count += 1;
            if entry.size >= large_file_threshold {
                let modified_at = entry.modified_time
                    .and_then(|secs| chrono::DateTime::from_timestamp(secs as i64, 0))
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default();
                root_large_files.push(FileInfo {
                    path: entry.path.to_string_lossy().to_string(),
                    name: entry.name.clone(),
                    size: entry.size,
                    extension: entry.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string(),
                    modified_at,
                    is_readonly: entry.is_readonly,
                    is_symlink: false,
                    link_target: None,
                });
            }
        }
        println!("[{:.3}s] 根目录文件扫描完成: {} 个文件, {:.2} MB",
            step_time.elapsed().as_secs_f64(), root_file_count, root_file_size as f64 / 1024.0 / 1024.0);
        step_time = Instant::now();

        let dir_paths: Vec<PathBuf> = directories_map.iter()
            .filter(|(_, node)| !node.is_symlink)
            .map(|(path, _)| path.clone())
            .collect();
        println!("开始并行扫描 {} 个顶层目录...", dir_paths.len());

        struct DirResult {
            path: PathBuf,
            size: u64,
            file_count: usize,
            dir_count: usize,
            large_files: Vec<FileInfo>,
            inaccessible: usize,
            name: String,
            elapsed_secs: f64,
        }

        let dir_results: Vec<DirResult> = dir_paths.par_iter()
            .map(|dir_path| {
                let dir_start = Instant::now();
                let mut size = 0u64;
                let mut file_count = 0usize;
                let mut dir_count = 0usize;
                let mut large_files = Vec::new();
                let mut inaccessible = 0usize;
                let mut batch_files = 0usize;
                let mut batch_size = 0u64;
                let dir_name = dir_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let mut pending_dirs = vec![dir_path.clone()];

                while let Some(current_dir) = pending_dirs.pop() {
                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }

                    let entries = match winfs::enumerate_directory(&current_dir, false) {
                        Ok(entries) => entries,
                        Err(_) => {
                            inaccessible += 1;
                            continue;
                        }
                    };

                    for entry in entries {
                        if cancelled.load(Ordering::Relaxed) {
                            break;
                        }

                        if entry.is_symlink {
                            if entry.is_dir && entry.path != *dir_path {
                                dir_count += 1;
                            }
                            continue;
                        }

                        if entry.is_dir {
                            if entry.path != *dir_path {
                                dir_count += 1;
                            }
                            pending_dirs.push(entry.path);
                            continue;
                        }

                        size += entry.size;
                        file_count += 1;
                        batch_size += entry.size;
                        batch_files += 1;

                        if entry.size >= large_file_threshold {
                            let modified_at = entry.modified_time
                                .and_then(|secs| chrono::DateTime::from_timestamp(secs as i64, 0))
                                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                .unwrap_or_default();
                            large_files.push(FileInfo {
                                path: entry.path.to_string_lossy().to_string(),
                                name: entry.name,
                                size: entry.size,
                                extension: entry.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string(),
                                modified_at,
                                is_readonly: entry.is_readonly,
                                is_symlink: false,
                                link_target: None,
                            });
                        }

                        if batch_files >= 512 {
                            total_files.fetch_add(batch_files, Ordering::Relaxed);
                            total_size.fetch_add(batch_size, Ordering::Relaxed);
                            batch_files = 0;
                            batch_size = 0;
                        }
                    }
                }

                if batch_files > 0 {
                    total_files.fetch_add(batch_files, Ordering::Relaxed);
                    total_size.fetch_add(batch_size, Ordering::Relaxed);
                }

                let elapsed = dir_start.elapsed().as_secs_f64();
                let total_entries = file_count + dir_count;
                println!(
                    "  [{dir_name}] {elapsed:.2}s | {file_count}F {dir_count}D | {:.0}/s",
                    total_entries as f64 / elapsed.max(0.001),
                );

                DirResult {
                    path: dir_path.clone(), size, file_count, dir_count,
                    large_files, inaccessible, name: dir_name,
                    elapsed_secs: elapsed,
                }
            })
            .collect();

        let mut scanned_files = root_file_count;
        let mut scanned_dirs = 0usize;
        let mut scanned_size = root_file_size;
        let mut inaccessible_count = 0usize;
        let mut large_files_vec = root_large_files;

        for result in &dir_results {
            println!("  [{}] {:.2}s | {} 文件 | {:.2} GB | {} 大文件",
                result.name, result.elapsed_secs, result.file_count,
                result.size as f64 / 1024.0 / 1024.0 / 1024.0, result.large_files.len());

            if let Some(node) = directories_map.get_mut(&result.path) {
                node.size = result.size;
                node.file_count = result.file_count;
                node.dir_count = result.dir_count + 1;
            }
            scanned_files += result.file_count;
            scanned_dirs += result.dir_count + 1;
            scanned_size += result.size;
            inaccessible_count += result.inaccessible;
            large_files_vec.extend(result.large_files.iter().cloned());
        }

        total_size.store(scanned_size, Ordering::Relaxed);
        total_files.store(scanned_files, Ordering::Relaxed);
        total_dirs.store(scanned_dirs, Ordering::Relaxed);

        println!("[{:.3}s] 并行扫描完成: {} 个文件, {} 个目录, {} 个无法访问",
            step_time.elapsed().as_secs_f64(), scanned_files, scanned_dirs, inaccessible_count);
        step_time = Instant::now();

        should_stop.store(true, Ordering::Relaxed);
        let _ = progress_handle.join();
        println!("[{:.3}s] 进度线程已停止", step_time.elapsed().as_secs_f64());
        step_time = Instant::now();

        if cancelled.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("扫描已取消"));
        }

        let mut directories: Vec<DirectoryNode> = directories_map.into_values().collect();
        println!("[{:.3}s] 目录结果收集完成, {} 个顶层目录", step_time.elapsed().as_secs_f64(), directories.len());
        step_time = Instant::now();

        if root_file_count > 0 {
            directories.push(DirectoryNode {
                path: path.to_string_lossy().to_string(),
                name: "根目录文件".to_string(),
                size: root_file_size,
                file_count: root_file_count,
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
        directories.sort_by(|a, b| b.size.cmp(&a.size));
        large_files_vec.sort_by(|a, b| b.size.cmp(&a.size));
        println!("[{:.3}s] 排序完成", step_time.elapsed().as_secs_f64());
        step_time = Instant::now();

        cache.set_last_total_files(scanned_files);
        println!("[{:.3}s] 缓存保存完成", step_time.elapsed().as_secs_f64());

        let duration = start.elapsed();
        let total_dirs_value = Self::sum_dir_count(&directories);

        println!("========== 快速扫描完成 ==========");
        println!("总耗时: {:.2}s | 文件: {} | 目录: {} | 大小: {:.2} GB | 大文件: {}",
            duration.as_secs_f64(), scanned_files, scanned_dirs,
            scanned_size as f64 / 1024.0 / 1024.0 / 1024.0, large_files_vec.len());

        Ok(ScanResult {
            root_path,
            total_size: scanned_size,
            total_files: scanned_files,
            total_dirs: total_dirs_value,
            scan_duration_ms: duration.as_millis() as u64,
            directories,
            large_files: large_files_vec,
            inaccessible_count,
            root_file_id: None,
            usn_journal_id: None,
            usn_next_usn: None,
        })
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

    // ======================== 深度扫描 ========================

    fn scan_deep_blocking(path: &Path, app: AppHandle, estimated_files: usize, cancelled: Arc<AtomicBool>) -> Result<ScanResult> {
        let start = Instant::now();
        let root_path = path.to_string_lossy().to_string();

        println!("\n========== 深度扫描开始 ==========");
        println!("扫描路径: {}", root_path);

        let inaccessible_count = Arc::new(AtomicUsize::new(0));
        let mut large_files: Vec<super::file_info::FileInfo> = Vec::new();
        let large_file_threshold = 100 * 1024 * 1024u64;

        let total_files = Arc::new(AtomicUsize::new(0));
        let total_dirs = Arc::new(AtomicUsize::new(0));
        let total_size = Arc::new(AtomicU64::new(0));

        // jwalk 迭代器是单线程消费的，这些 map 只在主线程访问，无需 Arc<Mutex<>>
        let mut dir_file_stats: HashMap<PathBuf, (u64, usize)> = HashMap::new();
        let mut dir_nodes: HashMap<PathBuf, DirectoryNode> = HashMap::new();

        let _ = app.emit("deep-scan-progress", ScanProgress {
            scanned_files: 0, scanned_dirs: 0, total_size: 0,
            current_path: root_path.clone(), elapsed_ms: 0,
            files_per_second: 0.0, progress_percent: 0.0,
        });

        // 进度报告线程
        let app_c = app.clone();
        let tf = Arc::clone(&total_files);
        let td = Arc::clone(&total_dirs);
        let ts = Arc::clone(&total_size);
        let start_c = start.clone();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_c = Arc::clone(&should_stop);
        let cancelled_c = Arc::clone(&cancelled);

        let progress_handle = std::thread::spawn(move || {
            let mut last_files = 0usize;
            let mut last_time = Instant::now();
            let mut est = (estimated_files as f64 * 1.1) as usize;
            let mut last_pct = 0.0f64;

            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                if should_stop_c.load(Ordering::Relaxed) || cancelled_c.load(Ordering::Relaxed) { break; }

                let cur = tf.load(Ordering::Relaxed);
                let elapsed = start_c.elapsed().as_millis() as u64;
                let now = Instant::now();
                let dt = now.duration_since(last_time).as_secs_f64();
                let fps = if dt > 0.0 { (cur - last_files) as f64 / dt } else { 0.0 };

                if cur > (est as f64 * 0.9) as usize { est = (cur as f64 * 1.2) as usize; }
                let raw_pct = if est > 0 { (cur as f64 / est as f64 * 100.0).min(99.0) } else { 0.0 };
                let pct = raw_pct.max(last_pct);
                last_pct = pct;
                last_files = cur;
                last_time = now;

                let _ = app_c.emit("deep-scan-progress", ScanProgress {
                    scanned_files: cur as u64, scanned_dirs: td.load(Ordering::Relaxed) as u64,
                    total_size: ts.load(Ordering::Relaxed), current_path: "深度扫描中...".to_string(),
                    elapsed_ms: elapsed, files_per_second: fps, progress_percent: pct,
                });
            }
        });

        let mut pending_dirs = vec![path.to_path_buf()];
        while let Some(current_dir) = pending_dirs.pop() {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }

            let entries = match winfs::enumerate_directory(&current_dir, true) {
                Ok(entries) => entries,
                Err(_) => {
                    inaccessible_count.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            };

            for entry in entries {
                if cancelled.load(Ordering::Relaxed) {
                    break;
                }

                if entry.is_symlink {
                    if entry.is_dir {
                        total_dirs.fetch_add(1, Ordering::Relaxed);
                        dir_nodes.insert(entry.path.clone(), DirectoryNode {
                            path: entry.path.to_string_lossy().to_string(),
                            name: entry.name,
                            size: 0,
                            file_count: 0,
                            dir_count: 1,
                            children: vec![],
                            has_children: false,
                            is_symlink: true,
                            link_target: Self::resolve_link_target(&entry.path),
                            safety: None,
                            modified_time: entry.modified_time,
                            file_id: None,
                        });
                    }
                    continue;
                }

                if entry.is_dir {
                    total_dirs.fetch_add(1, Ordering::Relaxed);
                    pending_dirs.push(entry.path.clone());

                    dir_nodes.insert(entry.path.clone(), DirectoryNode {
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
                    });
                    continue;
                }

                let file_size = entry.size;
                total_files.fetch_add(1, Ordering::Relaxed);
                total_size.fetch_add(file_size, Ordering::Relaxed);

                if file_size >= large_file_threshold {
                    let modified_at = entry.modified_time
                        .and_then(|secs| chrono::DateTime::from_timestamp(secs as i64, 0))
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default();

                    large_files.push(super::file_info::FileInfo {
                        path: entry.path.to_string_lossy().to_string(),
                        name: entry.name,
                        size: file_size,
                        extension: entry.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string(),
                        modified_at,
                        is_readonly: entry.is_readonly,
                        is_symlink: false,
                        link_target: None,
                    });
                }

                if let Some(parent) = entry.path.parent() {
                    let stats = dir_file_stats.entry(parent.to_path_buf()).or_insert((0, 0));
                    stats.0 += file_size;
                    stats.1 += 1;
                }
            }
        }

        should_stop.store(true, Ordering::Relaxed);
        let _ = progress_handle.join();

        if cancelled.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("扫描已取消"));
        }

        // 构建目录树
        let mut nodes_map = dir_nodes;

        let (root_file_count, root_file_size) = dir_file_stats.get(path)
            .map(|&(size, count)| (count, size))
            .unwrap_or((0, 0));

        for (dir_path, (size, count)) in &dir_file_stats {
            if let Some(node) = nodes_map.get_mut(dir_path) {
                node.size = *size;
                node.file_count = *count;
            }
        }

        // 从最深层开始累加子目录大小到父目录
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

        // 构建树结构
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
            .filter(|(p, _)| p.parent() == Some(path))
            .map(|(_, node)| node)
            .collect();

        if root_file_count > 0 {
            directories.push(DirectoryNode {
                path: path.to_string_lossy().to_string(),
                name: "根目录文件".to_string(),
                size: root_file_size,
                file_count: root_file_count,
                dir_count: 0,
                children: Vec::new(),
                has_children: false,
                is_symlink: false, link_target: None,
                safety: None, modified_time: None,
                file_id: None,
            });
        }

        Self::sort_directory_tree(&mut directories);

        // 不做全局校准——差异来自系统保护/无权限文件，不应"注水"到用户目录
        let scanned_size = total_size.load(Ordering::Relaxed);
        let scanned_files = total_files.load(Ordering::Relaxed);
        let scanned_dirs = Self::sum_dir_count(&directories);

        let duration = start.elapsed();
        let large_files_vec = large_files;

        println!("========== 深度扫描完成 ==========");
        println!("耗时: {:.2}s | 文件: {} | 目录: {} | 大小: {:.2} GB",
            duration.as_secs_f64(), scanned_files,
            scanned_dirs,
            scanned_size as f64 / 1024.0 / 1024.0 / 1024.0);

        if let Some((_, disk_used, _)) = Self::get_disk_usage(path) {
            let diff_pct = if disk_used > 0 {
                ((scanned_size as f64 - disk_used as f64) / disk_used as f64 * 100.0).abs()
            } else {
                0.0
            };
            println!("磁盘已用: {:.2} GB | 差异: {:.1}% (系统保留/无权限文件)",
                disk_used as f64 / 1024.0 / 1024.0 / 1024.0, diff_pct);
        }

        let journal = winfs::query_usn_checkpoint(path);

        Ok(ScanResult {
            root_path,
            total_size: scanned_size,
            total_files: scanned_files,
            total_dirs: scanned_dirs,
            scan_duration_ms: duration.as_millis() as u64,
            directories,
            large_files: large_files_vec,
            inaccessible_count: inaccessible_count.load(Ordering::Relaxed),
            root_file_id: winfs::get_path_file_id(path),
            usn_journal_id: journal.map(|item| item.journal_id),
            usn_next_usn: journal.map(|item| item.next_usn),
        })
    }

    fn sort_directory_tree(nodes: &mut [DirectoryNode]) {
        for node in nodes.iter_mut() {
            Self::sort_directory_tree(&mut node.children);
            node.has_children = !node.children.is_empty();
            node.children.sort_by(|a, b| b.size.cmp(&a.size));
        }
        nodes.sort_by(|a, b| b.size.cmp(&a.size));
    }

    fn sum_dir_count(nodes: &[DirectoryNode]) -> usize {
        nodes.iter().map(|node| node.dir_count).sum()
    }

    #[allow(dead_code)]
    fn analyze_directory_safety(dir: &mut DirectoryNode, app: &AppHandle) {
        let path = Path::new(&dir.path);
        dir.safety = crate::safety::analyze_migration_safety(path, dir.size, app.clone()).ok();
        for child in &mut dir.children {
            Self::analyze_directory_safety(child, app);
        }
    }
}
