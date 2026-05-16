use crate::database::{MigrationDb, ScanCacheDb};
use crate::migration::{
    file_migrator::{MigrationProgress, MigrationProgressCallback, MigrationResult},
    FileMigrator, LinkType,
};
use crate::scanner::{
    file_info::{FileInfo, ScanResult},
    timing::StageTimer,
    DiskScanner,
};
use crate::winfs;
use tauri::AppHandle;
use tauri::Emitter;

#[derive(serde::Serialize)]
pub struct ScanCapabilities {
    pub is_elevated: bool,
    pub file_system: String,
    pub mft_available: bool,
    pub preferred_backend: String,
    pub admin_recommended: bool,
    pub reason: String,
}

fn persist_scan_result_async(
    cache_db: ScanCacheDb,
    disk_path: String,
    scan_type: &'static str,
    result: ScanResult,
) {
    let log_path = disk_path.clone();
    tauri::async_runtime::spawn(async move {
        let save_task = tokio::task::spawn_blocking(move || -> Result<(), String> {
            let json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
            cache_db
                .save_scan_result(
                    &disk_path,
                    scan_type,
                    &json,
                    result.total_files as i64,
                    result.total_size as i64,
                )
                .map_err(|e| e.to_string())?;
            Ok(())
        });

        match save_task.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                tracing::warn!("[scan-cache] failed to persist {scan_type} scan for {log_path}: {err}")
            }
            Err(err) => tracing::warn!("[scan-cache] background task join failed for {log_path}: {err}"),
        }
    });
}

fn ensure_deep_scan_type(scan_type: &str) -> Result<(), String> {
    if scan_type == "deep" {
        Ok(())
    } else {
        Err("仅支持深度扫描缓存".to_string())
    }
}

fn cache_has_usable_usn_checkpoint(result: &ScanResult) -> bool {
    result.usn_journal_id.is_some() && result.usn_next_usn.is_some()
}

fn should_rebuild_cached_deep_scan(
    path: &std::path::Path,
    result: &ScanResult,
) -> Option<&'static str> {
    let backend = result.scan_backend.as_deref().unwrap_or("unknown");
    let expects_usn = matches!(backend, "mft_usn" | "incremental_usn");

    if expects_usn && !cache_has_usable_usn_checkpoint(result) {
        return Some("cached_mft_snapshot_missing_usn_checkpoint");
    }

    if winfs::supports_mft_scan(path)
        && !cache_has_usable_usn_checkpoint(result)
        && result.total_dirs > 50_000
    {
        return Some("cached_snapshot_would_fall_back_to_slow_mtime_walk");
    }

    None
}

#[tauri::command]
pub async fn scan_disk_deep(
    path: String,
    app: AppHandle,
    estimated_files: Option<usize>,
    scanner: tauri::State<'_, DiskScanner>,
    cache_db: tauri::State<'_, ScanCacheDb>,
) -> Result<ScanResult, String> {
    use crate::scanner::incremental;

    let estimated_files = estimated_files.unwrap_or(800000);
    let command_timer =
        StageTimer::start("scan-deep-command", format!("scan_disk_deep path={path}"));
    tracing::info!(
        "[scan-deep] request path={} estimated_files={estimated_files}",
        path
    );

    let cache_lookup_timer = StageTimer::start(
        "scan-deep-command",
        format!("cache_lookup path={path} type=deep"),
    );
    let cached = cache_db.get_scan_result(&path, "deep");
    match &cached {
        Ok(Some(_)) => cache_lookup_timer.finish_with("hit=true"),
        Ok(None) => cache_lookup_timer.finish_with("hit=false"),
        Err(err) => cache_lookup_timer.finish_with(format!("hit=unknown err={err}")),
    }

    let mut strategy = "fresh_scan";
    let full_result = if let Ok(Some(cached)) = cached {
        tracing::info!("[scan-deep] found cached deep snapshot for {}", path);
        let deserialize_timer = StageTimer::start(
            "scan-deep-command",
            format!("deserialize_cached_result path={path}"),
        );
        let cached_result = serde_json::from_str::<ScanResult>(&cached.result_json);
        match &cached_result {
            Ok(result) => deserialize_timer.finish_with(format!(
                "status=ok backend={:?} files={} dirs={} size={}",
                result.scan_backend, result.total_files, result.total_dirs, result.total_size
            )),
            Err(err) => deserialize_timer.finish_with(format!("status=corrupt err={err}")),
        }

        if let Ok(cached_result) = cached_result {
            tracing::info!(
                "[scan-deep] cached summary backend={:?} files={} dirs={} size={} inaccessible={} root_file_id={:?} usn_journal_id={:?} usn_next_usn={:?}",
                cached_result.scan_backend,
                cached_result.total_files,
                cached_result.total_dirs,
                cached_result.total_size,
                cached_result.inaccessible_count,
                cached_result.root_file_id,
                cached_result.usn_journal_id,
                cached_result.usn_next_usn
            );

            if let Some(reason) =
                should_rebuild_cached_deep_scan(std::path::Path::new(&path), &cached_result)
            {
                strategy = "fresh_rebuild_stale_deep_cache";
                tracing::info!(
                    "[scan-deep] cached deep snapshot is not eligible for incremental reuse: {reason}; rebuilding from scratch"
                );
                let execute_timer = StageTimer::start(
                    "scan-deep-command",
                    format!("execute_strategy strategy={strategy} path={path}"),
                );
                let result = scanner
                    .scan_deep(&path, app, estimated_files)
                    .await
                    .map_err(|e| e.to_string())?;
                execute_timer.finish_with(format!(
                    "backend={:?} files={} dirs={} size={} inaccessible={}",
                    result.scan_backend,
                    result.total_files,
                    result.total_dirs,
                    result.total_size,
                    result.inaccessible_count
                ));
                persist_scan_result_async(
                    cache_db.inner().clone(),
                    path.clone(),
                    "deep",
                    result.clone(),
                );
                result
            } else {
                strategy = "incremental_cache";
                let execute_timer = StageTimer::start(
                    "scan-deep-command",
                    format!("execute_strategy strategy={strategy} path={path}"),
                );
                let result = incremental::scan_incremental(
                    std::path::Path::new(&path),
                    cached_result,
                    app.clone(),
                    scanner.inner(),
                )
                .await
                .map_err(|e| e.to_string())?;
                execute_timer.finish_with(format!(
                    "backend={:?} files={} dirs={} size={} inaccessible={}",
                    result.scan_backend,
                    result.total_files,
                    result.total_dirs,
                    result.total_size,
                    result.inaccessible_count
                ));
                persist_scan_result_async(
                    cache_db.inner().clone(),
                    path.clone(),
                    "deep",
                    result.clone(),
                );
                result
            }
        } else {
            strategy = "fresh_rebuild_corrupt_deep_cache";
            tracing::info!("[scan-deep] cached deep snapshot is corrupt, rebuilding from scratch");
            let execute_timer = StageTimer::start(
                "scan-deep-command",
                format!("execute_strategy strategy={strategy} path={path}"),
            );
            let result = scanner
                .scan_deep(&path, app, estimated_files)
                .await
                .map_err(|e| e.to_string())?;
            execute_timer.finish_with(format!(
                "backend={:?} files={} dirs={} size={} inaccessible={}",
                result.scan_backend,
                result.total_files,
                result.total_dirs,
                result.total_size,
                result.inaccessible_count
            ));
            persist_scan_result_async(
                cache_db.inner().clone(),
                path.clone(),
                "deep",
                result.clone(),
            );
            result
        }
    } else {
        tracing::info!(
            "[scan-deep] no cached deep snapshot for {}, running fresh deep scan",
            path
        );
        let execute_timer = StageTimer::start(
            "scan-deep-command",
            format!("execute_strategy strategy={strategy} path={path}"),
        );
        let result = scanner
            .scan_deep(&path, app, estimated_files)
            .await
            .map_err(|e| e.to_string())?;
        execute_timer.finish_with(format!(
            "backend={:?} files={} dirs={} size={} inaccessible={}",
            result.scan_backend,
            result.total_files,
            result.total_dirs,
            result.total_size,
            result.inaccessible_count
        ));
        persist_scan_result_async(
            cache_db.inner().clone(),
            path.clone(),
            "deep",
            result.clone(),
        );
        result
    };

    tracing::info!(
        "[scan-deep] completed strategy={} backend={:?} files={} dirs={} size={} inaccessible={} duration_ms={}",
        strategy,
        full_result.scan_backend,
        full_result.total_files,
        full_result.total_dirs,
        full_result.total_size,
        full_result.inaccessible_count,
        full_result.scan_duration_ms
    );

    let index_timer =
        StageTimer::start("scan-deep-command", format!("build_scan_index path={path}"));
    scanner.store_indexed_scan_result(&full_result);
    let snapshot = scanner
        .get_directory_snapshot(&path, &path)
        .ok_or_else(|| "无法建立深度扫描索引".to_string())?;
    index_timer.finish_with(format!(
        "backend={:?} files={} dirs={} size={}",
        snapshot.scan_backend, snapshot.total_files, snapshot.total_dirs, snapshot.total_size
    ));
    tracing::info!(
        "[scan-deep] indexed root snapshot ready path={} backend={:?} files={} dirs={} size={}",
        snapshot.root_path,
        snapshot.scan_backend,
        snapshot.total_files,
        snapshot.total_dirs,
        snapshot.total_size
    );
    command_timer.finish_with(format!(
        "strategy={} backend={:?} files={} dirs={} size={} duration_ms={}",
        strategy,
        full_result.scan_backend,
        full_result.total_files,
        full_result.total_dirs,
        full_result.total_size,
        full_result.scan_duration_ms
    ));
    Ok(snapshot)
}

#[tauri::command]
pub fn get_directory_snapshot(
    root_path: String,
    path: String,
    scanner: tauri::State<'_, DiskScanner>,
) -> Result<ScanResult, String> {
    scanner
        .get_directory_snapshot(&root_path, &path)
        .ok_or_else(|| "目录快照不存在，请重新执行深度扫描".to_string())
}

#[tauri::command]
pub async fn cancel_scan(scanner: tauri::State<'_, DiskScanner>) -> Result<(), String> {
    scanner.cancel();
    Ok(())
}

#[tauri::command]
pub async fn scan_directory_files(path: String) -> Result<Vec<FileInfo>, String> {
    tokio::task::spawn_blocking(move || {
        let dir_path = std::path::PathBuf::from(path);
        if !dir_path.exists() || !dir_path.is_dir() {
            return Err("路径不存在或不是目录".to_string());
        }

        let mut files = Vec::new();
        for entry in winfs::enumerate_directory(&dir_path, false).map_err(|e| e.to_string())? {
            let modified_at = entry
                .modified_time
                .and_then(|secs| chrono::DateTime::from_timestamp(secs as i64, 0))
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default();

            if entry.is_symlink {
                if entry.is_dir {
                    continue;
                }

                files.push(FileInfo {
                    path: entry.path.to_string_lossy().to_string(),
                    name: entry.name,
                    size: 0,
                    extension: entry
                        .path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_string(),
                    modified_at,
                    is_readonly: entry.is_readonly,
                    is_symlink: true,
                    link_target: winfs::resolve_link_target(&entry.path),
                });
                continue;
            }

            if !entry.is_dir {
                files.push(FileInfo {
                    path: entry.path.to_string_lossy().to_string(),
                    name: entry.name,
                    size: entry.size,
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
        }
        files.sort_by(|a, b| b.size.cmp(&a.size));
        Ok(files)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn migrate_file(
    source: String,
    target_disk: String,
    link_type: Option<LinkType>,
    known_size: Option<u64>,
    known_files: Option<usize>,
    app: AppHandle,
    migration_db: tauri::State<'_, MigrationDb>,
) -> Result<MigrationResult, String> {
    tracing::info!(
        "[migration] request source={} target_disk={} requested_link_type={:?} known_size={:?} known_files={:?}",
        source,
        target_disk,
        link_type,
        known_size,
        known_files
    );
    let migrator = FileMigrator::new();
    let lt = link_type.unwrap_or(LinkType::Auto);
    let known_stats = match (known_size, known_files) {
        (Some(size), Some(files)) => Some((size, files)),
        _ => None,
    };
    let progress_callback: MigrationProgressCallback = std::sync::Arc::new({
        let app = app.clone();
        move |progress: MigrationProgress| {
            let _ = app.emit("migration-progress", progress);
        }
    });
    let mut result = migrator
        .migrate(
            &source,
            &target_disk,
            lt,
            known_stats,
            Some(progress_callback),
        )
        .await
        .map_err(|e| e.to_string())?;

    if !result.success {
        tracing::warn!(
            "[migration] failed source={} target_disk={} error={:?}",
            source, target_disk, result.error
        );
        return Err(result
            .error
            .take()
            .unwrap_or_else(|| "迁移失败".to_string()));
    }

    let lt_str = match result.link_type {
        LinkType::Junction => "Junction",
        LinkType::Symlink => "Symlink",
        LinkType::Hardlink => "Hardlink",
        LinkType::Auto => "Auto",
        LinkType::None => "None",
    };
    if let Ok(id) = migration_db.insert_migration(
        &result.source_path,
        &result.target_path,
        lt_str,
        result.file_size,
    ) {
        result.migration_id = id;
    }
    // 迁移成功后，源/目标路径状态已变化，缓存的安全分析结果作废。
    crate::safety::invalidate_safety_cache();
    tracing::info!(
        "[migration] completed source={} target={} actual_link_type={:?} size={} duration_ms={} history_id={}",
        result.source_path,
        result.target_path,
        result.link_type,
        result.file_size,
        result.duration_ms,
        result.migration_id
    );
    Ok(result)
}

#[tauri::command]
pub async fn get_disk_info() -> Result<Vec<DiskInfo>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            GetDiskFreeSpaceExW, GetLogicalDrives, GetVolumeInformationW,
        };

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
                        Some(&mut total_bytes),
                        Some(&mut free_bytes),
                    )
                    .is_ok()
                    {
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

                        disks.push(DiskInfo {
                            drive_letter,
                            used_space,
                            label: if label.is_empty() {
                                "Local Disk".to_string()
                            } else {
                                label
                            },
                            file_system: if fs.is_empty() {
                                "Unknown".to_string()
                            } else {
                                fs
                            },
                            total_space: total_bytes,
                            free_space: free_bytes,
                            usage_percent: if total_bytes > 0 {
                                used_space as f64 / total_bytes as f64 * 100.0
                            } else {
                                0.0
                            },
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
pub async fn analyze_migration_safety(
    path: String,
    size: Option<u64>,
    link_type: Option<String>,
    target_disk: Option<String>,
) -> Result<crate::safety::MigrationSafety, String> {
    tokio::task::spawn_blocking(move || {
        let path_buf = std::path::PathBuf::from(&path);
        let lt = match link_type.as_deref() {
            Some("none") => crate::migration::LinkType::None,
            Some("symlink") => crate::migration::LinkType::Symlink,
            Some("junction") => crate::migration::LinkType::Junction,
            _ => crate::migration::LinkType::Auto,
        };
        Ok(crate::safety::analyze(
            &path_buf,
            lt,
            target_disk.as_deref(),
            size.unwrap_or(0),
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_migration_history(
    db: tauri::State<'_, MigrationDb>,
) -> Result<Vec<crate::database::migrations::MigrationRecord>, String> {
    db.get_all_migrations().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_migration_stats(
    db: tauri::State<'_, MigrationDb>,
) -> Result<crate::database::migrations::MigrationStats, String> {
    db.get_stats().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rollback_migration(
    migration_id: i64,
    db: tauri::State<'_, MigrationDb>,
) -> Result<crate::migration::file_migrator::RollbackResult, String> {
    let record = db
        .get_migration_by_id(migration_id)
        .map_err(|e| e.to_string())?
        .ok_or("Migration record not found")?;
    if record.status != "active" {
        return Err("Migration is not active".to_string());
    }
    tracing::info!(
        "[migration] rollback request id={} source={} target={}",
        migration_id, record.source_path, record.target_path
    );

    let migrator = FileMigrator::new();
    let result = migrator
        .rollback(
            std::path::Path::new(&record.source_path),
            std::path::Path::new(&record.target_path),
        )
        .await
        .map_err(|e| e.to_string())?;
    if !result.success {
        tracing::warn!(
            "[migration] rollback failed id={} source={} target={} error={:?}",
            migration_id, record.source_path, record.target_path, result.error
        );
        return Err(result.error.unwrap_or_else(|| "回滚失败".to_string()));
    }
    db.update_status(migration_id, "rolled_back")
        .map_err(|e| e.to_string())?;
    // 回滚后路径状态恢复，安全分析的缓存也要清。
    crate::safety::invalidate_safety_cache();
    tracing::info!(
        "[migration] rollback completed id={} source={} target={} duration_ms={}",
        migration_id, result.source_path, result.target_path, result.duration_ms
    );
    Ok(result)
}

#[tauri::command]
pub async fn save_scan_cache(
    disk_path: String,
    scan_type: String,
    result: ScanResult,
    cache_db: tauri::State<'_, ScanCacheDb>,
) -> Result<(), String> {
    ensure_deep_scan_type(&scan_type)?;
    let json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
    cache_db
        .save_scan_result(
            &disk_path,
            &scan_type,
            &json,
            result.total_files as i64,
            result.total_size as i64,
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_scan_cache(
    disk_path: String,
    scan_type: String,
    cache_db: tauri::State<'_, ScanCacheDb>,
) -> Result<Option<ScanResult>, String> {
    ensure_deep_scan_type(&scan_type)?;
    match cache_db
        .get_scan_result(&disk_path, &scan_type)
        .map_err(|e| e.to_string())?
    {
        Some(cached) => Ok(Some(
            serde_json::from_str(&cached.result_json).map_err(|e| e.to_string())?,
        )),
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
        use windows::Win32::Security::{
            GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
        };
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        unsafe {
            let mut token = HANDLE::default();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_ok() {
                let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
                let mut size = 0u32;
                if GetTokenInformation(
                    token,
                    TokenElevation,
                    Some(&mut elevation as *mut _ as *mut _),
                    std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                    &mut size,
                )
                .is_ok()
                {
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
        use std::path::Path;
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        fn to_wide(path: &Path) -> Vec<u16> {
            use std::os::windows::ffi::OsStrExt;

            path.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        }

        let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
        let operation: Vec<u16> = "runas\0".encode_utf16().collect();
        let file = to_wide(&exe_path);
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(file.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if result.0 as usize <= 32 {
            return Err("Failed to restart as administrator".to_string());
        }
        app.exit(0);
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
pub async fn get_cache_info(cache_db: tauri::State<'_, ScanCacheDb>) -> Result<CacheInfo, String> {
    let cache_path = crate::utils::get_scan_cache_db_path().map_err(|e| e.to_string())?;
    let total_size = std::fs::metadata(&cache_path).map(|m| m.len()).unwrap_or(0);
    let entries = cache_db.get_all_entries().map_err(|e| e.to_string())?;
    let caches = entries
        .into_iter()
        .map(|(dp, st, fc, ts, ca, cs)| CacheEntry {
            disk_path: dp,
            scan_type: st,
            file_count: fc,
            total_size: ts,
            created_at: ca,
            cache_size: cs,
        })
        .collect();
    Ok(CacheInfo {
        cache_path: cache_path.to_string_lossy().to_string(),
        total_size,
        caches,
    })
}

#[tauri::command]
pub async fn delete_cache_entry(
    disk_path: String,
    scan_type: String,
    cache_db: tauri::State<'_, ScanCacheDb>,
) -> Result<(), String> {
    ensure_deep_scan_type(&scan_type)?;
    cache_db
        .delete_entry(&disk_path, &scan_type)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_scan_capabilities(path: String) -> Result<ScanCapabilities, String> {
    let path_buf = std::path::PathBuf::from(path);
    let elevated = is_elevated();
    let volume = winfs::query_volume_details(&path_buf);
    let file_system = volume
        .as_ref()
        .map(|details| details.file_system.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    let mft_available = winfs::supports_mft_scan(&path_buf);

    let reason = if mft_available {
        "当前环境可直接使用 MFT + USN 深度扫描".to_string()
    } else if !file_system.eq_ignore_ascii_case("NTFS") {
        "当前卷不是 NTFS，深度扫描将回退到原生目录枚举".to_string()
    } else if !elevated {
        "当前是标准权限。启用管理员模式后，可切换到 MFT + USN 深度扫描".to_string()
    } else {
        "当前卷是 NTFS，但系统没有提供可用的 MFT 访问，深度扫描将回退到原生目录枚举".to_string()
    };

    Ok(ScanCapabilities {
        is_elevated: elevated,
        file_system,
        mft_available,
        preferred_backend: if mft_available {
            "mft_usn".to_string()
        } else {
            "native".to_string()
        },
        admin_recommended: !mft_available && !elevated,
        reason,
    })
}
