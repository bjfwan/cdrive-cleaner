// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod scanner;
mod migration;
mod database;
mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::scan_disk,
            commands::scan_disk_deep,
            commands::scan_directory_files,
            commands::migrate_file,
            commands::get_disk_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
