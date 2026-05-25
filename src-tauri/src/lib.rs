#![allow(
    clippy::too_many_arguments,
    clippy::new_without_default,
    clippy::type_complexity,
)]

// === Track B ===
pub mod ai;
// === /Track B ===

// === Track A ===
pub mod scheduler;
pub mod tray;
// === /Track A ===

mod commands;
pub mod database;
pub mod diagnostics;
pub mod folder_redirect;
pub mod games;
pub mod migration;
pub mod pending_intent;
pub mod safety;
pub mod scanner;
pub mod session;
pub mod system_reclaim;
mod utils;
mod winfs;
pub mod junk;

#[cfg(any(test, feature = "bench"))]
pub mod bench;

use database::{MigrationDb, ScanCacheDb, SpaceHistoryDb};
use scanner::{duplicates::DuplicateScanRegistry, DiskScanner};
use session::ScanSessionRegistry;
use std::sync::Arc;

static LOG_GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> =
    std::sync::OnceLock::new();

fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    // 默认 info 级别，可用 RUST_LOG 环境变量覆盖。
    // 例：`set RUST_LOG=cdrive_cleaner_lib=debug,info` 看本 crate debug，其它默认 info。
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    if let Ok(log_dir) = utils::get_logs_dir() {
        let file_appender = tracing_appender::rolling::daily(log_dir, "cdrive-cleaner.log");
        let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
        let _ = LOG_GUARD.set(guard);
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_target(true).with_level(true))
            .with(
                fmt::layer()
                    .with_writer(file_writer)
                    .with_ansi(false)
                    .with_target(true)
                    .with_level(true),
            )
            .try_init();
        return;
    }

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true).with_level(true))
        .try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tracing::info!("CDrive Cleaner 启动");
    let scan_cache_db = utils::get_scan_cache_db_path()
        .and_then(|p| ScanCacheDb::new(p.to_string_lossy().as_ref()))
        .expect("Failed to open scan cache database");
    scan_cache_db
        .purge_legacy_scan_types()
        .expect("Failed to purge legacy scan cache types");

    let migration_db = utils::get_migrations_db_path()
        .and_then(|p| MigrationDb::new(p.to_string_lossy().as_ref()))
        .expect("Failed to open migration database");

    let space_history_db = utils::get_space_history_db_path()
        .and_then(|p| SpaceHistoryDb::new(p.to_string_lossy().as_ref()))
        .expect("Failed to open space history database");
    if let Err(err) = space_history_db.purge_old(90) {
        tracing::warn!("[space-history] failed to purge old snapshots: {err}");
    }

    // === Track B ===
    let ai_settings_store =
        Arc::new(ai::SettingsStore::load().expect("Failed to load settings store"));
    let ai_state = Arc::new(ai::AiState::new());
    // === /Track B ===

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // 第二实例启动：把主窗口拉到前台，立刻退出第二实例进程。
            // disk 转发暂未走 argv（前端没有 CLI 入口），按需后续扩展。
            use tauri::Manager;
            tracing::info!("[single-instance] 检测到第二实例，将主窗口聚焦后让第二实例退出");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(DiskScanner::new())
        .manage(Arc::new(ScanSessionRegistry::new()))
        .manage(Arc::new(DuplicateScanRegistry::new()))
        .manage(scan_cache_db)
        .manage(migration_db)
        .manage(space_history_db)
        // === Track B ===
        .manage(Arc::clone(&ai_settings_store))
        .manage(Arc::clone(&ai_state))
        // === /Track B ===
        // === Track A ===
        .manage(scheduler::SchedulerState::new())
        // === /Track A ===
        .setup({
            let ai_settings_store = Arc::clone(&ai_settings_store);
            let ai_state = Arc::clone(&ai_state);
            move |app| {
                // === Track B === scheduler-scan-done listener
                use tauri::Emitter;
                use tauri::Listener;
                use tauri::Manager;
                let app_handle = app.handle().clone();
                let settings_for_listener = Arc::clone(&ai_settings_store);
                let ai_state_for_listener = Arc::clone(&ai_state);
                app.listen("scheduler-scan-done", move |event| {
                    let raw = event.payload();
                    let scan_id = match serde_json::from_str::<serde_json::Value>(raw) {
                        Ok(v) if v.is_string() => {
                            v.as_str().unwrap_or_default().to_string()
                        }
                        Ok(v) => match v
                            .get("scan_id")
                            .or_else(|| v.get("root_path"))
                            .and_then(|f| f.as_str())
                        {
                            Some(s) => s.to_string(),
                            None => {
                                tracing::warn!(
                                    "[ai] scheduler-scan-done missing scan_id/root_path field"
                                );
                                return;
                            }
                        },
                        Err(err) => {
                            tracing::warn!(
                                "[ai] scheduler-scan-done payload not JSON: {err}"
                            );
                            return;
                        }
                    };
                    if scan_id.is_empty() {
                        return;
                    }

                    let ai_value = settings_for_listener.get("ai");
                    let ai_settings: ai::AiSettings =
                        serde_json::from_value(ai_value).unwrap_or_default();
                    if !ai_settings.enabled || !ai_settings.categories.any_enabled() {
                        return;
                    }

                    let scanner_state = match app_handle.try_state::<scanner::DiskScanner>() {
                        Some(s) => s,
                        None => {
                            tracing::warn!("[ai] DiskScanner state not managed");
                            return;
                        }
                    };
                    let scan = match scanner_state
                        .get_directory_snapshot(&scan_id, &scan_id)
                    {
                        Some(s) => s,
                        None => {
                            tracing::warn!(
                                "[ai] scheduler-scan-done: no scan result for {scan_id}"
                            );
                            return;
                        }
                    };

                    let (total, free) = {
                        #[cfg(windows)]
                        {
                            use std::os::windows::ffi::OsStrExt;
                            use windows::core::PCWSTR;
                            use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
                            let drive_root = if scan_id.len() >= 2
                                && scan_id.chars().nth(1) == Some(':')
                            {
                                format!("{}:\\", &scan_id[..1])
                            } else {
                                scan_id.clone()
                            };
                            let wide: Vec<u16> = std::ffi::OsStr::new(&drive_root)
                                .encode_wide()
                                .chain(std::iter::once(0))
                                .collect();
                            let mut total: u64 = 0;
                            let mut free: u64 = 0;
                            unsafe {
                                if GetDiskFreeSpaceExW(
                                    PCWSTR(wide.as_ptr()),
                                    None,
                                    Some(&mut total),
                                    Some(&mut free),
                                )
                                .is_ok()
                                {
                                    (total, free)
                                } else {
                                    (0, 0)
                                }
                            }
                        }
                        #[cfg(not(windows))]
                        {
                            (0_u64, 0_u64)
                        }
                    };

                    let prepared = ai::sanitizer::build_snapshot(
                        &scan,
                        total,
                        free,
                        &ai_settings,
                    );
                    ai_state_for_listener.store_snapshot(
                        &scan_id,
                        prepared.snapshot.clone(),
                        prepared.token_to_path,
                    );

                    let snapshot = prepared.snapshot;
                    let ai_state_for_task = Arc::clone(&ai_state_for_listener);
                    let app_for_task = app_handle.clone();
                    let scan_id_for_task = scan_id.clone();
                    let ai_settings_for_task = ai_settings.clone();

                    tauri::async_runtime::spawn(async move {
                        let res = tokio::task::spawn_blocking(move || {
                            ai::analyze(&snapshot, &ai_settings_for_task)
                        })
                        .await;
                        match res {
                            Ok(Ok(suggestions)) => {
                                ai_state_for_task.store_suggestions(&suggestions);
                                let _ = app_for_task.emit(
                                    "ai-suggestions-ready",
                                    serde_json::json!({
                                        "scan_id": scan_id_for_task,
                                        "suggestions": suggestions
                                    }),
                                );
                            }
                            Ok(Err(err)) => {
                                tracing::warn!(
                                    "[ai] auto-analyze on scheduler-scan-done failed: {err}"
                                );
                            }
                            Err(err) => {
                                tracing::error!("[ai] auto-analyze task join failed: {err}");
                            }
                        }
                    });
                });
                // === /Track B ===

                // === Track A ===
                // 托盘图标 + 关闭事件接管 + 后台调度 tick
                let track_a_handle = app.handle().clone();
                if let Err(err) = tray::setup_tray(&track_a_handle) {
                    tracing::warn!("[track-a] setup_tray failed: {err}");
                }
                tray::install_close_handler(&track_a_handle);
                scheduler::spawn_ticker(track_a_handle);
                // === /Track A ===
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_disk_deep,
            commands::get_directory_snapshot,
            commands::get_directory_children,
            commands::get_large_files,
            commands::get_large_files_page,
            commands::analyze_smart_groups,
            commands::reveal_in_explorer,
            commands::cancel_scan,
            commands::scan_directory_files,
            commands::get_directory_files_page,
            commands::scan_directory_files_page,
            commands::migrate_file,
            commands::delete_path,
            commands::get_disk_info,
            commands::analyze_migration_safety,
            commands::get_migration_history,
            commands::get_migration_stats,
            commands::rollback_migration,
            commands::save_scan_cache,
            commands::get_scan_cache,
            commands::clear_scan_cache,
            commands::get_cache_info,
            commands::delete_cache_entry,
            commands::get_scan_capabilities,
            commands::is_elevated,
            commands::restart_as_admin,
            commands::request_admin_rescan,
            commands::consume_pending_scan_intent,
            commands::exit_app,
            commands::get_space_history,
            commands::find_duplicates,
            commands::cancel_duplicate_scan,
            commands::delete_duplicate_files,
            commands::detect_game_libraries,
            commands::migrate_game,
            commands::open_native_migration_ui,
            commands::scan_junk_files,
            commands::clean_junk_files,
            commands::junk_dry_run,
            commands::junk_report_feedback,
            commands::get_reclaim_opportunities,
            commands::execute_reclaim,
            commands::get_known_folders,
            commands::relocate_folder,
            commands::restore_folder,
            commands::relocate_temp,
            commands::get_space_breakdown,
            commands::explain_file,
            commands::get_relocatable_programs,
            commands::get_balance_suggestion,
            commands::check_for_updates,
            commands::download_update,
            commands::install_update,
            commands::open_url,
            diagnostics::export_diagnostic_bundle,
            // === Track B ===
            ai::commands::cmd_ai_preview_snapshot,
            ai::commands::cmd_ai_analyze,
            ai::commands::cmd_ai_apply,
            ai::commands::cmd_settings_get,
            ai::commands::cmd_settings_set,
            // === /Track B ===
            // === Track A ===
            scheduler::cmd_scheduler_run_now,
            scheduler::cmd_scheduler_get_status,
            scheduler::cmd_scheduler_mark_scan_done,
            // === /Track A ===
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
