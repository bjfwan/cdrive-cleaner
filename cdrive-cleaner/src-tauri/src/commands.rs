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
