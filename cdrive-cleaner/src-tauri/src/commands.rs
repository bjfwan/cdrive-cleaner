use crate::scanner::{DiskScanner, file_info::{ScanResult, FileInfo}};
use crate::migration::{FileMigrator, LinkType, file_migrator::MigrationResult};
use crate::utils;
use tauri::AppHandle;
use rusqlite;

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
pub async fn scan_disk_incremental(
    path: String,
    app: AppHandle,
    scanner: tauri::State<'_, DiskScanner>
) -> Result<ScanResult, String> {
    use crate::database::ScanCacheDb;
    use crate::scanner::incremental;
    
    let scan_path = std::path::Path::new(&path);
    
    let db_path = utils::get_scan_cache_db_path().map_err(|e| e.to_string())?;
    
    if !db_path.exists() {
        return scanner.scan(scan_path, app).await.map_err(|e| e.to_string());
    }
    
    let db = ScanCacheDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    
    let cached = match db.get_scan_result(&path, "quick").map_err(|e| e.to_string())? {
        Some(c) => c,
        None => return scanner.scan(scan_path, app).await.map_err(|e| e.to_string()),
    };
    
    let cached_result: ScanResult = serde_json::from_str(&cached.result_json)
        .map_err(|e| e.to_string())?;
    
    let result = incremental::scan_incremental(scan_path, cached_result, app.clone())
        .await
        .map_err(|e| e.to_string())?;
    
    let result_json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
    db.save_scan_result(
        &path,
        "quick",
        &result_json,
        result.total_files as i64,
        result.total_size as i64,
    ).map_err(|e| e.to_string())?;
    
    Ok(result)
}

#[tauri::command]
pub async fn scan_disk_deep(path: String, app: AppHandle, estimated_files: Option<usize>, scanner: tauri::State<'_, DiskScanner>) -> Result<ScanResult, String> {
    use crate::database::ScanCacheDb;
    use crate::scanner::incremental;
    
    let scan_path = std::path::Path::new(&path);
    
    // 尝试使用增量扫描
    let db_path = utils::get_scan_cache_db_path().map_err(|e| e.to_string())?;
    
    if db_path.exists() {
        let db = ScanCacheDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
        
        if let Some(cached) = db.get_scan_result(&path, "deep").map_err(|e| e.to_string())? {
            let cached_result: ScanResult = serde_json::from_str(&cached.result_json)
                .map_err(|e| e.to_string())?;
            
            let result = incremental::scan_incremental(scan_path, cached_result, app.clone())
                .await
                .map_err(|e| e.to_string())?;
            
            // 保存更新后的缓存
            let result_json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
            db.save_scan_result(
                &path,
                "deep",
                &result_json,
                result.total_files as i64,
                result.total_size as i64,
            ).map_err(|e| e.to_string())?;
            
            return Ok(result);
        }
    }
    
    // 没有缓存，执行完整深度扫描
    let estimated = estimated_files.unwrap_or(800000);
    let result = scanner.scan_deep(&path, app.clone(), estimated).await.map_err(|e| e.to_string())?;
    
    // 保存深度扫描缓存
    let db_path = utils::get_scan_cache_db_path().map_err(|e| e.to_string())?;
    let db = ScanCacheDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    let result_json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
    db.save_scan_result(
        &path,
        "deep",
        &result_json,
        result.total_files as i64,
        result.total_size as i64,
    ).map_err(|e| e.to_string())?;
    
    Ok(result)
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
    _app: AppHandle,
) -> Result<MigrationResult, String> {
    use crate::database::MigrationDb;
    
    let migrator = FileMigrator::new();
    let link_type = link_type.unwrap_or(LinkType::Auto);
    let mut result = migrator.migrate(&source, &target_disk, link_type)
        .await
        .map_err(|e| e.to_string())?;
    
    if result.success {
        let db_path = utils::get_migrations_db_path().map_err(|e| e.to_string())?;
        
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
pub async fn analyze_migration_safety(path: String, size: u64, app: AppHandle) -> Result<crate::safety::MigrationSafety, String> {
    use std::path::Path;
    let path_obj = Path::new(&path);
    crate::safety::analyze_migration_safety(path_obj, size, app)
}

#[tauri::command]
pub async fn get_migration_history(_app: AppHandle) -> Result<Vec<crate::database::migrations::MigrationRecord>, String> {
    use crate::database::MigrationDb;
    
    let db_path = utils::get_migrations_db_path().map_err(|e| e.to_string())?;
    
    let db = MigrationDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    db.get_all_migrations().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_migration_stats(_app: AppHandle) -> Result<crate::database::migrations::MigrationStats, String> {
    use crate::database::MigrationDb;
    
    let db_path = utils::get_migrations_db_path().map_err(|e| e.to_string())?;
    
    let db = MigrationDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    db.get_stats().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rollback_migration(migration_id: i64, _app: AppHandle) -> Result<crate::migration::file_migrator::RollbackResult, String> {
    use crate::database::MigrationDb;
    use crate::migration::FileMigrator;
    
    let db_path = utils::get_migrations_db_path().map_err(|e| e.to_string())?;
    
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
    _app: AppHandle,
) -> Result<(), String> {
    use crate::database::ScanCacheDb;
    
    let db_path = utils::get_scan_cache_db_path().map_err(|e| e.to_string())?;
    
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
    _app: AppHandle,
) -> Result<Option<ScanResult>, String> {
    use crate::database::ScanCacheDb;
    
    let db_path = utils::get_scan_cache_db_path().map_err(|e| e.to_string())?;
    
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
pub async fn clear_scan_cache(_app: AppHandle) -> Result<(), String> {
    use crate::database::ScanCacheDb;
    
    let db_path = utils::get_scan_cache_db_path().map_err(|e| e.to_string())?;
    
    if !db_path.exists() {
        return Ok(());
    }
    
    let db = ScanCacheDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    db.clear_all().map_err(|e| e.to_string())?;
    
    // 压缩数据库以释放空间
    db.conn.execute("VACUUM", []).map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub fn is_elevated() -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        
        unsafe {
            let mut token: HANDLE = HANDLE::default();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_ok() {
                let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
                let mut size = 0u32;
                
                if GetTokenInformation(
                    token,
                    TokenElevation,
                    Some(&mut elevation as *mut _ as *mut _),
                    std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                    &mut size,
                ).is_ok() {
                    return elevation.TokenIsElevated != 0;
                }
            }
        }
    }
    
    false
}

#[tauri::command]
pub fn restart_as_admin(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        
        let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
        
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        
        let status = Command::new("powershell")
            .args(&[
                "-Command",
                &format!("Start-Process -FilePath '{}' -Verb RunAs", exe_path.display())
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| e.to_string())?;
        
        if status.success() {
            app.exit(0);
        } else {
            return Err("Failed to restart as administrator".to_string());
        }
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        return Err("Not supported on this platform".to_string());
    }
    
    Ok(())
}

#[tauri::command]
pub fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[derive(serde::Serialize)]
pub struct CacheInfo {
    pub cache_path: String,
    pub total_size: u64,
    pub caches: Vec<CacheEntry>,
}

#[derive(serde::Serialize)]
pub struct CacheEntry {
    pub disk_path: String,
    pub scan_type: String,
    pub file_count: i64,
    pub total_size: i64,
    pub created_at: String,
    pub cache_size: i64,
}

#[tauri::command]
pub async fn get_cache_info(_app: AppHandle) -> Result<CacheInfo, String> {
    use crate::database::ScanCacheDb;
    
    let db_path = utils::get_scan_cache_db_path().map_err(|e| e.to_string())?;
    let cache_path = db_path.to_string_lossy().to_string();
    
    if !db_path.exists() {
        return Ok(CacheInfo {
            cache_path,
            total_size: 0,
            caches: vec![],
        });
    }
    
    let total_size = std::fs::metadata(&db_path)
        .map(|m| m.len())
        .unwrap_or(0);
    
    let db = ScanCacheDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    
    // 获取所有缓存记录
    let mut stmt = db.conn.prepare(
        "SELECT disk_path, scan_type, file_count, total_size, created_at, LENGTH(result_json) FROM scan_cache ORDER BY created_at DESC"
    ).map_err(|e| e.to_string())?;
    
    let caches = stmt.query_map([], |row| {
        Ok(CacheEntry {
            disk_path: row.get(0)?,
            scan_type: row.get(1)?,
            file_count: row.get(2)?,
            total_size: row.get(3)?,
            created_at: row.get(4)?,
            cache_size: row.get(5)?,
        })
    }).map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.to_string())?;
    
    Ok(CacheInfo {
        cache_path,
        total_size,
        caches,
    })
}

#[tauri::command]
pub async fn delete_cache_entry(disk_path: String, scan_type: String, _app: AppHandle) -> Result<(), String> {
    use crate::database::ScanCacheDb;
    
    let db_path = utils::get_scan_cache_db_path().map_err(|e| e.to_string())?;
    
    if !db_path.exists() {
        return Ok(());
    }
    
    let db = ScanCacheDb::new(db_path.to_str().unwrap()).map_err(|e| e.to_string())?;
    
    db.conn.execute(
        "DELETE FROM scan_cache WHERE disk_path = ?1 AND scan_type = ?2",
        rusqlite::params![disk_path, scan_type],
    ).map_err(|e| e.to_string())?;
    
    // 压缩数据库以释放空间
    db.conn.execute("VACUUM", []).map_err(|e| e.to_string())?;
    
    Ok(())
}
