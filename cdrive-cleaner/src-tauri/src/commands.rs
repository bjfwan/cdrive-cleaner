use crate::scanner::{DiskScanner, file_info::{ScanResult, FileInfo}};
use crate::migration::{FileMigrator, LinkType, file_migrator::MigrationResult};
use tauri::{AppHandle, Emitter};

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
pub async fn scan_disk(path: String, app: AppHandle) -> Result<ScanResult, String> {
    let scanner = DiskScanner::new();
    scanner.scan(&path, app).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_disk_deep(path: String, app: AppHandle) -> Result<ScanResult, String> {
    let scanner = DiskScanner::new();
    scanner.scan_deep(&path, app).await.map_err(|e| e.to_string())
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
) -> Result<MigrationResult, String> {
    let migrator = FileMigrator::new();
    let link_type = link_type.unwrap_or(LinkType::Auto);
    migrator.migrate(&source, &target_disk, link_type)
        .await
        .map_err(|e| e.to_string())
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
