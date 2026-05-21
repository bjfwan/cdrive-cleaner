use super::backend::{self, ScanBackendKind};
use super::file_info::{DirectoryChildrenSnapshot, DirectoryNode, ScanResult};
use super::progress::ScanProgress;
use super::scan_index::IndexedScanResult;
use super::timing::StageTimer;
use crate::session::CancellationToken;
use anyhow::Result;
use chrono;
use dashmap::DashMap;
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
        let sessions = self.sessions.lock().ok()?;
        sessions.get(root_path)?.snapshot_for_path(path)
    }

    pub fn get_directory_children(
        &self,
        root_path: &str,
        path: &str,
    ) -> Option<DirectoryChildrenSnapshot> {
        let sessions = self.sessions.lock().ok()?;
        sessions.get(root_path)?.children_snapshot_for_path(path)
    }

    pub fn with_indexed<F, T>(&self, root_path: &str, f: F) -> Option<T>
    where
        F: FnOnce(&IndexedScanResult) -> T,
    {
        let sessions = self.sessions.lock().ok()?;
        let indexed = sessions.get(root_path)?;
        Some(f(indexed))
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

    /// 与 [`scan_deep`] 等价，但取消信号由外部 [`CancellationToken`] 提供。
    /// 命令层在拿到 RAII session handle 之后应该用这个入口，让 session 退出时
    /// 自动停掉扫描。
    pub async fn scan_deep_with_token<P: AsRef<Path>, R: Runtime>(
        &self,
        path: P,
        app: AppHandle<R>,
        estimated_files: usize,
        cancellation: &CancellationToken,
    ) -> Result<ScanResult> {
        let progress_emitter: ScanProgressEmitter = Arc::new(move |progress: ScanProgress| {
            let _ = app.emit("deep-scan-progress", progress);
        });
        self.scan_deep_with_progress_and_token(
            path,
            Some(progress_emitter),
            estimated_files,
            Some(cancellation),
        )
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
        self.scan_deep_with_progress_and_token(path, progress_emitter, estimated_files, None)
            .await
    }

    /// 通用入口：可选地接受一个 [`CancellationToken`]。
    /// - 不传：沿用原来"DiskScanner 自己持有的 AtomicBool + reset_cancel()"语义；
    /// - 传：把外部 token 的 atomic flag 同步到 self.cancelled，扫描期间任意一边
    ///   触发 cancel，扫描内部循环都会看到。
    pub(crate) async fn scan_deep_with_progress_and_token<P: AsRef<Path>>(
        &self,
        path: P,
        progress_emitter: Option<ScanProgressEmitter>,
        estimated_files: usize,
        cancellation: Option<&CancellationToken>,
    ) -> Result<ScanResult> {
        let path = path.as_ref().to_path_buf();

        // 选定扫描期间真正被 mft_usn / native walk 内部循环 load 的那个 flag。
        // 优先用外部 token：注册表 + RAII guard 才能保证 panic / 提前 return 时
        // 也会自动 cancel，避免遗留半成品扫描。
        let cancelled = match cancellation {
            Some(token) => {
                // 进入新扫描时把外部 token 的标志位重置一下，避免上次取消遗留
                // 影响这次扫描。Token 的所有权在 SessionHandle 上，重置是安全的。
                token.as_atomic().store(false, Ordering::Relaxed);
                token.as_atomic()
            }
            None => {
                self.reset_cancel();
                Arc::clone(&self.cancelled)
            }
        };

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
        tracing::info!(
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
                    tracing::info!("[mft-usn] 路径不满足条件，回退到原生递归扫描");
                }
                Err(err) => {
                    mft_timer.finish_with(format!("status=fallback reason={err}"));
                    tracing::info!("[mft-usn] 扫描失败({err})，回退到原生递归扫描");
                }
            }
        }

        let start = Instant::now();
        let root_path = path.to_string_lossy().to_string();

        tracing::info!("\n========== 深度扫描开始 ==========");
        tracing::info!("扫描路径: {}", root_path);

        let inaccessible_count = Arc::new(AtomicUsize::new(0));
        let large_file_threshold = 100 * 1024 * 1024u64;

        let total_files = Arc::new(AtomicUsize::new(0));
        let total_dirs = Arc::new(AtomicUsize::new(0));
        let total_size = Arc::new(AtomicU64::new(0));

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
        let start_c = start;
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

        // --- Parallel native walk using rayon::scope ---
        let dir_nodes: DashMap<PathBuf, DirectoryNode> =
            DashMap::with_capacity(estimated_files / 8);
        let dir_file_stats: DashMap<PathBuf, (u64, usize)> =
            DashMap::with_capacity(estimated_files / 8);
        let large_files: Mutex<Vec<super::file_info::FileInfo>> = Mutex::new(Vec::new());

        rayon::scope(|s| {
            Self::walk_dir_parallel(
                s,
                path.to_path_buf(),
                &cancelled,
                &inaccessible_count,
                &total_files,
                &total_dirs,
                &total_size,
                &dir_nodes,
                &dir_file_stats,
                &large_files,
                large_file_threshold,
            );
        });

        let large_files = large_files.into_inner().unwrap();

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

        // Convert DashMap to HashMap for sequential post-processing
        let dir_file_stats: HashMap<PathBuf, (u64, usize)> = dir_file_stats.into_iter().collect();
        let mut nodes_map: HashMap<PathBuf, DirectoryNode> = dir_nodes.into_iter().collect();

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
        all_paths.sort_by_key(|p| std::cmp::Reverse(p.components().count()));

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

        tracing::info!("========== 深度扫描完成 ==========");
        tracing::info!(
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
        let mut system_reserved_bytes = 0;
        match disk_usage {
            Some((_, disk_used, _)) => {
                disk_usage_timer.finish_with(format!("disk_used={disk_used}"));
                let missing = disk_used.saturating_sub(scanned_size);
                system_reserved_bytes = missing;
                let missing_pct = if disk_used > 0 {
                    missing as f64 / disk_used as f64 * 100.0
                } else {
                    0.0
                };
                tracing::info!(
                    "磁盘已用: {:.2} GB | 扫描到普通文件: {:.2} GB | 系统保留/卷元数据差额: {:.2} GB ({:.1}%)",
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
            tracing::info!(
                "[deep-scan] captured USN checkpoint journal_id={} next_usn={}",
                checkpoint.journal_id, checkpoint.next_usn
            );
        } else {
            tracing::info!(
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
                system_reserved_bytes,
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
                cache_schema_version: 0,
                env_fingerprint: Default::default(),
                scan_completed: false,
            });
        }

        Ok(ScanResult {
            root_path,
            total_size: scanned_size,
            system_reserved_bytes,
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
            cache_schema_version: 0,
            env_fingerprint: Default::default(),
            scan_completed: false,
        })
    }

    /// Recursively walk a directory in parallel using rayon::scope.
    /// Each directory enumeration is spawned as a rayon task; discovered
    /// subdirectories spawn further tasks, saturating the thread pool.
    fn walk_dir_parallel<'s>(
        scope: &rayon::Scope<'s>,
        dir: PathBuf,
        cancelled: &'s Arc<AtomicBool>,
        inaccessible_count: &'s Arc<AtomicUsize>,
        total_files: &'s Arc<AtomicUsize>,
        total_dirs: &'s Arc<AtomicUsize>,
        total_size: &'s Arc<AtomicU64>,
        dir_nodes: &'s DashMap<PathBuf, DirectoryNode>,
        dir_file_stats: &'s DashMap<PathBuf, (u64, usize)>,
        large_files: &'s Mutex<Vec<super::file_info::FileInfo>>,
        large_file_threshold: u64,
    ) {
        if cancelled.load(Ordering::Relaxed) {
            return;
        }

        let entries = match winfs::enumerate_directory(&dir, true) {
            Ok(entries) => entries,
            Err(_) => {
                inaccessible_count.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };

        // Collect subdirectories to spawn after processing entries
        let mut subdirs: Vec<PathBuf> = Vec::new();

        for entry in entries {
            if cancelled.load(Ordering::Relaxed) {
                return;
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
                subdirs.push(entry.path);
                continue;
            }

            // File entry
            let file_size = entry.size;
            total_files.fetch_add(1, Ordering::Relaxed);
            total_size.fetch_add(file_size, Ordering::Relaxed);

            if file_size >= large_file_threshold {
                let modified_at = entry
                    .modified_time
                    .and_then(|secs| chrono::DateTime::from_timestamp(secs as i64, 0))
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default();

                let file_info = super::file_info::FileInfo {
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
                };
                if let Ok(mut lf) = large_files.lock() {
                    lf.push(file_info);
                }
            }

            if let Some(parent) = entry.path.parent() {
                let mut stats = dir_file_stats
                    .entry(parent.to_path_buf())
                    .or_insert((0, 0));
                stats.0 += file_size;
                stats.1 += 1;
            }
        }

        // Spawn parallel tasks for subdirectories
        for subdir in subdirs {
            scope.spawn(move |s| {
                Self::walk_dir_parallel(
                    s,
                    subdir,
                    cancelled,
                    inaccessible_count,
                    total_files,
                    total_dirs,
                    total_size,
                    dir_nodes,
                    dir_file_stats,
                    large_files,
                    large_file_threshold,
                );
            });
        }
    }

    fn sort_directory_tree(nodes: &mut [DirectoryNode]) {
        for node in nodes.iter_mut() {
            Self::sort_directory_tree(&mut node.children);
            node.has_children = !node.children.is_empty();
            node.children.sort_by_key(|n| std::cmp::Reverse(n.size));
        }
        nodes.sort_by_key(|n| std::cmp::Reverse(n.size));
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
