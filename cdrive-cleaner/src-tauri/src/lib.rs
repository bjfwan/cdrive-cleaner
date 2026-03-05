// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod scanner;
pub mod migration;
mod database;
mod commands;
pub mod safety;
pub mod cache;

use scanner::DiskScanner;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(DiskScanner::new()) // 添加全局状态
        .invoke_handler(tauri::generate_handler![
            commands::scan_disk,
            commands::scan_disk_deep,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
