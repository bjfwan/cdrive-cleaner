use crate::scanner::{DiskScanner, file_info::ScanResult};
use crate::migration::{FileMigrator, LinkType, file_migrator::MigrationResult};

#[tauri::command]
pub async fn scan_disk(path: String) -> Result<ScanResult, String> {
    let scanner = DiskScanner::new();
    scanner.scan(&path).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn migrate_file(
    source: String,
    target_disk: String,
    link_type: Option<LinkType>,
) -> Result<MigrationResult, String> {
    let migrator = FileMigrator::new();
    let link_type = link_type.unwrap_or(LinkType::Auto);
    migrator.migrate(&source, &target_disk, link_type)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_disk_info() -> Result<Vec<DiskInfo>, String> {
    Ok(vec![])
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
