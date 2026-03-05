use crate::scanner::{DiskScanner, file_info::{ScanResult, FileInfo}};
use crate::migration::{FileMigrator, LinkType, file_migrator::MigrationResult};
use tauri::{AppHandle, Manager};

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

#[tauri::command]
pub async fn scan_disk(path: String, app: AppHandle, scanner: tauri::State<'_, DiskScanner>) -> Result<ScanResult, String> {
    scanner.scan(&path, app).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_disk_deep(path: String, app: AppHandle, estimated_files: Option<usize>, scanner: tauri::State<'_, DiskScanner>) -> Result<ScanResult, String> {
    let estimated = estimated_files.unwrap_or(800000); // 默认估算值
    scanner.scan_deep(&path, app, estimated).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_directory_files(path: String) -> Result<Vec<FileInfo>, String> {
    use std::fs;
    use std::path::Path;
    
    let dir_path = Path::new(&path);
    if !dir_path.exists() || !dir_path.is_dir() {
        return Err("路径不存在或不是目录".to_string());
    }

    let mut files = Vec::new();
    
    match fs::read_dir(dir_path) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(metadata) = entry.metadata() {
                        let file_name = path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Unknown")
                            .to_string();
                        
                        let extension = path.extension()
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
                        
                        files.push(FileInfo {
                            path: path.to_string_lossy().to_string(),
                            name: file_name,
                            size: metadata.len(),
                            extension,
                            modified_at,
                            is_readonly,
                        });
                    }
                }
            }
        }
        Err(e) => return Err(format!("无法读取目录: {}", e)),
    }
    
    files.sort_by(|a, b| b.size.cmp(&a.size));
    
    Ok(files)
}

#[tauri::command]
pub async fn migrate_file(
    source: String,
    target_disk: String,
    link_type: Option<LinkType>,
    app: AppHandle,
) -> Result<MigrationResult, String> {
    use crate::database::MigrationDb;
    
    let migrator = FileMigrator::new();
    let link_type = link_type.unwrap_or(LinkType::Auto);
    let mut result = migrator.migrate(&source, &target_disk, link_type)
        .await
        .map_err(|e| e.to_string())?;
    
    if result.success {
        let app_dir = app.path().app_data_dir()
            .map_err(|e| e.to_string())?;
        let db_path = app_dir.join("migrations.db");
        
        std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
        
        let db = MigrationDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
        let link_type_str = match result.link_type {
            LinkType::Junction => "Junction",
            LinkType::Symlink => "Symlink",
            LinkType::Hardlink => "Hardlink",
            LinkType::Auto => "Auto",
            LinkType::None => "None",
        };
        
        let migration_id = db.insert_migration(
            &result.source_path,
            &result.target_path,
            link_type_str,
            result.file_size,
        ).map_err(|e| e.to_string())?;
        
        result.migration_id = migration_id;
    }
    
    Ok(result)
}

#[tauri::command]
pub async fn get_disk_info() -> Result<Vec<DiskInfo>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Storage::FileSystem::{
            GetDiskFreeSpaceExW, GetVolumeInformationW, GetLogicalDrives
        };
        use windows::core::PCWSTR;

        let mut disks = Vec::new();
        
        unsafe {
            let drives = GetLogicalDrives();
            
            for i in 0..26 {
                if (drives & (1 << i)) != 0 {
                    let drive_letter = format!("{}:", (b'A' + i) as char);
                    let drive_path = format!("{}\\", drive_letter);
                    
                    let path_wide: Vec<u16> = std::ffi::OsStr::new(&drive_path)
                        .encode_wide()
                        .chain(std::iter::once(0))
                        .collect();

                    let mut total_bytes = 0u64;
                    let mut free_bytes = 0u64;
                    
                    if GetDiskFreeSpaceExW(
                        PCWSTR(path_wide.as_ptr()),
                        None,
                        Some(&mut total_bytes as *mut u64),
                        Some(&mut free_bytes as *mut u64),
                    ).is_ok() {
                        let mut volume_name = vec![0u16; 256];
                        let mut file_system = vec![0u16; 256];
                        
                        let _ = GetVolumeInformationW(
                            PCWSTR(path_wide.as_ptr()),
                            Some(&mut volume_name),
                            None,
                            None,
                            None,
                            Some(&mut file_system),
                        );

                        let label = String::from_utf16_lossy(&volume_name)
                            .trim_end_matches('\0')
                            .to_string();
                        let fs = String::from_utf16_lossy(&file_system)
                            .trim_end_matches('\0')
                            .to_string();

                        let used_space = total_bytes.saturating_sub(free_bytes);
                        let usage_percent = if total_bytes > 0 {
                            (used_space as f64 / total_bytes as f64) * 100.0
                        } else {
                            0.0
                        };

                        disks.push(DiskInfo {
                            drive_letter,
                            label: if label.is_empty() { "Local Disk".to_string() } else { label },
                            file_system: if fs.is_empty() { "Unknown".to_string() } else { fs },
                            total_space: total_bytes,
                            free_space: free_bytes,
                            used_space,
                            usage_percent,
                        });
                    }
                }
            }
        }
        
        Ok(disks)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(vec![])
    }
}

#[derive(serde::Serialize)]
pub struct DiskInfo {
    pub drive_letter: String,
    pub label: String,
    pub file_system: String,
    pub total_space: u64,
    pub free_space: u64,
    pub used_space: u64,
    pub usage_percent: f64,
}

#[tauri::command]
pub async fn analyze_migration_safety(path: String, size: u64) -> Result<crate::safety::MigrationSafety, String> {
    use std::path::Path;
    let path_obj = Path::new(&path);
    crate::safety::analyze_migration_safety(path_obj, size)
}

#[tauri::command]
pub async fn get_migration_history(app: AppHandle) -> Result<Vec<crate::database::migrations::MigrationRecord>, String> {
    use crate::database::MigrationDb;
    
    let app_dir = app.path().app_data_dir()
        .map_err(|e| e.to_string())?;
    let db_path = app_dir.join("migrations.db");
    
    std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    
    let db = MigrationDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    db.get_all_migrations().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_migration_stats(app: AppHandle) -> Result<crate::database::migrations::MigrationStats, String> {
    use crate::database::MigrationDb;
    
    let app_dir = app.path().app_data_dir()
        .map_err(|e| e.to_string())?;
    let db_path = app_dir.join("migrations.db");
    
    std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    
    let db = MigrationDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    db.get_stats().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rollback_migration(migration_id: i64, app: AppHandle) -> Result<crate::migration::file_migrator::RollbackResult, String> {
    use crate::database::MigrationDb;
    use crate::migration::FileMigrator;
    
    let app_dir = app.path().app_data_dir()
        .map_err(|e| e.to_string())?;
    let db_path = app_dir.join("migrations.db");
    
    let db = MigrationDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    
    let record = db.get_migration_by_id(migration_id)
        .map_err(|e| e.to_string())?
        .ok_or("Migration record not found")?;
    
    if record.status != "active" {
        return Err("Migration is not active".to_string());
    }
    
    let migrator = FileMigrator::new();
    let result = migrator.rollback(&record.source_path, &record.target_path)
        .await
        .map_err(|e| e.to_string())?;
    
    if result.success {
        db.update_status(migration_id, "rolled_back")
            .map_err(|e| e.to_string())?;
    }
    
    Ok(result)
}


#[tauri::command]
pub async fn save_scan_cache(
    disk_path: String,
    scan_type: String,
    result: ScanResult,
    app: AppHandle,
) -> Result<(), String> {
    use crate::database::ScanCacheDb;
    
    let app_dir = app.path().app_data_dir()
        .map_err(|e| e.to_string())?;
    let db_path = app_dir.join("scan_cache.db");
    
    std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    
    let db = ScanCacheDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    let result_json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
    
    db.save_scan_result(
        &disk_path,
        &scan_type,
        &result_json,
        result.total_files as i64,
        result.total_size as i64,
    ).map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub async fn get_scan_cache(
    disk_path: String,
    scan_type: String,
    app: AppHandle,
) -> Result<Option<ScanResult>, String> {
    use crate::database::ScanCacheDb;
    
    let app_dir = app.path().app_data_dir()
        .map_err(|e| e.to_string())?;
    let db_path = app_dir.join("scan_cache.db");
    
    if !db_path.exists() {
        return Ok(None);
    }
    
    let db = ScanCacheDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    
    if let Some(cached) = db.get_scan_result(&disk_path, &scan_type).map_err(|e| e.to_string())? {
        let result: ScanResult = serde_json::from_str(&cached.result_json)
            .map_err(|e| e.to_string())?;
        Ok(Some(result))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn clear_scan_cache(app: AppHandle) -> Result<(), String> {
    use crate::database::ScanCacheDb;
    
    let app_dir = app.path().app_data_dir()
        .map_err(|e| e.to_string())?;
    let db_path = app_dir.join("scan_cache.db");
    
    if !db_path.exists() {
        return Ok(());
    }
    
    let db = ScanCacheDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    db.clear_all().map_err(|e| e.to_string())?;
    
    Ok(())
}
