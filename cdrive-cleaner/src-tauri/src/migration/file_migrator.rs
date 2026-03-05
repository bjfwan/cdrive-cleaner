use super::link_creator::{LinkCreator, LinkType};
use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

pub struct FileMigrator {
    link_creator: LinkCreator,
}

impl FileMigrator {
    pub fn new() -> Self {
        Self {
            link_creator: LinkCreator::new(),
        }
    }

    pub async fn migrate<P: AsRef<Path>>(
        &self,
        source: P,
        target_disk: P,
        link_type: LinkType,
    ) -> Result<MigrationResult> {
        let start = Instant::now();
        let source = source.as_ref();
        let target_disk = target_disk.as_ref();

        if !source.exists() {
            return Ok(MigrationResult {
                success: false,
                source_path: source.to_string_lossy().to_string(),
                target_path: String::new(),
                link_type: link_type.clone(),
                file_size: 0,
                duration_ms: start.elapsed().as_millis() as u64,
                migration_id: 0,
                error: Some("Source not found".to_string()),
            });
        }

        let file_name = source.file_name()
            .ok_or_else(|| anyhow!("Invalid source path"))?;
        let target_path = target_disk.join(file_name);

        let is_directory = source.is_dir();
        let file_size = self.calculate_size(source)?;
        let available_space = self.get_available_space(target_disk)?;
        
        if available_space < file_size {
            return Ok(MigrationResult {
                success: false,
                source_path: source.to_string_lossy().to_string(),
                target_path: target_path.to_string_lossy().to_string(),
                link_type: link_type.clone(),
                file_size,
                duration_ms: start.elapsed().as_millis() as u64,
                migration_id: 0,
                error: Some("Target disk full".to_string()),
            });
        }

        match self.copy_to_target(source, &target_path) {
            Ok(_) => {},
            Err(e) => {
                return Ok(MigrationResult {
                    success: false,
                    source_path: source.to_string_lossy().to_string(),
                    target_path: target_path.to_string_lossy().to_string(),
                    link_type: link_type.clone(),
                    file_size,
                    duration_ms: start.elapsed().as_millis() as u64,
                    migration_id: 0,
                    error: Some(format!("Copy failed: {}", e)),
                });
            }
        }

        if !self.verify_copy(source, &target_path)? {
            let _ = self.cleanup_target(&target_path);
            return Ok(MigrationResult {
                success: false,
                source_path: source.to_string_lossy().to_string(),
                target_path: target_path.to_string_lossy().to_string(),
                link_type: link_type.clone(),
                file_size,
                duration_ms: start.elapsed().as_millis() as u64,
                migration_id: 0,
                error: Some("Verification failed".to_string()),
            });
        }

        let backup_path = self.create_backup_path(source);
        
        if let Err(e) = fs::rename(source, &backup_path) {
            let _ = self.cleanup_target(&target_path);
            return Ok(MigrationResult {
                success: false,
                source_path: source.to_string_lossy().to_string(),
                target_path: target_path.to_string_lossy().to_string(),
                link_type: link_type.clone(),
                file_size,
                duration_ms: start.elapsed().as_millis() as u64,
                migration_id: 0,
                error: Some(format!("Backup failed: {}", e)),
            });
        }

        // 如果用户选择不创建链接，直接返回成功
        if link_type == LinkType::None {
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

        let actual_link_type = match self.link_creator.create_link(source, &target_path, link_type.clone(), is_directory) {
            Ok(lt) => lt,
            Err(e) => {
                let _ = fs::rename(&backup_path, source);
                let _ = self.cleanup_target(&target_path);
                return Ok(MigrationResult {
                    success: false,
                    source_path: source.to_string_lossy().to_string(),
                    target_path: target_path.to_string_lossy().to_string(),
                    link_type: link_type.clone(),
                    file_size,
                    duration_ms: start.elapsed().as_millis() as u64,
                    migration_id: 0,
                    error: Some(format!("Link creation failed: {}", e)),
                });
            }
        };

        if !self.link_creator.verify_link(source, &target_path)? {
            let _ = fs::remove_file(source).or_else(|_| fs::remove_dir_all(source));
            let _ = fs::rename(&backup_path, source);
            let _ = self.cleanup_target(&target_path);
            return Ok(MigrationResult {
                success: false,
                source_path: source.to_string_lossy().to_string(),
                target_path: target_path.to_string_lossy().to_string(),
                link_type: actual_link_type.clone(),
                file_size,
                duration_ms: start.elapsed().as_millis() as u64,
                migration_id: 0,
                error: Some("Link verification failed".to_string()),
            });
        }

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

    fn calculate_size<P: AsRef<Path>>(&self, path: P) -> Result<u64> {
        let path = path.as_ref();
        let metadata = fs::metadata(path)?;
        
        if metadata.is_file() {
            Ok(metadata.len())
        } else {
            let mut total = 0u64;
            for entry in WalkDir::new(path) {
                if let Ok(entry) = entry {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_file() {
                            total += metadata.len();
                        }
                    }
                }
            }
            Ok(total)
        }
    }

    fn get_available_space<P: AsRef<Path>>(&self, path: P) -> Result<u64> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
            use windows::core::PCWSTR;

            let path_wide: Vec<u16> = path.as_ref()
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let mut free_bytes = 0u64;
            unsafe {
                GetDiskFreeSpaceExW(
                    PCWSTR(path_wide.as_ptr()),
                    None,
                    None,
                    Some(&mut free_bytes as *mut u64),
                )?;
            }
            Ok(free_bytes)
        }

        #[cfg(not(target_os = "windows"))]
        {
            Ok(u64::MAX)
        }
    }

    fn copy_to_target<P: AsRef<Path>>(&self, source: P, target: P) -> Result<()> {
        let source = source.as_ref();
        let target = target.as_ref();

        if source.is_file() {
            fs::copy(source, target)?;
        } else {
            self.copy_dir_recursive(source, target)?;
        }
        Ok(())
    }

    fn copy_dir_recursive<P: AsRef<Path>>(&self, source: P, target: P) -> Result<()> {
        let source = source.as_ref();
        let target = target.as_ref();

        fs::create_dir_all(target)?;

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let source_path = entry.path();
            let target_path = target.join(entry.file_name());

            if file_type.is_dir() {
                self.copy_dir_recursive(&source_path, &target_path)?;
            } else {
                fs::copy(&source_path, &target_path)?;
            }
        }
        Ok(())
    }

    fn verify_copy<P: AsRef<Path>>(&self, source: P, target: P) -> Result<bool> {
        let source = source.as_ref();
        let target = target.as_ref();

        let source_size = self.calculate_size(source)?;
        let target_size = self.calculate_size(target)?;

        Ok(source_size == target_size)
    }

    fn create_backup_path<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let path = path.as_ref();
        let mut backup = path.to_path_buf();
        backup.set_extension("backup_temp");
        backup
    }

    fn cleanup_target<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        if path.is_file() {
            fs::remove_file(path)?;
        } else if path.is_dir() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }

    fn cleanup_backup<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        self.cleanup_target(path)
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
    pub async fn rollback<P: AsRef<Path>>(
        &self,
        source: P,
        target: P,
    ) -> Result<RollbackResult> {
        let start = Instant::now();
        let source = source.as_ref();
        let target = target.as_ref();

        if !target.exists() {
            return Ok(RollbackResult {
                success: false,
                source_path: source.to_string_lossy().to_string(),
                target_path: target.to_string_lossy().to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some("Target not found".to_string()),
            });
        }

        if source.exists() {
            if let Err(e) = fs::remove_file(source).or_else(|_| fs::remove_dir_all(source)) {
                return Ok(RollbackResult {
                    success: false,
                    source_path: source.to_string_lossy().to_string(),
                    target_path: target.to_string_lossy().to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(format!("Failed to remove link: {}", e)),
                });
            }
        }

        if let Err(e) = self.copy_to_target(target, source) {
            return Ok(RollbackResult {
                success: false,
                source_path: source.to_string_lossy().to_string(),
                target_path: target.to_string_lossy().to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(format!("Failed to restore: {}", e)),
            });
        }

        if let Err(e) = self.cleanup_target(target) {
            return Ok(RollbackResult {
                success: false,
                source_path: source.to_string_lossy().to_string(),
                target_path: target.to_string_lossy().to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(format!("Failed to cleanup target: {}", e)),
            });
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
