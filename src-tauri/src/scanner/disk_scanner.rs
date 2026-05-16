use super::backend::{self, ScanBackendKind};
use super::file_info::{DirectoryNode, ScanResult};
use super::progress::ScanProgress;
use super::scan_index::IndexedScanResult;
use super::timing::StageTimer;
use anyhow::Result;
use chrono;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

use crate::winfs;
use tauri::{AppHandle, Emitter, Runtime};

pub(crate) type ScanProgressEmitter = Arc<dyn Fn(ScanProgress) + Send + Sync>;

pub struct DiskScanner {
    cancelled: Arc<AtomicBool>,
    sessions: Arc<Mutex<HashMap<String, IndexedScanResult>>>,
}

impl DiskScanner {
    pub fn new() -> Self {
        Self {
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
            )
            .is_ok()
            {
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

    pub async fn scan_deep<P: AsRef<Path>, R: Runtime>(
        &self,
        path: P,
        app: AppHandle<R>,
        estimated_files: usize,
    ) -> Result<ScanResult> {
        let progress_emitter: ScanProgressEmitter = Arc::new(move |progress: ScanProgress| {
            let _ = app.emit("deep-scan-progress", progress);
        });
        self.scan_deep_with_progress(path, Some(progress_emitter), estimated_files)
            .await
    }

    pub async fn scan_deep_silent<P: AsRef<Path>>(
        &self,
        path: P,
        estimated_files: usize,
    ) -> Result<ScanResult> {
        self.scan_deep_with_progress(path, None, estimated_files)
            .await
    }

    pub(crate) async fn scan_deep_with_progress<P: AsRef<Path>>(
        &self,
        path: P,
        progress_emitter: Option<ScanProgressEmitter>,
        estimated_files: usize,
    ) -> Result<ScanResult> {
        let path = path.as_ref().to_path_buf();
        let cancelled = Arc::clone(&self.cancelled);
        self.reset_cancel();

        tokio::task::spawn_blocking(move || {
            Self::scan_deep_blocking_internal(&path, estimated_files, cancelled, progress_emitter)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    fn emit_progress(progress_emitter: Option<&ScanProgressEmitter>, progress: ScanProgress) {
        if let Some(emitter) = progress_emitter {
            emitter(progress);
        }
    }

    fn scan_deep_blocking_internal(
        path: &Path,
        estimated_files: usize,
        cancelled: Arc<AtomicBool>,
        progress_emitter: Option<ScanProgressEmitter>,
    ) -> Result<ScanResult> {
        let total_timer = StageTimer::start(
            "deep-scan",
            format!("scan_deep_blocking_internal path={}", path.display()),
        );
        let backend_timer = StageTimer::start(
            "deep-scan",
            format!("select_backend path={}", path.display()),
        );
        let preferred_backend = backend::select_backend(path);
        backend_timer.finish_with(format!("preferred_backend={}", preferred_backend.label()));
        println!(
            "[deep-scan] request path={} estimated_files={} preferred_backend={}",
            path.display(),
            estimated_files,
            preferred_backend.label()
        );

        if preferred_backend == ScanBackendKind::MftUsn {
            let mft_timer = StageTimer::start(
                "deep-scan",
                format!("attempt_mft_usn_backend path={}", path.display()),
            );
            match super::mft_usn::scan_path(
                path,
                progress_emitter.clone(),
                estimated_files,
                Arc::clone(&cancelled),
            ) {
                Ok(Some(result)) => {
                    let detail = format!(
                        "status=used backend={:?} files={} dirs={} size={} inaccessible={}",
                        result.scan_backend,
                        result.total_files,
                        result.total_dirs,
                        result.total_size,
                        result.inaccessible_count
                    );
                    mft_timer.finish_with(&detail);
                    total_timer.finish_with(&detail);
                    return Ok(result);
                }
                Ok(None) => {
                    mft_timer.finish_with("status=fallback reason=unsupported_or_unavailable");
                    println!("[mft-usn] 路径不满足条件，回退到原生递归扫描");
                }
                Err(err) => {
                    mft_timer.finish_with(format!("status=fallback reason={err}"));
                    println!("[mft-usn] 扫描失败({err})，回退到原生递归扫描");
                }
            }
        }

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

        Self::emit_progress(
            progress_emitter.as_ref(),
            ScanProgress {
                scanned_files: 0,
                scanned_dirs: 0,
                total_size: 0,
                current_path: root_path.clone(),
                elapsed_ms: 0,
                files_per_second: 0.0,
                progress_percent: 0.0,
            },
        );

        // 进度报告线程
        let tf = Arc::clone(&total_files);
        let td = Arc::clone(&total_dirs);
        let ts = Arc::clone(&total_size);
        let start_c = start.clone();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_c = Arc::clone(&should_stop);
        let cancelled_c = Arc::clone(&cancelled);
        let progress_emitter_c = progress_emitter.clone();

        let progress_handle = std::thread::spawn(move || {
            let mut last_files = 0usize;
            let mut last_time = Instant::now();
            let mut est = (estimated_files as f64 * 1.1) as usize;
            let mut last_pct = 0.0f64;

            loop {
                // 把 500ms 拆成 25 片 × 20ms，每片之间检查停止旗，
                // 避免扫描已结束但还要硬等 500ms 才退出的问题。
                let mut waited = 0u64;
                while waited < 500 {
                    if should_stop_c.load(Ordering::Relaxed)
                        || cancelled_c.load(Ordering::Relaxed)
                    {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    waited += 20;
                }
                if should_stop_c.load(Ordering::Relaxed) || cancelled_c.load(Ordering::Relaxed) {
                    break;
                }

                let cur = tf.load(Ordering::Relaxed);
                let elapsed = start_c.elapsed().as_millis() as u64;
                let now = Instant::now();
                let dt = now.duration_since(last_time).as_secs_f64();
                let fps = if dt > 0.0 {
                    (cur - last_files) as f64 / dt
                } else {
                    0.0
                };

                if cur > (est as f64 * 0.9) as usize {
                    est = (cur as f64 * 1.2) as usize;
                }
                let raw_pct = if est > 0 {
                    (cur as f64 / est as f64 * 100.0).min(99.0)
                } else {
                    0.0
                };
                let pct = raw_pct.max(last_pct);
                last_pct = pct;
                last_files = cur;
                last_time = now;

                Self::emit_progress(
                    progress_emitter_c.as_ref(),
                    ScanProgress {
                        scanned_files: cur as u64,
                        scanned_dirs: td.load(Ordering::Relaxed) as u64,
                        total_size: ts.load(Ordering::Relaxed),
                        current_path: "深度扫描中...".to_string(),
                        elapsed_ms: elapsed,
                        files_per_second: fps,
                        progress_percent: pct,
                    },
                );
            }
        });

        let enumerate_timer = StageTimer::start(
            "deep-native",
            format!("walk_native_directory_tree path={}", path.display()),
        );
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
                    total_dirs.fetch_add(1, Ordering::Relaxed);
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

                let file_size = entry.size;
                total_files.fetch_add(1, Ordering::Relaxed);
                total_size.fetch_add(file_size, Ordering::Relaxed);

                if file_size >= large_file_threshold {
                    let modified_at = entry
                        .modified_time
                        .and_then(|secs| chrono::DateTime::from_timestamp(secs as i64, 0))
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default();

                    large_files.push(super::file_info::FileInfo {
                        path: entry.path.to_string_lossy().to_string(),
                        name: entry.name,
                        size: file_size,
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

                if let Some(parent) = entry.path.parent() {
                    let stats = dir_file_stats.entry(parent.to_path_buf()).or_insert((0, 0));
                    stats.0 += file_size;
                    stats.1 += 1;
                }
            }
        }
        enumerate_timer.finish_with(format!(
            "files={} dirs={} size={} inaccessible={} large_files={}",
            total_files.load(Ordering::Relaxed),
            total_dirs.load(Ordering::Relaxed),
            total_size.load(Ordering::Relaxed),
            inaccessible_count.load(Ordering::Relaxed),
            large_files.len()
        ));

        should_stop.store(true, Ordering::Relaxed);
        let _ = progress_handle.join();

        if cancelled.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("扫描已取消"));
        }

        // 构建目录树
        let aggregate_timer = StageTimer::start(
            "deep-native",
            format!("aggregate_directory_totals path={}", path.display()),
        );
        let mut nodes_map = dir_nodes;

        let (root_file_count, root_file_size) = dir_file_stats
            .get(path)
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
        aggregate_timer.finish_with(format!(
            "nodes={} tracked_dirs={} root_file_count={} root_file_size={}",
            nodes_map.len(),
            dir_file_stats.len(),
            root_file_count,
            root_file_size
        ));

        // 构建树结构
        let tree_timer = StageTimer::start(
            "deep-native",
            format!("build_directory_tree path={}", path.display()),
        );
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
                is_symlink: false,
                link_target: None,
                safety: None,
                modified_time: None,
                file_id: None,
            });
        }
        tree_timer.finish_with(format!(
            "top_level_nodes={} total_paths={}",
            directories.len(),
            all_paths.len()
        ));

        let sort_timer = StageTimer::start(
            "deep-native",
            format!("sort_directory_tree path={}", path.display()),
        );
        Self::sort_directory_tree(&mut directories);
        sort_timer.finish_with(format!("top_level_nodes={}", directories.len()));

        // 不做全局校准——差异来自系统保护/无权限文件，不应"注水"到用户目录
        let scanned_size = total_size.load(Ordering::Relaxed);
        let scanned_files = total_files.load(Ordering::Relaxed);
        let scanned_dirs = Self::sum_dir_count(&directories);

        let duration = start.elapsed();
        let large_files_vec = large_files;

        println!("========== 深度扫描完成 ==========");
        println!(
            "耗时: {:.2}s | 文件: {} | 目录: {} | 大小: {:.2} GB",
            duration.as_secs_f64(),
            scanned_files,
            scanned_dirs,
            scanned_size as f64 / 1024.0 / 1024.0 / 1024.0
        );

        let disk_usage_timer = StageTimer::start(
            "deep-native",
            format!("query_disk_usage path={}", path.display()),
        );
        let disk_usage = Self::get_disk_usage(path);
        match disk_usage {
            Some((_, disk_used, _)) => {
                disk_usage_timer.finish_with(format!("disk_used={disk_used}"));
                let missing = disk_used.saturating_sub(scanned_size);
                let missing_pct = if disk_used > 0 {
                    missing as f64 / disk_used as f64 * 100.0
                } else {
                    0.0
                };
                println!(
                    "磁盘已用: {:.2} GB | 扫描到: {:.2} GB | 漏算量: {:.2} GB ({:.1}%) (系统保留/无权限文件)",
                    disk_used as f64 / 1024.0 / 1024.0 / 1024.0,
                    scanned_size as f64 / 1024.0 / 1024.0 / 1024.0,
                    missing as f64 / 1024.0 / 1024.0 / 1024.0,
                    missing_pct
                );
            }
            None => {
                disk_usage_timer.finish_with("disk_used=unavailable");
            }
        }

        let checkpoint_timer = StageTimer::start(
            "deep-native",
            format!("capture_usn_checkpoint path={}", path.display()),
        );
        let journal = winfs::query_usn_checkpoint(path);
        match &journal {
            Some(checkpoint) => {
                checkpoint_timer.finish_with(format!(
                    "available=true journal_id={} next_usn={}",
                    checkpoint.journal_id, checkpoint.next_usn
                ));
            }
            None => {
                checkpoint_timer.finish_with("available=false");
            }
        }

        if let Some(checkpoint) = journal {
            println!(
                "[deep-scan] captured USN checkpoint journal_id={} next_usn={}",
                checkpoint.journal_id, checkpoint.next_usn
            );
        } else {
            println!(
                "[deep-scan] USN checkpoint unavailable for {}; future runs will reuse the fresh cache only when fast incremental data is available",
                path.display()
            );
        }

        total_timer.finish_with(format!(
            "backend={} files={} dirs={} size={} inaccessible={}",
            ScanBackendKind::Native.label(),
            scanned_files,
            scanned_dirs,
            scanned_size,
            inaccessible_count.load(Ordering::Relaxed)
        ));

        if let Some(journal) = journal {
            return Ok(ScanResult {
                root_path,
                total_size: scanned_size,
                total_files: scanned_files,
                total_dirs: scanned_dirs,
                scan_duration_ms: duration.as_millis() as u64,
                directories,
                large_files: large_files_vec,
                inaccessible_count: inaccessible_count.load(Ordering::Relaxed),
                scan_backend: Some(ScanBackendKind::Native.label().to_string()),
                root_file_id: winfs::get_path_file_id(path),
                usn_journal_id: Some(journal.journal_id),
                usn_next_usn: Some(journal.next_usn),
            });
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
            scan_backend: Some(ScanBackendKind::Native.label().to_string()),
            root_file_id: winfs::get_path_file_id(path),
            usn_journal_id: None,
            usn_next_usn: None,
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
    fn analyze_directory_safety(dir: &mut DirectoryNode, _app: &AppHandle) {
        let path = Path::new(&dir.path);
        dir.safety = Some(crate::safety::analyze(
            path,
            crate::migration::LinkType::Auto,
            None,
            dir.size,
        ));
        for child in &mut dir.children {
            Self::analyze_directory_safety(child, _app);
        }
    }
}
