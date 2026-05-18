use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use windows::core::PWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures,
    FOLDERID_Videos, SHGetKnownFolderPath, SHSetKnownFolderPath, KNOWN_FOLDER_FLAG,
};

#[derive(Serialize, Clone)]
pub struct KnownFolderInfo {
    pub id: String,
    pub display_name: String,
    pub current_path: String,
    pub default_path: String,
    pub size_bytes: u64,
    pub is_on_system_drive: bool,
    pub is_default_location: bool,
}

#[derive(Serialize, Clone)]
pub struct RedirectResult {
    pub success: bool,
    pub moved_files: u64,
    pub moved_bytes: u64,
    pub requires_reboot: bool,
    pub message: String,
}

struct FolderDef {
    id: &'static str,
    display_name: &'static str,
    folder_id: windows::core::GUID,
    default_relative: &'static str,
}

fn known_folder_defs() -> Vec<FolderDef> {
    vec![
        FolderDef {
            id: "downloads",
            display_name: "下载",
            folder_id: FOLDERID_Downloads,
            default_relative: "Downloads",
        },
        FolderDef {
            id: "documents",
            display_name: "文档",
            folder_id: FOLDERID_Documents,
            default_relative: "Documents",
        },
        FolderDef {
            id: "desktop",
            display_name: "桌面",
            folder_id: FOLDERID_Desktop,
            default_relative: "Desktop",
        },
        FolderDef {
            id: "pictures",
            display_name: "图片",
            folder_id: FOLDERID_Pictures,
            default_relative: "Pictures",
        },
        FolderDef {
            id: "videos",
            display_name: "视频",
            folder_id: FOLDERID_Videos,
            default_relative: "Videos",
        },
        FolderDef {
            id: "music",
            display_name: "音乐",
            folder_id: FOLDERID_Music,
            default_relative: "Music",
        },
    ]
}

fn get_known_folder_path(folder_id: &windows::core::GUID) -> Result<PathBuf> {
    unsafe {
        let path: PWSTR =
            SHGetKnownFolderPath(folder_id, KNOWN_FOLDER_FLAG(0), HANDLE::default())
                .context("SHGetKnownFolderPath 失败")?;
        let result = path.to_string().context("路径转换失败")?;
        windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as *const _));
        Ok(PathBuf::from(result))
    }
}

fn dir_size_blocking(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn is_on_c_drive(path: &str) -> bool {
    path.to_lowercase().starts_with("c:")
}

fn default_path_for(relative: &str) -> String {
    let user_profile = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\User".into());
    format!("{}\\{}", user_profile, relative)
}

/// 获取所有 Known Folder 信息（含 TEMP）
pub async fn get_known_folders() -> Result<Vec<KnownFolderInfo>> {
    tokio::task::spawn_blocking(|| {
        let mut folders = Vec::new();

        for def in known_folder_defs() {
            let current = get_known_folder_path(&def.folder_id)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let default = default_path_for(def.default_relative);
            let size = dir_size_blocking(Path::new(&current));

            folders.push(KnownFolderInfo {
                id: def.id.into(),
                display_name: def.display_name.into(),
                current_path: current.clone(),
                default_path: default.clone(),
                size_bytes: size,
                is_on_system_drive: is_on_c_drive(&current),
                is_default_location: current.eq_ignore_ascii_case(&default),
            });
        }

        let temp_path = std::env::var("TEMP")
            .or_else(|_| std::env::var("TMP"))
            .unwrap_or_else(|_| r"C:\Windows\Temp".into());
        let temp_default = format!(
            "{}\\AppData\\Local\\Temp",
            std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\User".into())
        );
        let temp_size = dir_size_blocking(Path::new(&temp_path));

        folders.push(KnownFolderInfo {
            id: "temp".into(),
            display_name: "临时文件夹".into(),
            current_path: temp_path.clone(),
            default_path: temp_default.clone(),
            size_bytes: temp_size,
            is_on_system_drive: is_on_c_drive(&temp_path),
            is_default_location: temp_path.eq_ignore_ascii_case(&temp_default),
        });

        Ok(folders)
    })
    .await
    .context("spawn_blocking 失败")?
}

/// 重定向 Known Folder 到新路径
pub async fn relocate_known_folder(
    folder_id: &str,
    target_path: &Path,
    move_files: bool,
) -> Result<RedirectResult> {
    let defs = known_folder_defs();
    let def = defs
        .iter()
        .find(|d| d.id == folder_id)
        .context("未知的 folder_id")?;

    let current = get_known_folder_path(&def.folder_id)?;
    let target = target_path.to_path_buf();

    if !target.exists() {
        std::fs::create_dir_all(&target).context("创建目标目录失败")?;
    }

    let mut moved_files = 0u64;
    let mut moved_bytes = 0u64;

    if move_files {
        let source_str = current.to_string_lossy().to_string();
        let target_str = target.to_string_lossy().to_string();

        let output = Command::new("robocopy")
            .args([
                &source_str,
                &target_str,
                "/E",
                "/MOVE",
                "/R:1",
                "/W:1",
                "/NP",
                "/NFL",
                "/NDL",
            ])
            .output()
            .await
            .context("无法执行 robocopy")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (files, bytes) = parse_robocopy_summary(&stdout);
        moved_files = files;
        moved_bytes = bytes;
    }

    let guid = def.folder_id;
    let target_clone = target.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        unsafe {
            let wide: Vec<u16> = target_clone
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            SHSetKnownFolderPath(
                &guid,
                0u32,
                HANDLE::default(),
                windows::core::PCWSTR(wide.as_ptr()),
            )
            .context("SHSetKnownFolderPath 失败")?;

            use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ALLEVENTS, SHCNF_IDLIST};
            SHChangeNotify(SHCNE_ALLEVENTS, SHCNF_IDLIST, None, None);
        }
        Ok(())
    })
    .await
    .context("spawn_blocking 失败")??;

    Ok(RedirectResult {
        success: true,
        moved_files,
        moved_bytes,
        requires_reboot: false,
        message: format!(
            "{} 已重定向到 {}",
            def.display_name,
            target.display()
        ),
    })
}

/// 重定向 TEMP/TMP 环境变量
pub async fn relocate_temp(target_path: &Path) -> Result<RedirectResult> {
    if !target_path.exists() {
        std::fs::create_dir_all(target_path).context("创建目标 TEMP 目录失败")?;
    }

    let target_str = target_path.to_string_lossy().to_string();
    let target_for_reg = target_str.clone();

    tokio::task::spawn_blocking(move || -> Result<()> {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let env_key = hkcu
            .open_subkey_with_flags("Environment", winreg::enums::KEY_WRITE)
            .context("无法打开 HKCU\\Environment")?;

        env_key
            .set_value("TEMP", &target_for_reg)
            .context("写入 TEMP 失败")?;
        env_key
            .set_value("TMP", &target_for_reg)
            .context("写入 TMP 失败")?;

        broadcast_setting_change();
        Ok(())
    })
    .await
    .context("spawn_blocking 失败")??;

    Ok(RedirectResult {
        success: true,
        moved_files: 0,
        moved_bytes: 0,
        requires_reboot: true,
        message: format!("TEMP/TMP 已设置为 {}，需要重启才能完全生效", target_str),
    })
}

/// 获取重定向状态
pub async fn get_redirect_status() -> Result<Vec<KnownFolderInfo>> {
    get_known_folders().await
}

fn broadcast_setting_change() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    let env_wide: Vec<u16> = OsStr::new("Environment")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(env_wide.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            5000,
            None,
        );
    }
}

fn parse_robocopy_summary(output: &str) -> (u64, u64) {
    let mut files = 0u64;
    let mut bytes = 0u64;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Files :") || trimmed.starts_with("Files:") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 5 {
                files = parts[4].parse().unwrap_or(0);
            }
        }
        if trimmed.starts_with("Bytes :") || trimmed.starts_with("Bytes:") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 5 {
                bytes = parts[4].replace(',', "").parse().unwrap_or(0);
            }
        }
    }
    (files, bytes)
}
