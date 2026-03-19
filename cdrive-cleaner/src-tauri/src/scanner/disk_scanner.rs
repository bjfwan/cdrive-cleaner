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

        let entries: Vec<_> = match fs::read_dir(path) {
            Ok(entries) => entries.collect(),
            Err(e) => return Err(anyhow::anyhow!("Failed to read directory: {}", e)),
        };
        println!("[{:.3}s] read_dir 完成, {} 个条目", step_time.elapsed().as_secs_f64(), entries.len());
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

        let mut directories_map: HashMap<PathBuf, DirectoryNode> = entries.par_iter()
            .filter_map(|entry_result| {
                let entry = match entry_result {
                    Ok(e) => e,
                    Err(_) => return None,
                };
                let path = entry.path();
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(m) => m,
                    Err(_) => return None,
                };
                let is_symlink = Self::is_link_entry(&metadata);
                let is_directory = if is_symlink {
                    Self::path_points_to_directory(&path)
                } else {
                    metadata.is_dir()
                };

                if is_directory {
                    let link_target = if is_symlink {
                        Self::resolve_link_target(&path)
                    } else { None };
                    let modified_time = metadata.modified().ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs());

                    Some((path.clone(), DirectoryNode {
                        path: path.to_string_lossy().to_string(),
                        name: entry.file_name().to_string_lossy().to_string(),
                        size: 0,
                        file_count: 0,
                        dir_count: 1,
                        children: vec![],
                        has_children: false,
                        is_symlink,
                        link_target,
                        safety: None,
                        modified_time,
                    }))
                } else { None }
            })
            .collect();

        println!("[{:.3}s] rayon 构建 directories_map 完成, {} 个目录", step_time.elapsed().as_secs_f64(), directories_map.len());
        step_time = Instant::now();

        let mut root_file_size = 0u64;
        let mut root_file_count = 0usize;
        let mut root_large_files: Vec<FileInfo> = Vec::new();
        if let Ok(root_entries) = fs::read_dir(path) {
            for entry in root_entries.flatten() {
                let entry_path = entry.path();
                let Ok(metadata) = fs::symlink_metadata(&entry_path) else { continue; };

                if Self::is_link_entry(&metadata) {
                    continue;
                }

                if metadata.is_file() {
                    root_file_size += metadata.len();
                    root_file_count += 1;
                    if metadata.len() >= large_file_threshold {
                        root_large_files.push(Self::build_file_info(&entry_path, &metadata));
                    }
                }
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
                let mut local_counter = 0usize;
                let dir_name = dir_path.file_name().unwrap_or_default().to_string_lossy().to_string();

                let mut t_iter_total: u64 = 0;
                let mut t_meta_total: u64 = 0;
                let mut t_proc_total: u64 = 0;
                let mut last_report = Instant::now();

                let walker = jwalk::WalkDir::new(dir_path).skip_hidden(false).follow_links(false);
                let mut iter = walker.into_iter();
                let mut t0 = Instant::now();

                while let Some(entry_result) = iter.next() {
                    let t1 = Instant::now();
                    t_iter_total += t1.duration_since(t0).as_nanos() as u64;

                    if cancelled.load(Ordering::Relaxed) { break; }

                    match entry_result {
                        Ok(entry) => {
                            let entry_path = entry.path();
                            let tm0 = Instant::now();
                            let link_meta_result = fs::symlink_metadata(&entry_path);
                            let tm1 = Instant::now();
                            t_meta_total += tm1.duration_since(tm0).as_nanos() as u64;

                            let Ok(link_metadata) = link_meta_result else {
                                inaccessible += 1;
                                t0 = Instant::now();
                                continue;
                            };

                            if Self::is_link_entry(&link_metadata) {
                                if Self::path_points_to_directory(&entry_path) && entry_path != *dir_path {
                                    dir_count += 1;
                                }
                                t0 = Instant::now();
                                continue;
                            }

                            if let Ok(metadata) = entry.metadata() {
                                if metadata.is_file() {
                                    let file_size = metadata.len();
                                    size += file_size;
                                    file_count += 1;
                                    if file_size >= large_file_threshold {
                                        large_files.push(Self::build_file_info(&entry_path, &metadata));
                                    }
                                } else if metadata.is_dir() && entry_path != *dir_path {
                                    dir_count += 1;
                                }
                            } else {
                                inaccessible += 1;
                            }
                            t_proc_total += tm1.elapsed().as_nanos() as u64;
                        }
                        Err(_) => { inaccessible += 1; }
                    }

                    local_counter += 1;
                    if local_counter >= 2048 {
                        total_files.fetch_add(local_counter, Ordering::Relaxed);
                        total_size.fetch_add(size, Ordering::Relaxed);
                        local_counter = 0;
                    }

                    if last_report.elapsed().as_secs() >= 5 {
                        let total_entries = file_count + dir_count;
                        let wall = dir_start.elapsed().as_secs_f64();
                        let sum_ns = (t_iter_total + t_meta_total + t_proc_total).max(1) as f64;
                        println!("    [{dir_name}] {wall:.1}s | {total_entries} 条目 | \
                            {:.0}/s | iter={:.1}% meta={:.1}% proc={:.1}%",
                            total_entries as f64 / wall.max(0.001),
                            t_iter_total as f64 / sum_ns * 100.0,
                            t_meta_total as f64 / sum_ns * 100.0,
                            t_proc_total as f64 / sum_ns * 100.0);
                        last_report = Instant::now();
                    }

                    t0 = Instant::now();
                }

                total_files.fetch_add(local_counter, Ordering::Relaxed);

                let elapsed = dir_start.elapsed().as_secs_f64();
                let sum_ns = (t_iter_total + t_meta_total + t_proc_total).max(1) as f64;
                let total_entries = file_count + dir_count;
                println!("  [{dir_name}] {elapsed:.2}s | {file_count}F {dir_count}D | \
                    {:.0}/s | iter={:.1}% meta={:.1}% proc={:.1}%",
                    total_entries as f64 / elapsed.max(0.001),
                    t_iter_total as f64 / sum_ns * 100.0,
                    t_meta_total as f64 / sum_ns * 100.0,
                    t_proc_total as f64 / sum_ns * 100.0);

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
        })
    }

    /// 使用 jwalk 并行遍历计算目录大小（替代 walkdir + par_bridge）
    fn top_level_child_path(root: &Path, path: &Path) -> Option<PathBuf> {
        let relative = path.strip_prefix(root).ok()?;
        let mut components = relative.components();
        let first = components.next()?;
        Some(root.join(first.as_os_str()))
    }

    fn build_quick_node(path: &Path) -> DirectoryNode {
        DirectoryNode {
            path: path.to_string_lossy().to_string(),
            name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            size: 0,
            file_count: 0,
            dir_count: 1,
            children: vec![],
            has_children: false,
            is_symlink: false,
            link_target: None,
            safety: None,
            modified_time: None,
        }
    }

    fn build_file_info(path: &Path, metadata: &fs::Metadata) -> FileInfo {
        let modified_at = metadata.modified()
            .ok()
            .map(|modified| {
                let datetime: chrono::DateTime<chrono::Local> = modified.into();
                datetime.format("%Y-%m-%d %H:%M:%S").to_string()
            })
            .unwrap_or_default();

        FileInfo {
            path: path.to_string_lossy().to_string(),
            name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            size: metadata.len(),
            extension: path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string(),
            modified_at,
            is_readonly: metadata.permissions().readonly(),
            is_symlink: false,
            link_target: None,
        }
    }

    #[cfg(windows)]
    fn is_link_entry(metadata: &fs::Metadata) -> bool {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    #[cfg(not(windows))]
    fn is_link_entry(metadata: &fs::Metadata) -> bool {
        metadata.file_type().is_symlink()
    }

    fn path_points_to_directory(path: &Path) -> bool {
        fs::metadata(path).map(|metadata| metadata.is_dir()).unwrap_or(false)
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

        // jwalk 并行遍历目录树，迭代器单线程消费，直接写入本地 HashMap
        for entry in jwalk::WalkDir::new(path).skip_hidden(false).follow_links(false) {
            if cancelled.load(Ordering::Relaxed) { break; }

            match entry {
                Ok(entry) => {
                    let entry_path = entry.path();
                    let Ok(link_metadata) = fs::symlink_metadata(&entry_path) else {
                        inaccessible_count.fetch_add(1, Ordering::Relaxed);
                        continue;
                    };

                    if Self::is_link_entry(&link_metadata) {
                        if Self::path_points_to_directory(&entry_path) {
                            total_dirs.fetch_add(1, Ordering::Relaxed);
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
                                link_target: Self::resolve_link_target(&entry_path),
                                safety: None,
                                modified_time,
                            });
                        }
                        continue;
                    }

                    if link_metadata.is_file() {
                        let file_size = link_metadata.len();
                        total_files.fetch_add(1, Ordering::Relaxed);
                        total_size.fetch_add(file_size, Ordering::Relaxed);

                        if file_size >= large_file_threshold {
                            let file_info = super::file_info::FileInfo {
                                path: entry_path.to_string_lossy().to_string(),
                                name: entry_path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown").to_string(),
                                size: file_size,
                                extension: entry_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string(),
                                modified_at: link_metadata.modified().ok()
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
                                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                    .unwrap_or_default(),
                                is_readonly: link_metadata.permissions().readonly(),
                                is_symlink: false,
                                link_target: None,
                            };
                            large_files.push(file_info);
                        }

                        if let Some(parent) = entry_path.parent() {
                            let stats = dir_file_stats.entry(parent.to_path_buf()).or_insert((0, 0));
                            stats.0 += file_size;
                            stats.1 += 1;
                        }
                    } else if link_metadata.is_dir() {
                        total_dirs.fetch_add(1, Ordering::Relaxed);
                        let modified_time = link_metadata.modified().ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs());

                        dir_nodes.insert(entry_path.to_path_buf(), DirectoryNode {
                            path: entry_path.to_string_lossy().to_string(),
                            name: entry_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                            size: 0, file_count: 0, dir_count: 1, children: vec![],
                            has_children: false,
                            is_symlink: false,
                            link_target: None,
                            safety: None,
                            modified_time,
                        });
                    }
                }
                Err(_) => { inaccessible_count.fetch_add(1, Ordering::Relaxed); }
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

        Ok(ScanResult {
            root_path,
            total_size: scanned_size,
            total_files: scanned_files,
            total_dirs: scanned_dirs,
            scan_duration_ms: duration.as_millis() as u64,
            directories,
            large_files: large_files_vec,
            inaccessible_count: inaccessible_count.load(Ordering::Relaxed),
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
