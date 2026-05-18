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
use scanner::DiskScanner;
use session::ScanSessionRegistry;
use std::sync::Arc;

fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    // 默认 info 级别，可用 RUST_LOG 环境变量覆盖。
    // 例：`set RUST_LOG=cdrive_cleaner_lib=debug,info` 看本 crate debug，其它默认 info。
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
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
        .and_then(|p| ScanCacheDb::new(p.to_string_lossy().as_ref()).map_err(Into::into))
        .expect("Failed to open scan cache database");
    scan_cache_db
        .purge_legacy_scan_types()
        .expect("Failed to purge legacy scan cache types");

    let migration_db = utils::get_migrations_db_path()
        .and_then(|p| MigrationDb::new(p.to_string_lossy().as_ref()).map_err(Into::into))
        .expect("Failed to open migration database");

    let space_history_db = utils::get_space_history_db_path()
        .and_then(|p| SpaceHistoryDb::new(p.to_string_lossy().as_ref()).map_err(Into::into))
        .expect("Failed to open space history database");
    if let Err(err) = space_history_db.purge_old(90) {
        tracing::warn!("[space-history] failed to purge old snapshots: {err}");
    }

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
        .manage(scan_cache_db)
        .manage(migration_db)
        .manage(space_history_db)
        .invoke_handler(tauri::generate_handler![
            commands::scan_disk_deep,
            commands::get_directory_snapshot,
            commands::analyze_smart_groups,
            commands::reveal_in_explorer,
            commands::cancel_scan,
            commands::scan_directory_files,
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
            commands::delete_duplicate_files,
            commands::detect_game_libraries,
            commands::migrate_game,
            commands::open_native_migration_ui,
            commands::scan_junk_files,
            commands::clean_junk_files,
            commands::get_reclaim_opportunities,
            commands::execute_reclaim,
            commands::get_known_folders,
            commands::relocate_folder,
            commands::relocate_temp,
            commands::get_space_breakdown,
            commands::explain_file,
            commands::get_relocatable_programs,
            commands::get_balance_suggestion,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
