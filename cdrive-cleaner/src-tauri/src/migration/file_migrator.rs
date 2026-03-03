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

        println!("\n=== 开始迁移 ===");
        println!("源路径: {}", source.display());
        println!("目标磁盘: {}", target_disk.display());
        println!("链接类型: {:?}", link_type);

        if !source.exists() {
            println!("❌ 错误: 源路径不存在");
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
        println!("✓ 源路径存在");

        let file_name = source.file_name()
            .ok_or_else(|| anyhow!("Invalid source path"))?;
        let target_path = target_disk.join(file_name);
        println!("目标路径: {}", target_path.display());

        let is_directory = source.is_dir();
        println!("类型: {}", if is_directory { "目录" } else { "文件" });
        
        let file_size = self.calculate_size(source)?;
        println!("文件大小: {} 字节", file_size);

        let available_space = self.get_available_space(target_disk)?;
        println!("目标磁盘可用空间: {} 字节", available_space);
        
        if available_space < file_size {
            println!("❌ 错误: 目标磁盘空间不足");
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
        println!("✓ 目标磁盘空间充足");

        println!("开始复制文件...");
        match self.copy_to_target(source, &target_path) {
            Ok(_) => println!("✓ 文件复制完成"),
            Err(e) => {
                println!("❌ 复制失败: {}", e);
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

        println!("验证复制完整性...");
        if !self.verify_copy(source, &target_path)? {
            println!("❌ 验证失败");
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
        println!("✓ 验证通过");

        let backup_path = self.create_backup_path(source);
        println!("创建备份: {}", backup_path.display());
        
        if let Err(e) = fs::rename(source, &backup_path) {
            println!("❌ 备份失败: {}", e);
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
        println!("✓ 备份完成");

        println!("创建链接...");
        let actual_link_type = match self.link_creator.create_link(source, &target_path, link_type.clone(), is_directory) {
            Ok(lt) => {
                println!("✓ 链接创建成功，类型: {:?}", lt);
                lt
            },
            Err(e) => {
                println!("❌ 链接创建失败: {}", e);
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

        println!("验证链接...");
        if !self.link_creator.verify_link(source, &target_path)? {
            println!("❌ 链接验证失败");
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
        println!("✓ 链接验证通过");

        println!("清理备份...");
        let _ = self.cleanup_backup(&backup_path);
        println!("✓ 备份清理完成");

        println!("=== 迁移成功 ===\n");

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
