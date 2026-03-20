use super::link_creator::{LinkCreator, LinkType};
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, serde::Serialize)]
pub struct MigrationProgress {
    pub status: String,
    pub copied_bytes: u64,
    pub total_bytes: u64,
    pub copied_files: usize,
    pub total_files: usize,
    pub current_file: String,
    pub progress_percent: f64,
}

pub type MigrationProgressCallback = Arc<dyn Fn(MigrationProgress) + Send + Sync>;

pub struct FileMigrator {
    link_creator: LinkCreator,
}

#[derive(Debug, Clone, Copy)]
struct CopySummary {
    copied_bytes: u64,
    copied_files: usize,
}

#[derive(Debug, Clone)]
struct CopyTask {
    source: PathBuf,
    target: PathBuf,
    expected_size: u64,
}

#[derive(Debug, Default)]
struct DirectoryCopyPlan {
    directories: Vec<PathBuf>,
    files: Vec<CopyTask>,
}

#[derive(Debug, Default)]
struct CopyProgressTracker {
    copied_bytes: AtomicU64,
    copied_files: AtomicUsize,
    should_stop: AtomicBool,
    current_file: Mutex<String>,
}

const LARGE_FILE_COPY_THRESHOLD: u64 = 128 * 1024 * 1024;
const PARALLEL_COPY_MIN_BYTES: u64 = 512 * 1024 * 1024;
const PARALLEL_COPY_MIN_FILES: usize = 128;
const DEFAULT_COPY_BUFFER_SIZE: usize = 1024 * 1024;
const LARGE_COPY_BUFFER_SIZE: usize = 8 * 1024 * 1024;
const HUGE_COPY_BUFFER_SIZE: usize = 16 * 1024 * 1024;
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(200);

impl CopyProgressTracker {
    fn set_current_file(&self, path: &Path) {
        if let Ok(mut current_file) = self.current_file.lock() {
            *current_file = path.to_string_lossy().to_string();
        }
    }

    fn add_copied_bytes(&self, path: &Path, bytes: u64) {
        self.set_current_file(path);
        self.copied_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    fn complete_file(&self, path: &Path, bytes: u64) {
        self.set_current_file(path);
        self.copied_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.copied_files.fetch_add(1, Ordering::Relaxed);
    }

    fn finish_buffered_file(&self, path: &Path) {
        self.set_current_file(path);
        self.copied_files.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> (u64, usize, String) {
        let current_file = self.current_file
            .lock()
            .map(|path| path.clone())
            .unwrap_or_default();

        (
            self.copied_bytes.load(Ordering::Relaxed),
            self.copied_files.load(Ordering::Relaxed),
            current_file,
        )
    }
}

impl FileMigrator {
    pub fn new() -> Self {
        Self { link_creator: LinkCreator::new() }
    }

    pub async fn migrate<P: AsRef<Path>>(
        &self,
        source: P,
        target_disk: P,
        link_type: LinkType,
        known_stats: Option<(u64, usize)>,
        progress: Option<MigrationProgressCallback>,
    ) -> Result<MigrationResult> {
        let start = Instant::now();
        let source = source.as_ref();
        let target_disk = target_disk.as_ref();

        let make_err = |error: String| MigrationResult {
            success: false,
            source_path: source.to_string_lossy().to_string(),
            target_path: String::new(),
            link_type: link_type.clone(),
            file_size: 0,
            duration_ms: start.elapsed().as_millis() as u64,
            migration_id: 0,
            error: Some(error),
        };

        if !source.exists() {
            return Ok(make_err("Source not found".to_string()));
        }

        let source_metadata = fs::symlink_metadata(source)?;
        if Self::is_link_entry(&source_metadata) {
            return Ok(make_err("Source path is already a link placeholder".to_string()));
        }

        let file_name = source.file_name().ok_or_else(|| anyhow!("Invalid source path"))?;
        let target_path = target_disk.join(file_name);
        let is_directory = source_metadata.is_dir();
        let source_stats = match known_stats {
            Some(stats) => stats,
            None => Self::calculate_stats(source)?,
        };
        let (file_size, total_files) = source_stats;
        let available_space = self.get_available_space(target_disk)?;

        if target_path.exists() {
            return Ok(MigrationResult {
                target_path: target_path.to_string_lossy().to_string(),
                file_size,
                ..make_err("Target path already exists".to_string())
            });
        }

        if available_space < file_size {
            return Ok(MigrationResult {
                target_path: target_path.to_string_lossy().to_string(),
                file_size,
                ..make_err("Target disk full".to_string())
            });
        }

        let make_err_with_target = |error: String| MigrationResult {
            success: false,
            source_path: source.to_string_lossy().to_string(),
            target_path: target_path.to_string_lossy().to_string(),
            link_type: link_type.clone(),
            file_size,
            duration_ms: start.elapsed().as_millis() as u64,
            migration_id: 0,
            error: Some(error),
        };

        // 带进度报告的复制
        self.emit_progress(
            &progress,
            "copying",
            0,
            file_size,
            0,
            total_files,
            source.to_string_lossy().as_ref(),
        );
        let copy_summary = match self.copy_with_progress(source, &target_path, file_size, total_files, &progress) {
            Ok(summary) => summary,
            Err(e) => return Ok(make_err_with_target(format!("Copy failed: {}", e))),
        };

        self.emit_progress(
            &progress,
            "verifying",
            file_size,
            file_size,
            total_files,
            total_files,
            target_path.to_string_lossy().as_ref(),
        );
        if copy_summary.copied_bytes != file_size || copy_summary.copied_files != total_files {
            let _ = self.cleanup_target(&target_path);
            return Ok(make_err_with_target("Verification failed".to_string()));
        }

        let backup_path = self.create_backup_path(source);
        if let Err(e) = fs::rename(source, &backup_path) {
            let _ = self.cleanup_target(&target_path);
            return Ok(make_err_with_target(format!("Backup failed: {}", e)));
        }

        if link_type == LinkType::None {
            self.emit_progress(
                &progress,
                "cleaning_up",
                file_size,
                file_size,
                total_files,
                total_files,
                source.to_string_lossy().as_ref(),
            );
            let _ = self.cleanup_backup(&backup_path);
            return Ok(MigrationResult {
                success: true,
                source_path: source.to_string_lossy().to_string(),
                target_path: target_path.to_string_lossy().to_string(),
                link_type: LinkType::None,
                file_size,
                duration_ms: start.elapsed().as_millis() as u64,
                migration_id: 0,
                error: None,
            });
        }

        self.emit_progress(
            &progress,
            "creating_link",
            file_size,
            file_size,
            total_files,
            total_files,
            source.to_string_lossy().as_ref(),
        );
        let actual_link_type = match self.link_creator.create_link(source, &target_path, link_type.clone(), is_directory) {
            Ok(lt) => lt,
            Err(e) => {
                let _ = fs::rename(&backup_path, source);
                let _ = self.cleanup_target(&target_path);
                return Ok(make_err_with_target(format!("Link creation failed: {}", e)));
            }
        };

        if !self.link_creator.verify_link(source, &target_path)? {
            let _ = self.remove_path(source);
            let _ = fs::rename(&backup_path, source);
            let _ = self.cleanup_target(&target_path);
            return Ok(MigrationResult {
                link_type: actual_link_type,
                ..make_err_with_target("Link verification failed".to_string())
            });
        }

        self.emit_progress(
            &progress,
            "cleaning_up",
            file_size,
            file_size,
            total_files,
            total_files,
            source.to_string_lossy().as_ref(),
        );
        let _ = self.cleanup_backup(&backup_path);

        Ok(MigrationResult {
            success: true,
            source_path: source.to_string_lossy().to_string(),
            target_path: target_path.to_string_lossy().to_string(),
            link_type: actual_link_type,
            file_size,
            duration_ms: start.elapsed().as_millis() as u64,
            migration_id: 0,
            error: None,
        })
    }

    fn calculate_stats(path: &Path) -> Result<(u64, usize)> {
        let metadata = fs::metadata(path)?;
        if metadata.is_file() {
            return Ok((metadata.len(), 1));
        }
        let mut total_size = 0u64;
        let mut total_count = 0usize;
        for entry in jwalk::WalkDir::new(path).skip_hidden(false).follow_links(false) {
            if let Ok(entry) = entry {
                if let Ok(m) = entry.metadata() {
                    if m.is_file() {
                        total_size += m.len();
                        total_count += 1;
                    }
                }
            }
        }
        Ok((total_size, total_count))
    }

    fn get_available_space<P: AsRef<Path>>(&self, path: P) -> Result<u64> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
            use windows::core::PCWSTR;

            let path_wide: Vec<u16> = path.as_ref().as_os_str()
                .encode_wide().chain(std::iter::once(0)).collect();
            let mut free_bytes = 0u64;
            unsafe {
                GetDiskFreeSpaceExW(
                    PCWSTR(path_wide.as_ptr()),
                    None, None,
                    Some(&mut free_bytes as *mut u64),
                )?;
            }
            Ok(free_bytes)
        }
        #[cfg(not(target_os = "windows"))]
        { Ok(u64::MAX) }
    }

    #[cfg(target_os = "windows")]
    fn is_link_entry(metadata: &fs::Metadata) -> bool {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    #[cfg(not(target_os = "windows"))]
    fn is_link_entry(metadata: &fs::Metadata) -> bool {
        metadata.file_type().is_symlink()
    }

    fn verify_copied_file(&self, expected_size: u64, target: &Path) -> Result<()> {
        let target_metadata = fs::metadata(target)?;
        if !target_metadata.is_file() {
            return Err(anyhow!("Target file missing after copy"));
        }
        if target_metadata.len() != expected_size {
            return Err(anyhow!(
                "Target file size mismatch: expected {} bytes, got {} bytes",
                expected_size,
                target_metadata.len()
            ));
        }
        Ok(())
    }

    fn source_entry_size(path: &Path, metadata: &fs::Metadata) -> Result<u64> {
        if Self::is_link_entry(metadata) {
            Ok(fs::metadata(path)?.len())
        } else {
            Ok(metadata.len())
        }
    }

    fn copy_buffer_size(file_size: u64) -> usize {
        if file_size >= 1024 * 1024 * 1024 {
            HUGE_COPY_BUFFER_SIZE
        } else if file_size >= LARGE_FILE_COPY_THRESHOLD {
            LARGE_COPY_BUFFER_SIZE
        } else {
            DEFAULT_COPY_BUFFER_SIZE
        }
    }

    #[cfg(target_os = "windows")]
    fn should_use_buffered_copy(file_size: u64) -> bool {
        file_size >= LARGE_FILE_COPY_THRESHOLD
    }

    #[cfg(not(target_os = "windows"))]
    fn should_use_buffered_copy(_file_size: u64) -> bool {
        false
    }

    #[cfg(target_os = "windows")]
    fn should_parallelize_directory_copy(total_size: u64, total_files: usize) -> bool {
        total_size >= PARALLEL_COPY_MIN_BYTES || total_files >= PARALLEL_COPY_MIN_FILES
    }

    #[cfg(not(target_os = "windows"))]
    fn should_parallelize_directory_copy(_total_size: u64, _total_files: usize) -> bool {
        false
    }

    fn build_directory_copy_plan(&self, source: &Path, target: &Path) -> Result<DirectoryCopyPlan> {
        let mut plan = DirectoryCopyPlan {
            directories: vec![target.to_path_buf()],
            files: Vec::new(),
        };
        let mut pending = vec![(source.to_path_buf(), target.to_path_buf())];

        while let Some((src_dir, dst_dir)) = pending.pop() {
            for entry in fs::read_dir(&src_dir)? {
                let entry = entry?;
                let file_type = entry.file_type()?;
                let src = entry.path();
                let dst = dst_dir.join(entry.file_name());
                let metadata = fs::symlink_metadata(&src)?;

                if Self::is_link_entry(&metadata) && file_type.is_dir() {
                    return Err(anyhow!(
                        "Directory contains nested symlink or junction: {}",
                        src.display()
                    ));
                }

                if file_type.is_dir() {
                    plan.directories.push(dst.clone());
                    pending.push((src, dst));
                    continue;
                }

                let expected_size = Self::source_entry_size(src.as_path(), &metadata)?;
                plan.files.push(CopyTask {
                    source: src,
                    target: dst,
                    expected_size,
                });
            }
        }

        plan.directories.sort_by(|a, b| {
            a.components()
                .count()
                .cmp(&b.components().count())
                .then_with(|| a.cmp(b))
        });
        plan.directories.dedup();
        plan.files.sort_by(|a, b| a.source.cmp(&b.source));

        Ok(plan)
    }

    fn prepare_directory_copy_plan(&self, plan: &DirectoryCopyPlan) -> Result<()> {
        for dir in &plan.directories {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    fn start_progress_reporter(
        progress: &Option<MigrationProgressCallback>,
        tracker: &Arc<CopyProgressTracker>,
        total_size: u64,
        total_files: usize,
    ) -> Option<std::thread::JoinHandle<()>> {
        let progress = progress.clone()?;
        let tracker = Arc::clone(tracker);

        Some(std::thread::spawn(move || {
            loop {
                std::thread::sleep(PROGRESS_EMIT_INTERVAL);

                let (copied_bytes, copied_files, current_file) = tracker.snapshot();
                Self::emit_progress_impl(
                    Some(&progress),
                    "copying",
                    copied_bytes,
                    total_size,
                    copied_files,
                    total_files,
                    &current_file,
                );

                if tracker.should_stop.load(Ordering::Relaxed) {
                    break;
                }
            }
        }))
    }

    fn finish_copy_progress(
        &self,
        progress: &Option<MigrationProgressCallback>,
        tracker: Arc<CopyProgressTracker>,
        reporter: Option<std::thread::JoinHandle<()>>,
        total_size: u64,
        total_files: usize,
        fallback_path: &Path,
    ) -> CopySummary {
        tracker.should_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = reporter {
            let _ = handle.join();
        }

        let (copied_bytes, copied_files, current_file) = tracker.snapshot();
        let current_file = if current_file.is_empty() {
            fallback_path.to_string_lossy().to_string()
        } else {
            current_file
        };

        self.emit_progress(
            progress,
            "copying",
            copied_bytes,
            total_size,
            copied_files,
            total_files,
            &current_file,
        );

        CopySummary {
            copied_bytes,
            copied_files,
        }
    }

    fn stop_progress_reporter(
        tracker: &Arc<CopyProgressTracker>,
        reporter: Option<std::thread::JoinHandle<()>>,
    ) {
        tracker.should_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = reporter {
            let _ = handle.join();
        }
    }

    fn copy_file_buffered(
        &self,
        source: &Path,
        target: &Path,
        expected_size: u64,
        progress: Option<&CopyProgressTracker>,
    ) -> Result<u64> {
        #[cfg(target_os = "windows")]
        let mut input = {
            use std::os::windows::fs::OpenOptionsExt;
            use windows::Win32::Storage::FileSystem::FILE_FLAG_SEQUENTIAL_SCAN;

            fs::OpenOptions::new()
                .read(true)
                .custom_flags(FILE_FLAG_SEQUENTIAL_SCAN.0)
                .open(source)?
        };

        #[cfg(not(target_os = "windows"))]
        let mut input = fs::File::open(source)?;

        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;

        let mut buffer = vec![0u8; Self::copy_buffer_size(expected_size)];
        let mut copied = 0u64;

        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }

            output.write_all(&buffer[..read])?;
            copied += read as u64;

            if let Some(progress) = progress {
                progress.add_copied_bytes(source, read as u64);
            }
        }

        output.flush()?;

        if let Ok(source_metadata) = fs::metadata(source) {
            let _ = fs::set_permissions(target, source_metadata.permissions());
        }

        Ok(copied)
    }

    #[cfg(windows)]
    fn copy_file_via_copyfileex(&self, source: &Path, target: &Path) -> Result<u64> {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Storage::FileSystem::CopyFileExW;
        use windows::core::PCWSTR;

        let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let target_wide: Vec<u16> = target.as_os_str().encode_wide().chain(std::iter::once(0)).collect();

        unsafe {
            CopyFileExW(
                PCWSTR(source_wide.as_ptr()),
                PCWSTR(target_wide.as_ptr()),
                None,
                None,
                None,
                0,
            )?;
        }

        Ok(fs::metadata(target)?.len())
    }

    #[cfg(windows)]
    fn copy_file_optimized(
        &self,
        source: &Path,
        target: &Path,
        expected_size: u64,
        progress: Option<&CopyProgressTracker>,
    ) -> Result<u64> {
        let use_buffered_copy = Self::should_use_buffered_copy(expected_size);
        let copied = if use_buffered_copy {
            self.copy_file_buffered(source, target, expected_size, progress)?
        } else {
            self.copy_file_via_copyfileex(source, target)?
        };

        self.verify_copied_file(expected_size, target)?;

        if let Some(progress) = progress {
            if use_buffered_copy {
                progress.finish_buffered_file(source);
            } else {
                progress.complete_file(source, copied);
            }
        }

        Ok(copied)
    }

    #[cfg(not(windows))]
    fn copy_file_optimized(
        &self,
        source: &Path,
        target: &Path,
        expected_size: u64,
        progress: Option<&CopyProgressTracker>,
    ) -> Result<u64> {
        let copied = fs::copy(source, target)?;
        self.verify_copied_file(expected_size, target)?;

        if let Some(progress) = progress {
            progress.complete_file(source, copied);
        }

        Ok(copied)
    }

    fn copy_directory_serial(
        &self,
        plan: &DirectoryCopyPlan,
        total_size: u64,
        total_files: usize,
        progress: &Option<MigrationProgressCallback>,
        fallback_path: &Path,
    ) -> Result<CopySummary> {
        let tracker = Arc::new(CopyProgressTracker::default());
        let reporter = Self::start_progress_reporter(progress, &tracker, total_size, total_files);

        let copy_result = (|| -> Result<()> {
            for task in &plan.files {
                self.copy_file_optimized(&task.source, &task.target, task.expected_size, Some(tracker.as_ref()))?;
            }
            Ok(())
        })();

        if let Err(err) = copy_result {
            Self::stop_progress_reporter(&tracker, reporter);
            return Err(err);
        }

        Ok(self.finish_copy_progress(progress, tracker, reporter, total_size, total_files, fallback_path))
    }

    fn copy_directory_parallel(
        &self,
        plan: &DirectoryCopyPlan,
        total_size: u64,
        total_files: usize,
        progress: &Option<MigrationProgressCallback>,
        fallback_path: &Path,
    ) -> Result<CopySummary> {
        let tracker = Arc::new(CopyProgressTracker::default());
        let reporter = Self::start_progress_reporter(progress, &tracker, total_size, total_files);

        let copy_result = plan.files.par_iter().try_for_each(|task| -> Result<()> {
            self.copy_file_optimized(&task.source, &task.target, task.expected_size, Some(tracker.as_ref()))?;
            Ok(())
        });

        if let Err(err) = copy_result {
            Self::stop_progress_reporter(&tracker, reporter);
            return Err(err);
        }

        Ok(self.finish_copy_progress(progress, tracker, reporter, total_size, total_files, fallback_path))
    }

    /// 带进度报告的复制
    fn copy_with_progress(
        &self,
        source: &Path,
        target: &Path,
        total_size: u64,
        total_files: usize,
        progress: &Option<MigrationProgressCallback>,
    ) -> Result<CopySummary> {
        if source.is_file() {
            let tracker = Arc::new(CopyProgressTracker::default());
            let tracked_files = total_files.max(1);
            let reporter = Self::start_progress_reporter(progress, &tracker, total_size, tracked_files);

            if let Err(err) = self.copy_file_optimized(source, target, total_size, Some(tracker.as_ref())) {
                Self::stop_progress_reporter(&tracker, reporter);
                return Err(err);
            }
            return Ok(self.finish_copy_progress(progress, tracker, reporter, total_size, tracked_files, source));
        }

        let plan = self.build_directory_copy_plan(source, target)?;
        self.prepare_directory_copy_plan(&plan)?;

        if Self::should_parallelize_directory_copy(total_size, total_files) {
            self.copy_directory_parallel(&plan, total_size, total_files, progress, target)
        } else {
            self.copy_directory_serial(&plan, total_size, total_files, progress, target)
        }
    }

    fn create_backup_path(&self, path: &Path) -> PathBuf {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();

        for attempt in 0.. {
            let suffix = if attempt == 0 {
                ".backup_temp".to_string()
            } else {
                format!(".backup_temp.{attempt}")
            };
            let candidate = parent.join(format!("{file_name}{suffix}"));
            if !candidate.exists() {
                return candidate;
            }
        }

        unreachable!()
    }

    fn cleanup_target(&self, path: &Path) -> Result<()> {
        if path.exists() {
            self.remove_path(path)?;
        }
        Ok(())
    }

    fn cleanup_backup(&self, path: &Path) -> Result<()> {
        self.cleanup_target(path)
    }

    fn emit_progress(
        &self,
        progress: &Option<MigrationProgressCallback>,
        status: &str,
        copied_bytes: u64,
        total_bytes: u64,
        copied_files: usize,
        total_files: usize,
        current_file: &str,
    ) {
        Self::emit_progress_impl(
            progress.as_ref(),
            status,
            copied_bytes,
            total_bytes,
            copied_files,
            total_files,
            current_file,
        );
    }

    fn emit_progress_impl(
        progress: Option<&MigrationProgressCallback>,
        status: &str,
        copied_bytes: u64,
        total_bytes: u64,
        copied_files: usize,
        total_files: usize,
        current_file: &str,
    ) {
        if let Some(progress) = progress {
            let pct = if total_bytes > 0 {
                copied_bytes as f64 / total_bytes as f64 * 100.0
            } else {
                100.0
            };

            progress(MigrationProgress {
                status: status.to_string(),
                copied_bytes,
                total_bytes,
                copied_files,
                total_files,
                current_file: current_file.to_string(),
                progress_percent: pct.clamp(0.0, 100.0),
            });
        }
    }

    fn remove_path(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        let metadata = fs::symlink_metadata(path)?;

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::MetadataExt;

            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
            let is_link = metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
            if is_link {
                if fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false) {
                    fs::remove_dir(path)?;
                } else {
                    fs::remove_file(path)?;
                }
                return Ok(());
            }
        }

        #[cfg(not(target_os = "windows"))]
        if metadata.file_type().is_symlink() {
            if fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false) {
                fs::remove_dir(path)?;
            } else {
                fs::remove_file(path)?;
            }
            return Ok(());
        }

        if metadata.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }

        Ok(())
    }
}

#[derive(Debug, serde::Serialize)]
pub struct MigrationResult {
    pub success: bool,
    pub source_path: String,
    pub target_path: String,
    pub link_type: LinkType,
    pub file_size: u64,
    pub duration_ms: u64,
    pub migration_id: i64,
    pub error: Option<String>,
}

impl FileMigrator {
    pub async fn rollback(&self, source: &Path, target: &Path) -> Result<RollbackResult> {
        let start = Instant::now();

        let make_err = |error: String| RollbackResult {
            success: false,
            source_path: source.to_string_lossy().to_string(),
            target_path: target.to_string_lossy().to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some(error),
        };

        if !target.exists() {
            return Ok(make_err("Target not found".to_string()));
        }

        if source.exists() {
            if let Err(e) = self.remove_path(source) {
                return Ok(make_err(format!("Failed to remove link: {}", e)));
            }
        }

        let (target_size, target_files) = Self::calculate_stats(target)?;
        if let Err(e) = self.copy_with_progress(target, source, target_size, target_files, &None) {
            return Ok(make_err(format!("Failed to restore: {}", e)));
        }

        if let Err(e) = self.cleanup_target(target) {
            return Ok(make_err(format!("Failed to cleanup target: {}", e)));
        }

        Ok(RollbackResult {
            success: true,
            source_path: source.to_string_lossy().to_string(),
            target_path: target.to_string_lossy().to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
        })
    }
}

#[derive(Debug, serde::Serialize)]
pub struct RollbackResult {
    pub success: bool,
    pub source_path: String,
    pub target_path: String,
    pub duration_ms: u64,
    pub error: Option<String>,
}
