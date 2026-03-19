use crate::scanner::{DiskScanner, file_info::{ScanResult, FileInfo}};
use crate::migration::{FileMigrator, LinkType, file_migrator::MigrationResult};
use crate::database::{ScanCacheDb, MigrationDb};
use tauri::AppHandle;

#[cfg(windows)]
fn is_link_entry(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_link_entry(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn path_points_to_directory(path: &std::path::Path) -> bool {
    std::fs::metadata(path).map(|metadata| metadata.is_dir()).unwrap_or(false)
}

fn resolve_link_target(path: &std::path::Path) -> Option<String> {
    let resolved = if let Ok(target) = std::fs::read_link(path) {
        if target.is_absolute() {
            target
        } else if let Some(parent) = path.parent() {
            parent.join(&target)
        } else {
            target
        }
    } else {
        let canonical = std::fs::canonicalize(path).ok()?;
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

fn persist_scan_result_async(cache_db: ScanCacheDb, disk_path: String, scan_type: &'static str, result: ScanResult) {
    let log_path = disk_path.clone();
    tauri::async_runtime::spawn(async move {
        let save_task = tokio::task::spawn_blocking(move || -> Result<(), String> {
            let json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
            cache_db
                .save_scan_result(&disk_path, scan_type, &json, result.total_files as i64, result.total_size as i64)
                .map_err(|e| e.to_string())?;
            Ok(())
        });

        match save_task.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => eprintln!("[scan-cache] failed to persist {scan_type} scan for {log_path}: {err}"),
            Err(err) => eprintln!("[scan-cache] background task join failed for {log_path}: {err}"),
        }
    });
}

#[tauri::command]
pub async fn scan_disk(path: String, app: AppHandle, scanner: tauri::State<'_, DiskScanner>) -> Result<ScanResult, String> {
    scanner.scan(&path, app).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_disk_incremental(
    path: String, app: AppHandle,
    scanner: tauri::State<'_, DiskScanner>,
    cache_db: tauri::State<'_, ScanCacheDb>,
) -> Result<ScanResult, String> {
    let result = scanner.scan(&path, app).await.map_err(|e| e.to_string())?;
    persist_scan_result_async(cache_db.inner().clone(), path, "quick", result.clone());
    Ok(result)
}

#[tauri::command]
pub async fn scan_disk_deep(
    path: String, app: AppHandle, estimated_files: Option<usize>,
    scanner: tauri::State<'_, DiskScanner>,
    cache_db: tauri::State<'_, ScanCacheDb>,
) -> Result<ScanResult, String> {
    use crate::scanner::incremental;

    if let Ok(Some(cached)) = cache_db.get_scan_result(&path, "deep") {
        if let Ok(cached_result) = serde_json::from_str::<ScanResult>(&cached.result_json) {
            let result = incremental::scan_incremental(
                std::path::Path::new(&path),
                cached_result,
                app.clone(),
            )
                .await.map_err(|e| e.to_string())?;
            persist_scan_result_async(cache_db.inner().clone(), path, "deep", result.clone());
            return Ok(result);
        }
    }

    let result = scanner.scan_deep(&path, app, estimated_files.unwrap_or(800000))
        .await.map_err(|e| e.to_string())?;
    persist_scan_result_async(cache_db.inner().clone(), path, "deep", result.clone());
    Ok(result)
}

#[tauri::command]
pub async fn cancel_scan(scanner: tauri::State<'_, DiskScanner>) -> Result<(), String> {
    scanner.cancel();
    Ok(())
}

#[tauri::command]
pub async fn scan_directory_files(path: String) -> Result<Vec<FileInfo>, String> {
    let dir_path = std::path::Path::new(&path);
    if !dir_path.exists() || !dir_path.is_dir() {
        return Err("路径不存在或不是目录".to_string());
    }

    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir_path).map_err(|e| format!("无法读取目录: {}", e))?.flatten() {
        let path = entry.path();
        let Ok(link_metadata) = std::fs::symlink_metadata(&path) else { continue; };
        let modified_at = link_metadata.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();

        if is_link_entry(&link_metadata) {
            if path_points_to_directory(&path) {
                continue;
            }

            #[cfg(windows)]
            use std::os::windows::fs::MetadataExt;

            files.push(FileInfo {
                path: path.to_string_lossy().to_string(),
                name: path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown").to_string(),
                size: 0,
                extension: path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string(),
                modified_at,
                #[cfg(windows)]
                is_readonly: link_metadata.file_attributes() & 0x1 != 0,
                #[cfg(not(windows))]
                is_readonly: link_metadata.permissions().readonly(),
                is_symlink: true,
                link_target: resolve_link_target(&path),
            });
            continue;
        }

        if link_metadata.is_file() {
            #[cfg(windows)]
            use std::os::windows::fs::MetadataExt;

            files.push(FileInfo {
                path: path.to_string_lossy().to_string(),
                name: path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown").to_string(),
                size: link_metadata.len(),
                extension: path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string(),
                modified_at,
                #[cfg(windows)]
                is_readonly: link_metadata.file_attributes() & 0x1 != 0,
                #[cfg(not(windows))]
                is_readonly: link_metadata.permissions().readonly(),
                is_symlink: false,
                link_target: None,
            });
        }
    }
    files.sort_by(|a, b| b.size.cmp(&a.size));
    Ok(files)
}

#[tauri::command]
pub async fn migrate_file(
    source: String, target_disk: String, link_type: Option<LinkType>,
    app: AppHandle, migration_db: tauri::State<'_, MigrationDb>,
) -> Result<MigrationResult, String> {
    let migrator = FileMigrator::new();
    let lt = link_type.unwrap_or(LinkType::Auto);
    let mut result = migrator.migrate(&source, &target_disk, lt, Some(app))
        .await.map_err(|e| e.to_string())?;

    if !result.success {
        return Err(result.error.take().unwrap_or_else(|| "迁移失败".to_string()));
    }

    let lt_str = match result.link_type {
        LinkType::Junction => "Junction", LinkType::Symlink => "Symlink",
        LinkType::Hardlink => "Hardlink", LinkType::Auto => "Auto", LinkType::None => "None",
    };
    if let Ok(id) = migration_db.insert_migration(&result.source_path, &result.target_path, lt_str, result.file_size) {
        result.migration_id = id;
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_disk_info() -> Result<Vec<DiskInfo>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetVolumeInformationW, GetLogicalDrives};
        use windows::core::PCWSTR;

        let mut disks = Vec::new();
        unsafe {
            let drives = GetLogicalDrives();
            for i in 0..26 {
                if (drives & (1 << i)) != 0 {
                    let drive_letter = format!("{}:", (b'A' + i) as char);
                    let drive_path = format!("{}\\", drive_letter);
                    let path_wide: Vec<u16> = std::ffi::OsStr::new(&drive_path)
                        .encode_wide().chain(std::iter::once(0)).collect();

                    let mut total_bytes = 0u64;
                    let mut free_bytes = 0u64;
                    if GetDiskFreeSpaceExW(PCWSTR(path_wide.as_ptr()), None, Some(&mut total_bytes), Some(&mut free_bytes)).is_ok() {
                        let mut volume_name = vec![0u16; 256];
                        let mut file_system = vec![0u16; 256];
                        let _ = GetVolumeInformationW(PCWSTR(path_wide.as_ptr()), Some(&mut volume_name), None, None, None, Some(&mut file_system));

                        let label = String::from_utf16_lossy(&volume_name).trim_end_matches('\0').to_string();
                        let fs = String::from_utf16_lossy(&file_system).trim_end_matches('\0').to_string();
                        let used_space = total_bytes.saturating_sub(free_bytes);

                        disks.push(DiskInfo {
                            drive_letter, used_space,
                            label: if label.is_empty() { "Local Disk".to_string() } else { label },
                            file_system: if fs.is_empty() { "Unknown".to_string() } else { fs },
                            total_space: total_bytes, free_space: free_bytes,
                            usage_percent: if total_bytes > 0 { used_space as f64 / total_bytes as f64 * 100.0 } else { 0.0 },
                        });
                    }
                }
            }
        }
        Ok(disks)
    }
    #[cfg(not(target_os = "windows"))]
    { Ok(vec![]) }
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
    crate::safety::analyze_migration_safety(std::path::Path::new(&path), size, app)
}

#[tauri::command]
pub async fn get_migration_history(db: tauri::State<'_, MigrationDb>) -> Result<Vec<crate::database::migrations::MigrationRecord>, String> {
    db.get_all_migrations().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_migration_stats(db: tauri::State<'_, MigrationDb>) -> Result<crate::database::migrations::MigrationStats, String> {
    db.get_stats().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rollback_migration(migration_id: i64, db: tauri::State<'_, MigrationDb>) -> Result<crate::migration::file_migrator::RollbackResult, String> {
    let record = db.get_migration_by_id(migration_id).map_err(|e| e.to_string())?
        .ok_or("Migration record not found")?;
    if record.status != "active" { return Err("Migration is not active".to_string()); }

    let migrator = FileMigrator::new();
    let result = migrator.rollback(std::path::Path::new(&record.source_path), std::path::Path::new(&record.target_path))
        .await.map_err(|e| e.to_string())?;
    if !result.success {
        return Err(result.error.unwrap_or_else(|| "回滚失败".to_string()));
    }
    db.update_status(migration_id, "rolled_back").map_err(|e| e.to_string())?;
    Ok(result)
}

#[tauri::command]
pub async fn save_scan_cache(disk_path: String, scan_type: String, result: ScanResult, cache_db: tauri::State<'_, ScanCacheDb>) -> Result<(), String> {
    let json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
    cache_db.save_scan_result(&disk_path, &scan_type, &json, result.total_files as i64, result.total_size as i64)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_scan_cache(disk_path: String, scan_type: String, cache_db: tauri::State<'_, ScanCacheDb>) -> Result<Option<ScanResult>, String> {
    match cache_db.get_scan_result(&disk_path, &scan_type).map_err(|e| e.to_string())? {
        Some(cached) => Ok(Some(serde_json::from_str(&cached.result_json).map_err(|e| e.to_string())?)),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn clear_scan_cache(cache_db: tauri::State<'_, ScanCacheDb>) -> Result<(), String> {
    cache_db.clear_all().map_err(|e| e.to_string())?;
    cache_db.vacuum().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn is_elevated() -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        unsafe {
            let mut token = HANDLE::default();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_ok() {
                let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
                let mut size = 0u32;
                if GetTokenInformation(token, TokenElevation, Some(&mut elevation as *mut _ as *mut _),
                    std::mem::size_of::<TOKEN_ELEVATION>() as u32, &mut size).is_ok() {
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
        let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
        let status = std::process::Command::new("powershell")
            .args(&["-Command", &format!("Start-Process -FilePath '{}' -Verb RunAs", exe_path.display())])
            .creation_flags(0x08000000)
            .status().map_err(|e| e.to_string())?;
        if status.success() { app.exit(0); }
        else { return Err("Failed to restart as administrator".to_string()); }
    }
    #[cfg(not(target_os = "windows"))]
    { return Err("Not supported on this platform".to_string()); }
    Ok(())
}

#[tauri::command]
pub fn exit_app(app: AppHandle) { app.exit(0); }

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
pub async fn get_cache_info(cache_db: tauri::State<'_, ScanCacheDb>) -> Result<CacheInfo, String> {
    let cache_path = crate::utils::get_scan_cache_db_path().map_err(|e| e.to_string())?;
    let total_size = std::fs::metadata(&cache_path).map(|m| m.len()).unwrap_or(0);
    let entries = cache_db.get_all_entries().map_err(|e| e.to_string())?;
    let caches = entries.into_iter().map(|(dp, st, fc, ts, ca, cs)| {
        CacheEntry { disk_path: dp, scan_type: st, file_count: fc, total_size: ts, created_at: ca, cache_size: cs }
    }).collect();
    Ok(CacheInfo { cache_path: cache_path.to_string_lossy().to_string(), total_size, caches })
}

#[tauri::command]
pub async fn delete_cache_entry(disk_path: String, scan_type: String, cache_db: tauri::State<'_, ScanCacheDb>) -> Result<(), String> {
    cache_db.delete_entry(&disk_path, &scan_type).map_err(|e| e.to_string())
}
