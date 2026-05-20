use crate::database::{MigrationDb, ScanCacheDb, SpaceHistoryDb};
use crate::migration::{
    delete::{DeleteMode, DeleteProgress, DeleteProgressCallback, DeleteResult},
    file_migrator::{MigrationProgress, MigrationProgressCallback, MigrationResult},
    FileMigrator, LinkType,
};
use crate::pending_intent::{self, PendingScanIntent};
use crate::scanner::{
    duplicates::{find_duplicates_blocking, DuplicateGroup},
    env_fingerprint::{EnvFingerprint, CACHE_SCHEMA_VERSION},
    file_info::{FileInfo, ScanResult},
    timing::StageTimer,
    DiskScanner,
};
use crate::session::ScanSessionRegistry;
use crate::winfs;
use std::sync::Arc;
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
    fingerprint: EnvFingerprint,
) {
    let log_path = disk_path.clone();
    tauri::async_runtime::spawn(async move {
        let save_task = tokio::task::spawn_blocking(move || -> Result<(), String> {
            // 把指纹 + schema_version + scan_completed 写进 ScanResult JSON，
            // 这样下次读出来就能直接和当前环境对比，无需再依赖 SQL 元列。
            let mut sealed = result;
            sealed.cache_schema_version = CACHE_SCHEMA_VERSION;
            sealed.env_fingerprint = fingerprint.clone();
            sealed.scan_completed = true;
            let json = serde_json::to_string(&sealed).map_err(|e| e.to_string())?;
            let fp_json = serde_json::to_string(&fingerprint).map_err(|e| e.to_string())?;
            // 第一步：先 UPSERT 一条 scan_completed=0 的占位记录。
            // 进程在 mark_scan_completed 之前挂掉，下次启动看到 0 直接判脏。
            cache_db
                .save_scan_result(
                    &disk_path,
                    scan_type,
                    &json,
                    sealed.total_files as i64,
                    sealed.total_size as i64,
                    &fp_json,
                    false,
                )
                .map_err(|e| e.to_string())?;
            // 第二步：把占位记录置为 scan_completed=1。
            cache_db
                .mark_scan_completed(&disk_path, scan_type)
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
    current_fp: &EnvFingerprint,
) -> Option<&'static str> {
    // 任何环境/完成态/schema 异常都比 USN 检查优先：先排除"根本不该被复用"的情况。
    if !result.scan_completed {
        return Some("cache_marked_incomplete");
    }
    if result.cache_schema_version != CACHE_SCHEMA_VERSION {
        return Some("cache_schema_version_outdated");
    }
    if &result.env_fingerprint != current_fp {
        return Some("env_fingerprint_changed");
    }

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
    space_history: tauri::State<'_, SpaceHistoryDb>,
    sessions: tauri::State<'_, Arc<ScanSessionRegistry>>,
) -> Result<ScanResult, String> {
    use crate::scanner::incremental;

    let estimated_files = estimated_files.unwrap_or(800000);
    let command_timer =
        StageTimer::start("scan-deep-command", format!("scan_disk_deep path={path}"));
    tracing::info!(
        "[scan-deep] request path={} estimated_files={estimated_files}",
        path
    );

    // 在这一刻拍下当前环境指纹，作为整次 scan_disk_deep 的判定基线（指纹一旦变了就走 fresh_rebuild）。
    let current_fp = EnvFingerprint::current(std::path::Path::new(&path));
    tracing::info!(
        "[scan-deep] env fingerprint elevated={} fs={} volume_serial={:?} app={} rule={} sid={:?}",
        current_fp.is_elevated,
        current_fp.file_system,
        current_fp.volume_serial,
        current_fp.app_version,
        current_fp.rule_version,
        current_fp.user_sid,
    );

    // 同 disk 已有 inflight 会话先取消并等它退出（最长 5 秒）。
    // RAII：本函数返回 / panic 时 session 会自动从注册表移除并 cancel token，
    // 避免半成品扫描继续往 cache 写数据。
    let registry = sessions.inner().clone();
    let mut session = registry.begin_session(&path).await;
    let token = session.token();
    let scan_token = token.clone();

    let cache_lookup_timer = StageTimer::start(
        "scan-deep-command",
        format!("cache_lookup path={path} type=deep"),
    );
    let cached = cache_db.get_valid_scan_result(&path, "deep", &current_fp);
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
        let cached_result = cached.deserialize_result::<ScanResult>();
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
                should_rebuild_cached_deep_scan(std::path::Path::new(&path), &cached_result, &current_fp)
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
                    .scan_deep_with_token(&path, app, estimated_files, &scan_token)
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
                if scan_token.is_cancelled() {
                    return Err("扫描已取消".to_string());
                }
                persist_scan_result_async(
                    cache_db.inner().clone(),
                    path.clone(),
                    "deep",
                    result.clone(),
                    current_fp.clone(),
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
                    Some(&scan_token),
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
                if scan_token.is_cancelled() {
                    return Err("扫描已取消".to_string());
                }
                persist_scan_result_async(
                    cache_db.inner().clone(),
                    path.clone(),
                    "deep",
                    result.clone(),
                    current_fp.clone(),
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
                .scan_deep_with_token(&path, app, estimated_files, &scan_token)
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
            if scan_token.is_cancelled() {
                return Err("扫描已取消".to_string());
            }
            persist_scan_result_async(
                cache_db.inner().clone(),
                path.clone(),
                "deep",
                result.clone(),
                current_fp.clone(),
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
            .scan_deep_with_token(&path, app, estimated_files, &scan_token)
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
        if scan_token.is_cancelled() {
            return Err("扫描已取消".to_string());
        }
        persist_scan_result_async(
            cache_db.inner().clone(),
            path.clone(),
            "deep",
            result.clone(),
            current_fp.clone(),
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

    if let Some(drive) = drive_letter_from_path(&path) {
        let (total, used) = disk_total_used(&path).unwrap_or((0, 0));
        if let Err(err) = space_history.record_snapshot(&drive, total, used, full_result.total_size)
        {
            tracing::warn!(
                "[space-history] failed to record snapshot drive={drive} error={err}"
            );
        }
    }

    // 走到这里说明扫描成功完成且没被取消：标记 session 为完成态，
    // 让 SessionHandle::drop 不再额外触发 cancel。
    session.mark_completed();
    let _ = token;
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
pub fn get_large_files(
    root_path: String,
    path: Option<String>,
    scanner: tauri::State<'_, DiskScanner>,
) -> Result<Vec<FileInfo>, String> {
    scanner
        .with_indexed(&root_path, |indexed| {
            indexed
                .large_files_for_path_public(path.as_deref().unwrap_or(&root_path))
                .into_iter()
                .collect::<Vec<_>>()
        })
        .ok_or_else(|| "尚未扫描，请先执行深度扫描".to_string())
}

#[tauri::command]
pub fn analyze_smart_groups(
    root_path: String,
    scanner: tauri::State<'_, DiskScanner>,
) -> Result<crate::scanner::smart_scan::SmartScanReport, String> {
    scanner
        .with_indexed(&root_path, |indexed| {
            crate::scanner::smart_scan::build_report(indexed)
        })
        .ok_or_else(|| "尚未扫描，请先执行深度扫描".to_string())
}

#[tauri::command]
pub fn reveal_in_explorer(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("explorer.exe")
            .arg(format!("/select,{}", path))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    let _ = path;
    Ok(())
}

#[tauri::command]
pub async fn cancel_scan(
    scanner: tauri::State<'_, DiskScanner>,
    sessions: tauri::State<'_, Arc<ScanSessionRegistry>>,
    disk_path: Option<String>,
) -> Result<(), String> {
    // 兼容旧调用：DiskScanner 自身的 AtomicBool 仍然 cancel 一下，
    // 同时把 session registry 里对应（或全部）会话也 cancel。
    scanner.cancel();
    match disk_path {
        Some(path) if !path.is_empty() => sessions.cancel(&path),
        _ => sessions.cancel_all(),
    }
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
pub async fn delete_path(
    path: String,
    mode: DeleteMode,
    app: AppHandle,
    migration_db: tauri::State<'_, MigrationDb>,
) -> Result<DeleteResult, String> {
    tracing::info!("[delete] request path={} mode={:?}", path, mode);

    let safety_path = std::path::PathBuf::from(&path);
    let safety = tokio::task::spawn_blocking({
        let path_buf = safety_path.clone();
        move || crate::safety::analyze(&path_buf, LinkType::None, None, 0)
    })
    .await
    .map_err(|e| e.to_string())?;

    if matches!(safety.verdict, crate::safety::Verdict::SystemCritical) {
        tracing::warn!(
            "[delete] blocked by safety verdict=SystemCritical path={}",
            path
        );
        return Err(format!(
            "系统关键路径不允许删除：{}",
            safety
                .findings
                .iter()
                .filter(|f| f.gate == "system_critical")
                .map(|f| f.message.clone())
                .next()
                .unwrap_or_else(|| "命中 system_critical 规则".to_string())
        ));
    }

    let progress_callback: DeleteProgressCallback = std::sync::Arc::new({
        let app = app.clone();
        move |progress: DeleteProgress| {
            let _ = app.emit("delete-progress", progress);
        }
    });

    let result = crate::migration::delete::delete_path(&path, mode, Some(progress_callback))
        .await
        .map_err(|e| e.to_string())?;

    if result.deleted_size > 0 || result.deleted_files > 0 {
        let target_label = match mode {
            DeleteMode::Recycle => "recycle_bin",
            DeleteMode::Permanent => "permanent",
        };
        if let Err(err) = migration_db.insert_delete_record(
            &result.source_path,
            target_label,
            result.deleted_size,
        ) {
            tracing::warn!(
                "[delete] failed to insert history record path={} error={err}",
                result.source_path
            );
        }
    }

    crate::safety::invalidate_safety_cache();

    tracing::info!(
        "[delete] completed path={} mode={:?} success={} size={} files={} errors={} duration_ms={}",
        result.source_path,
        result.mode,
        result.success,
        result.deleted_size,
        result.deleted_files,
        result.errors.len(),
        result.duration_ms
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
    let is_known_folder_redirect = record.link_type.starts_with("KnownFolderRedirect");
    if record.status != "active" && !(record.status == "redirected" && is_known_folder_redirect) {
        return Err("Migration is not active".to_string());
    }
    tracing::info!(
        "[migration] rollback request id={} source={} target={}",
        migration_id, record.source_path, record.target_path
    );

    let source = std::path::Path::new(&record.source_path);
    let target = std::path::Path::new(&record.target_path);
    let result = if is_known_folder_redirect {
        let folder_id =
            crate::folder_redirect::folder_id_from_redirect_record(&record.link_type, source, target)
                .ok_or_else(|| "Cannot identify known folder for redirect rollback".to_string())?;
        crate::folder_redirect::rollback_known_folder_redirect(&folder_id, source, target, None)
            .await
            .map_err(|e| e.to_string())?
    } else {
        let migrator = FileMigrator::new();
        migrator
            .rollback(source, target)
            .await
            .map_err(|e| e.to_string())?
    };
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
    let current_fp = EnvFingerprint::current(std::path::Path::new(&disk_path));
    let mut result = result;
    result.cache_schema_version = CACHE_SCHEMA_VERSION;
    result.env_fingerprint = current_fp.clone();
    result.scan_completed = true;
    let json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
    let fp_json = serde_json::to_string(&current_fp).map_err(|e| e.to_string())?;
    cache_db
        .save_scan_result(
            &disk_path,
            &scan_type,
            &json,
            result.total_files as i64,
            result.total_size as i64,
            &fp_json,
            true,
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
    let current_fp = EnvFingerprint::current(std::path::Path::new(&disk_path));
    match cache_db
        .get_valid_scan_result(&disk_path, &scan_type, &current_fp)
        .map_err(|e| e.to_string())?
    {
        Some(cached) => Ok(Some(
            cached.deserialize_result().map_err(|e| e.to_string())?,
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
    do_restart_as_admin(app)
}

fn do_restart_as_admin(app: AppHandle) -> Result<(), String> {
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
        let _ = app;
        return Err("Not supported on this platform".to_string());
    }
    #[cfg(target_os = "windows")]
    Ok(())
}

/// 用户在 UI 上点「以管理员身份继续」时调用：先把当前 disk 的扫描意图持久化到
/// `pending_scan.json`，再触发 ShellExecuteW("runas") 重启。
///
/// 重启失败时仍把 intent 留在磁盘上是可以接受的：consume() 走 TTL，10 分钟内用户
/// 手动重启也能续上；超过 TTL 则被忽略并删除。
#[tauri::command]
pub fn request_admin_rescan(app: AppHandle, disk: String) -> Result<(), String> {
    let trimmed = disk.trim();
    if trimmed.is_empty() {
        return Err("disk 不能为空".to_string());
    }
    let already_elevated = is_elevated();
    let intent = PendingScanIntent::new(trimmed.to_string(), !already_elevated);
    if let Err(err) = pending_intent::write(&intent) {
        // 写失败不阻塞重启 —— 用户体验上还是要把 UAC 拉起来。
        tracing::warn!("[pending-intent] write failed disk={trimmed} error={err}");
    } else {
        tracing::info!(
            "[pending-intent] saved disk={} requested_with_elevation={}",
            intent.disk,
            intent.requested_with_elevation
        );
    }

    if already_elevated {
        // 已经是管理员的情况下也允许调用：意图已写入，前端应当跳过 UAC，
        // 直接根据 consume_pending_scan_intent 在下一次启动时续扫。
        // 这里不重启，避免无意义的 UAC 弹窗。
        let _ = app;
        return Ok(());
    }

    do_restart_as_admin(app)
}

/// 启动后由前端调用：读出上一次 `request_admin_rescan` 写入的意图，
/// 文件读后立刻删除。返回 None 表示无 pending / 已过期 / JSON 损坏。
#[tauri::command]
pub fn consume_pending_scan_intent() -> Option<PendingScanIntent> {
    pending_intent::consume()
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

fn drive_letter_from_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    let mut chars = trimmed.chars();
    let letter = chars.next()?;
    if !letter.is_ascii_alphabetic() {
        return None;
    }
    let colon = chars.next()?;
    if colon != ':' {
        return None;
    }
    Some(format!("{}:\\", letter.to_ascii_uppercase()))
}

#[cfg(target_os = "windows")]
fn disk_total_used(path: &str) -> Option<(u64, u64)> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let drive = drive_letter_from_path(path)?;
    let path_wide: Vec<u16> = drive.encode_utf16().chain(std::iter::once(0)).collect();
    let mut total = 0u64;
    let mut free = 0u64;
    unsafe {
        if GetDiskFreeSpaceExW(
            PCWSTR(path_wide.as_ptr()),
            None,
            Some(&mut total),
            Some(&mut free),
        )
        .is_ok()
        {
            return Some((total, total.saturating_sub(free)));
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn disk_total_used(_path: &str) -> Option<(u64, u64)> {
    None
}

#[tauri::command]
pub async fn get_space_history(
    drive: String,
    days: u32,
    space_history: tauri::State<'_, SpaceHistoryDb>,
) -> Result<Vec<crate::database::space_history::DiskSnapshot>, String> {
    space_history.get_history(&drive, days).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn find_duplicates(
    root_path: String,
    app: AppHandle,
    scanner: tauri::State<'_, DiskScanner>,
) -> Result<Vec<DuplicateGroup>, String> {
    let candidates = scanner
        .with_indexed(&root_path, |indexed| {
            indexed.large_files_iter().cloned().collect::<Vec<FileInfo>>()
        })
        .ok_or_else(|| "尚未扫描，请先执行深度扫描".to_string())?;

    tracing::info!(
        "[duplicates] start root={} candidates={}",
        root_path,
        candidates.len()
    );

    let app_handle = app.clone();
    let emitter: crate::scanner::duplicates::DuplicateProgressEmitter =
        std::sync::Arc::new(move |progress| {
            let _ = app_handle.emit("duplicate-progress", progress);
        });
    tokio::task::spawn_blocking(move || find_duplicates_blocking(candidates, Some(emitter)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_duplicate_files(
    paths: Vec<String>,
    mode: String,
    app: AppHandle,
) -> Result<DeleteResult, String> {
    let delete_mode = match mode.as_str() {
        "recycle" => DeleteMode::Recycle,
        _ => DeleteMode::Permanent,
    };

    let progress_callback: DeleteProgressCallback = std::sync::Arc::new({
        let app = app.clone();
        move |progress: DeleteProgress| {
            let _ = app.emit("delete-progress", progress);
        }
    });

    let mut total_deleted_size: u64 = 0;
    let mut total_deleted_files: usize = 0;
    let mut errors: Vec<crate::migration::delete::DeleteError> = Vec::new();
    let mut total_duration: u64 = 0;
    let mut last_path = String::new();
    let mut overall_success = true;

    for path in &paths {
        last_path = path.clone();
        match crate::migration::delete::delete_path(
            path,
            delete_mode,
            Some(progress_callback.clone()),
        )
        .await
        {
            Ok(result) => {
                total_deleted_size = total_deleted_size.saturating_add(result.deleted_size);
                total_deleted_files = total_deleted_files.saturating_add(result.deleted_files);
                total_duration = total_duration.saturating_add(result.duration_ms);
                if !result.success {
                    overall_success = false;
                }
                errors.extend(result.errors);
            }
            Err(err) => {
                overall_success = false;
                errors.push(crate::migration::delete::DeleteError::new(
                    path.clone(),
                    err.to_string(),
                ));
            }
        }
    }

    Ok(DeleteResult {
        success: overall_success,
        source_path: last_path,
        mode: delete_mode,
        deleted_size: total_deleted_size,
        deleted_files: total_deleted_files,
        errors,
        duration_ms: total_duration,
    })
}

#[tauri::command]
pub async fn detect_game_libraries() -> Result<Vec<crate::games::GameLibraryInfo>, String> {
    tokio::task::spawn_blocking(|| {
        let mut libraries = Vec::new();

        match crate::games::steam::detect() {
            Ok(Some(info)) => libraries.push(info),
            Ok(None) => libraries.push(crate::games::GameLibraryInfo {
                platform: crate::games::GamePlatform::Steam,
                library_paths: vec![],
                games: vec![],
                installed: false,
            }),
            Err(err) => tracing::warn!("[games] steam detection failed: {err}"),
        }

        match crate::games::epic::detect() {
            Ok(Some(info)) => libraries.push(info),
            Ok(None) => libraries.push(crate::games::GameLibraryInfo {
                platform: crate::games::GamePlatform::Epic,
                library_paths: vec![],
                games: vec![],
                installed: false,
            }),
            Err(err) => tracing::warn!("[games] epic detection failed: {err}"),
        }

        match crate::games::gamepass::detect() {
            Ok(Some(info)) => libraries.push(info),
            Ok(None) => libraries.push(crate::games::GameLibraryInfo {
                platform: crate::games::GamePlatform::GamePass,
                library_paths: vec![],
                games: vec![],
                installed: false,
            }),
            Err(err) => tracing::warn!("[games] gamepass detection failed: {err}"),
        }

        tracing::info!(
            "[games] detection complete platforms={} games={}",
            libraries.len(),
            libraries.iter().map(|l| l.games.len()).sum::<usize>()
        );

        Ok::<Vec<crate::games::GameLibraryInfo>, String>(libraries)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn migrate_game(
    platform: crate::games::GamePlatform,
    app_id: String,
    source_path: String,
    target_disk: String,
    known_size: Option<u64>,
    known_files: Option<usize>,
    app: AppHandle,
    migration_db: tauri::State<'_, MigrationDb>,
) -> Result<MigrationResult, String> {
    if matches!(platform, crate::games::GamePlatform::MicrosoftStore) {
        return Err(
            "Microsoft Store / WindowsApps 游戏请用「在系统设置中迁移」按钮".to_string(),
        );
    }

    tracing::info!(
        "[games-migrate] platform={} app_id={} source={} target_disk={}",
        platform.label(),
        app_id,
        source_path,
        target_disk
    );

    let migrator = FileMigrator::new();
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
            &source_path,
            &target_disk,
            LinkType::Junction,
            known_stats,
            Some(progress_callback),
        )
        .await
        .map_err(|e| e.to_string())?;

    if !result.success {
        tracing::warn!(
            "[games-migrate] failed platform={} app_id={} error={:?}",
            platform.label(),
            app_id,
            result.error
        );
        return Err(result
            .error
            .take()
            .unwrap_or_else(|| "游戏迁移失败".to_string()));
    }

    let lt_str = format!("game:{}:{}", platform.label(), app_id);
    if let Ok(id) = migration_db.insert_migration(
        &result.source_path,
        &result.target_path,
        &lt_str,
        result.file_size,
    ) {
        result.migration_id = id;
    }
    crate::safety::invalidate_safety_cache();
    tracing::info!(
        "[games-migrate] completed platform={} app_id={} source={} target={} size={} duration_ms={} history_id={}",
        platform.label(),
        app_id,
        result.source_path,
        result.target_path,
        result.file_size,
        result.duration_ms,
        result.migration_id
    );
    Ok(result)
}

#[tauri::command]
pub fn open_native_migration_ui(
    platform: crate::games::GamePlatform,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let target = match platform {
            crate::games::GamePlatform::MicrosoftStore | crate::games::GamePlatform::GamePass => {
                "ms-settings:appsfeatures"
            }
            crate::games::GamePlatform::Steam => "ms-settings:appsfeatures",
            crate::games::GamePlatform::Epic => "ms-settings:appsfeatures",
        };
        Command::new("explorer.exe")
            .arg(target)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = platform;
    }
    Ok(())
}

#[tauri::command]
pub async fn scan_junk_files(
    app: tauri::AppHandle,
    categories: Option<Vec<crate::junk::rules::JunkCategory>>,
) -> Result<crate::junk::scanner::JunkScanResult, String> {
    let t_cmd_start = std::time::Instant::now();
    tracing::info!(target: "junk_perf", "[scan_junk_files] T0 enter");

    let t_admin = std::time::Instant::now();
    let is_admin = is_elevated();
    tracing::info!(
        target: "junk_perf",
        "[scan_junk_files] T1 is_elevated took {}ms result={}",
        t_admin.elapsed().as_millis(),
        is_admin,
    );

    let app_handle = app.clone();
    let cb: crate::junk::scanner::JunkScanProgressCallback = std::sync::Arc::new(move |p| {
        let _ = tauri::Emitter::emit(&app_handle, "junk-scan-progress", p);
    });
    tracing::info!(
        target: "junk_perf",
        "[scan_junk_files] T2 about to spawn_blocking categories={:?}",
        categories.as_ref().map(|v| v.len()),
    );

    let t_spawn = std::time::Instant::now();
    let result = tokio::task::spawn_blocking(move || {
        let t_inside = std::time::Instant::now();
        tracing::info!(
            target: "junk_perf",
            "[scan_junk_files] T3 inside spawn_blocking, dispatch took {}ms",
            t_spawn.elapsed().as_millis(),
        );
        let r = crate::junk::scanner::scan_junk_blocking(categories, is_admin, Some(cb));
        tracing::info!(
            target: "junk_perf",
            "[scan_junk_files] T4 spawn_blocking body finished, internal_total={}ms",
            t_inside.elapsed().as_millis(),
        );
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
    .map_err(|e| e.to_string())?;
    let blocking_ms = t_spawn.elapsed().as_millis();

    // Estimate serialization cost separately from IPC. Tauri serializes the
    // command return value before posting it to the webview; if `items` is
    // huge this can be a non-trivial slice of total command latency.
    let t_ser = std::time::Instant::now();
    let json_size = serde_json::to_vec(&result).map(|v| v.len()).unwrap_or(0);
    let ser_ms = t_ser.elapsed().as_millis();

    tracing::info!(
        target: "junk_perf",
        "[scan_junk_files] T5 exit cmd_total={}ms blocking={}ms items={} total_size={} reported_scan_ms={} estimate_serialize_ms={} json_bytes={}",
        t_cmd_start.elapsed().as_millis(),
        blocking_ms,
        result.items.len(),
        result.total_size,
        result.scan_duration_ms,
        ser_ms,
        json_size,
    );

    Ok(result)
}

#[tauri::command]
pub async fn clean_junk_files(
    app: tauri::AppHandle,
    paths: Vec<String>,
    to_recycle_bin: bool,
) -> Result<crate::junk::cleaner::JunkCleanResult, String> {
    let app_handle = app.clone();
    let cb: crate::junk::cleaner::JunkCleanProgressCallback = std::sync::Arc::new(move |p| {
        let _ = tauri::Emitter::emit(&app_handle, "junk-clean-progress", p);
    });

    crate::junk::cleaner::clean_junk_paths(paths, to_recycle_bin, Some(cb))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_reclaim_opportunities() -> Result<Vec<crate::system_reclaim::ReclaimOpportunity>, String> {
    crate::system_reclaim::get_reclaim_opportunities()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn execute_reclaim(id: String, force: bool) -> Result<crate::system_reclaim::ReclaimResult, String> {
    match id.as_str() {
        "windows_update" => crate::system_reclaim::cleanup_windows_update(force)
            .await
            .map_err(|e| e.to_string()),
        "delivery_optimization" => crate::system_reclaim::cleanup_delivery_optimization()
            .await
            .map_err(|e| e.to_string()),
        "hibernation" => crate::system_reclaim::disable_hibernation()
            .await
            .map_err(|e| e.to_string()),
        "restore_points" => crate::system_reclaim::cleanup_restore_points(!force)
            .await
            .map_err(|e| e.to_string()),
        "pagefile" => Err("pagefile 迁移需要指定目标磁盘，请使用专用接口".into()),
        _ => Err(format!("未知的回收操作: {}", id)),
    }
}

#[tauri::command]
pub async fn get_known_folders() -> Result<Vec<crate::folder_redirect::KnownFolderInfo>, String> {
    crate::folder_redirect::get_known_folders()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn relocate_folder(
    folder_id: String,
    target_path: String,
    move_files: bool,
    app: tauri::AppHandle,
    migration_db: tauri::State<'_, MigrationDb>,
) -> Result<crate::folder_redirect::RedirectResult, String> {
    let app_handle = app.clone();
    let cb: crate::folder_redirect::RedirectProgressCallback = std::sync::Arc::new(move |p| {
        let _ = tauri::Emitter::emit(&app_handle, "redirect-progress", p);
    });

    let result = crate::folder_redirect::relocate_known_folder(
        &folder_id,
        std::path::Path::new(&target_path),
        move_files,
        Some(cb),
    )
    .await
    .map_err(|e| e.to_string())?;

    if result.success && (result.moved_bytes > 0 || result.moved_files > 0) {
        if let Err(err) = migration_db.insert_redirect_record(
            &folder_id,
            &result.source_path,
            &result.target_path,
            result.moved_bytes,
        ) {
            tracing::warn!(
                "[folder-redirect] failed to insert history folder_id={} target={} error={err}",
                folder_id,
                target_path
            );
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn restore_folder(
    folder_id: String,
    app: tauri::AppHandle,
) -> Result<crate::folder_redirect::RedirectResult, String> {
    let app_handle = app.clone();
    let cb: crate::folder_redirect::RedirectProgressCallback = std::sync::Arc::new(move |p| {
        let _ = tauri::Emitter::emit(&app_handle, "redirect-progress", p);
    });

    crate::folder_redirect::restore_known_folder(&folder_id, Some(cb))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn relocate_temp(target_path: String) -> Result<crate::folder_redirect::RedirectResult, String> {
    crate::folder_redirect::relocate_temp(std::path::Path::new(&target_path))
        .await
        .map_err(|e| e.to_string())
}


#[tauri::command]
pub fn get_space_breakdown(
    root_path: String,
    scanner: tauri::State<'_, DiskScanner>,
) -> Result<crate::scanner::space_breakdown::SpaceBreakdown, String> {
    let disk_info = get_disk_info_for_path(&root_path)?;
    scanner
        .with_indexed(&root_path, |indexed| {
            crate::scanner::space_breakdown::analyze_space_breakdown(
                indexed,
                disk_info.total_space,
                disk_info.used_space,
            )
        })
        .ok_or_else(|| "尚未扫描，请先执行深度扫描".to_string())
}

#[tauri::command]
pub fn explain_file(path: String) -> Result<crate::scanner::space_breakdown::FileExplanation, String> {
    Ok(crate::scanner::space_breakdown::explain_path(&path))
}

#[tauri::command]
pub fn get_relocatable_programs(
    root_path: String,
    scanner: tauri::State<'_, DiskScanner>,
) -> Result<Vec<crate::scanner::space_breakdown::RelocatableProgram>, String> {
    scanner
        .with_indexed(&root_path, |indexed| {
            crate::scanner::space_breakdown::get_relocatable_programs(indexed)
        })
        .ok_or_else(|| "尚未扫描，请先执行深度扫描".to_string())
}

#[tauri::command]
pub async fn get_balance_suggestion(
    root_path: String,
    scanner: tauri::State<'_, DiskScanner>,
) -> Result<crate::scanner::space_breakdown::BalanceSuggestion, String> {
    let disks = get_disk_info().await?;
    let breakdown = scanner
        .with_indexed(&root_path, |indexed| {
            let info = get_disk_info_for_path_inner(&root_path, &disks);
            crate::scanner::space_breakdown::analyze_space_breakdown(
                indexed,
                info.map(|d| d.total_space).unwrap_or(0),
                info.map(|d| d.used_space).unwrap_or(0),
            )
        })
        .ok_or_else(|| "尚未扫描，请先执行深度扫描".to_string())?;
    Ok(crate::scanner::space_breakdown::suggest_balance(&disks, &breakdown))
}

fn get_disk_info_for_path(root_path: &str) -> Result<DiskInfo, String> {
    let drive = root_path.chars().next().unwrap_or('C').to_ascii_uppercase();
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

        let drive_path = format!("{}:\\", drive);
        let path_wide: Vec<u16> = std::ffi::OsStr::new(&drive_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut total_bytes = 0u64;
        let mut free_bytes = 0u64;
        unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(path_wide.as_ptr()),
                None,
                Some(&mut total_bytes),
                Some(&mut free_bytes),
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(DiskInfo {
            drive_letter: format!("{}:", drive),
            label: String::new(),
            file_system: String::new(),
            total_space: total_bytes,
            free_space: free_bytes,
            used_space: total_bytes.saturating_sub(free_bytes),
            usage_percent: if total_bytes > 0 {
                ((total_bytes - free_bytes) as f64 / total_bytes as f64) * 100.0
            } else {
                0.0
            },
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = drive;
        Err("仅支持 Windows".to_string())
    }
}

fn get_disk_info_for_path_inner<'a>(root_path: &str, disks: &'a [DiskInfo]) -> Option<&'a DiskInfo> {
    let drive = root_path.chars().next()?.to_ascii_uppercase();
    let letter = format!("{}:", drive);
    disks.iter().find(|d| d.drive_letter.eq_ignore_ascii_case(&letter))
}
