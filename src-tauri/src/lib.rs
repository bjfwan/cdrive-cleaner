mod commands;
pub mod database;
pub mod diagnostics;
pub mod migration;
pub mod safety;
pub mod scanner;
mod utils;
mod winfs;

#[cfg(any(test, feature = "bench"))]
pub mod bench;

use database::{MigrationDb, ScanCacheDb};
use scanner::DiskScanner;

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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(DiskScanner::new())
        .manage(scan_cache_db)
        .manage(migration_db)
        .invoke_handler(tauri::generate_handler![
            commands::scan_disk_deep,
            commands::get_directory_snapshot,
            commands::analyze_smart_groups,
            commands::reveal_in_explorer,
            commands::cancel_scan,
            commands::scan_directory_files,
            commands::migrate_file,
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
            commands::exit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
