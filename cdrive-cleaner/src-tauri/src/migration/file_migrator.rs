use super::link_creator::{LinkCreator, LinkType};
use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

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

pub struct FileMigrator {
    link_creator: LinkCreator,
}

#[derive(Debug, Clone, Copy)]
struct CopySummary {
    copied_bytes: u64,
    copied_files: usize,
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
        app: Option<AppHandle>,
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
            &app,
            "copying",
            0,
            file_size,
            0,
            total_files,
            source.to_string_lossy().as_ref(),
        );
        let copy_summary = match self.copy_with_progress(source, &target_path, file_size, total_files, &app) {
            Ok(summary) => summary,
            Err(e) => return Ok(make_err_with_target(format!("Copy failed: {}", e))),
        };

        self.emit_progress(
            &app,
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
                &app,
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
            &app,
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
            &app,
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

    #[cfg(windows)]
    fn copy_file_optimized(&self, source: &Path, target: &Path) -> Result<u64> {
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

    #[cfg(not(windows))]
    fn copy_file_optimized(&self, source: &Path, target: &Path) -> Result<u64> {
        Ok(fs::copy(source, target)?)
    }

    /// 带进度报告的复制
    fn copy_with_progress(
        &self,
        source: &Path,
        target: &Path,
        total_size: u64,
        total_files: usize,
        app: &Option<AppHandle>,
    ) -> Result<CopySummary> {
        if source.is_file() {
            let copied = self.copy_file_optimized(source, target)?;
            self.verify_copied_file(total_size, target)?;
            self.emit_progress(
                app,
                "copying",
                copied,
                total_size,
                total_files.min(1),
                total_files.max(1),
                source.to_string_lossy().as_ref(),
            );
            return Ok(CopySummary {
                copied_bytes: copied,
                copied_files: 1,
            });
        }

        let mut copied_bytes = 0u64;
        let mut copied_files = 0usize;
        let mut last_emit = Instant::now();

        self.copy_dir_with_progress(source, target, total_size, total_files, &mut copied_bytes, &mut copied_files, &mut last_emit, app)?;
        self.emit_progress(
            app,
            "copying",
            copied_bytes,
            total_size,
            copied_files,
            total_files,
            target.to_string_lossy().as_ref(),
        );
        Ok(CopySummary {
            copied_bytes,
            copied_files,
        })
    }

    fn copy_dir_with_progress(
        &self, source: &Path, target: &Path,
        total_size: u64, total_files: usize,
        copied_bytes: &mut u64, copied_files: &mut usize,
        last_emit: &mut Instant, app: &Option<AppHandle>,
    ) -> Result<()> {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let src = entry.path();
            let dst = target.join(entry.file_name());
            let metadata = fs::symlink_metadata(&src)?;

            if Self::is_link_entry(&metadata) && file_type.is_dir() {
                return Err(anyhow!(
                    "Directory contains nested symlink or junction: {}",
                    src.display()
                ));
            }

            if file_type.is_dir() {
                self.copy_dir_with_progress(&src, &dst, total_size, total_files, copied_bytes, copied_files, last_emit, app)?;
            } else {
                let expected_size = metadata.len();
                self.copy_file_optimized(&src, &dst)?;
                self.verify_copied_file(expected_size, &dst)?;
                *copied_bytes += expected_size;
                *copied_files += 1;

                // 每 200ms 发送一次进度，避免事件风暴
                if last_emit.elapsed().as_millis() >= 200 {
                    self.emit_progress(
                        app,
                        "copying",
                        *copied_bytes,
                        total_size,
                        *copied_files,
                        total_files,
                        src.to_string_lossy().as_ref(),
                    );
                    *last_emit = Instant::now();
                }
            }
        }
        Ok(())
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
        app: &Option<AppHandle>,
        status: &str,
        copied_bytes: u64,
        total_bytes: u64,
        copied_files: usize,
        total_files: usize,
        current_file: &str,
    ) {
        if let Some(app) = app {
            let pct = if total_bytes > 0 {
                copied_bytes as f64 / total_bytes as f64 * 100.0
            } else {
                100.0
            };

            let _ = app.emit("migration-progress", MigrationProgress {
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
