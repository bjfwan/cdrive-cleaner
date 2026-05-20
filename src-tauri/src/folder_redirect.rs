use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::process::Command;
use windows::core::PWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures,
    FOLDERID_Videos, SHGetKnownFolderPath, SHSetKnownFolderPath, KNOWN_FOLDER_FLAG,
};

#[cfg(target_os = "windows")]
fn hidden_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    Command::from(command)
}

#[cfg(not(target_os = "windows"))]
fn hidden_command(program: &str) -> Command {
    Command::new(program)
}

#[derive(Serialize, Clone)]
pub struct KnownFolderInfo {
    pub id: String,
    pub display_name: String,
    pub current_path: String,
    pub default_path: String,
    pub suggested_target_path: String,
    pub size_bytes: u64,
    pub is_on_system_drive: bool,
    pub is_default_location: bool,
}

#[derive(Serialize, Clone)]
pub struct RedirectResult {
    pub success: bool,
    pub moved_files: u64,
    pub moved_bytes: u64,
    pub source_path: String,
    pub target_path: String,
    pub requires_reboot: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RedirectProgress {
    pub folder_id: String,
    pub folder_name: String,
    pub source_path: String,
    pub target_path: String,
    pub status: String,
    pub progress_percent: f64,
    pub moved_files: u64,
    pub moved_bytes: u64,
}

pub type RedirectProgressCallback = std::sync::Arc<dyn Fn(RedirectProgress) + Send + Sync>;

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
    dir_stats_blocking(path).0
}

fn dir_stats_blocking(path: &Path) -> (u64, u64) {
    if !path.exists() {
        return (0, 0);
    }
    let mut size = 0u64;
    let mut count = 0u64;
    for entry in jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Ok(metadata) = entry.metadata() {
                size = size.saturating_add(metadata.len());
                count = count.saturating_add(1);
            }
        }
    }
    (size, count)
}

fn is_on_c_drive(path: &str) -> bool {
    path.to_lowercase().starts_with("c:")
}

fn default_path_for(relative: &str) -> String {
    let user_profile = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\User".into());
    format!("{}\\{}", user_profile, relative)
}

fn target_path_for(base: &Path, def: &FolderDef) -> PathBuf {
    let base_str = base.to_string_lossy();
    let trimmed = base_str.trim_end_matches(['\\', '/']);
    let is_drive_root = trimmed.len() == 2 && trimmed.as_bytes().get(1) == Some(&b':');
    let base_name = base
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    if is_drive_root {
        return PathBuf::from(format!("{}\\{}", trimmed, def.default_relative));
    }

    if !base_name.eq_ignore_ascii_case(def.default_relative) {
        base.join(def.default_relative)
    } else {
        base.to_path_buf()
    }
}

fn suggested_target_path(def: &FolderDef) -> String {
    for drive in 'D'..='Z' {
        let root = format!("{}:\\", drive);
        if Path::new(&root).exists() {
            return target_path_for(Path::new(&root), def)
                .to_string_lossy()
                .to_string();
        }
    }
    target_path_for(Path::new("D:\\"), def)
        .to_string_lossy()
        .to_string()
}

fn path_is_drive_root(path: &Path) -> bool {
    let path = path.to_string_lossy();
    let trimmed = path.trim_end_matches(['\\', '/']);
    trimmed.len() == 2 && trimmed.as_bytes().get(1) == Some(&b':')
}

fn safe_size_blocking(path: &Path, def: Option<&FolderDef>) -> u64 {
    if path_is_drive_root(path) {
        return def
            .map(|d| dir_size_blocking(&target_path_for(path, d)))
            .unwrap_or(0);
    }
    dir_size_blocking(path)
}

fn emit_redirect_progress(
    callback: Option<&RedirectProgressCallback>,
    folder_id: &str,
    folder_name: &str,
    source_path: &Path,
    target_path: &Path,
    status: &str,
    progress_percent: f64,
    moved_files: u64,
    moved_bytes: u64,
) {
    if let Some(callback) = callback {
        callback(RedirectProgress {
            folder_id: folder_id.to_string(),
            folder_name: folder_name.to_string(),
            source_path: source_path.to_string_lossy().to_string(),
            target_path: target_path.to_string_lossy().to_string(),
            status: status.to_string(),
            progress_percent,
            moved_files,
            moved_bytes,
        });
    }
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
            let size = safe_size_blocking(Path::new(&current), Some(&def));
            let suggested_target_path = suggested_target_path(&def);

            folders.push(KnownFolderInfo {
                id: def.id.into(),
                display_name: def.display_name.into(),
                current_path: current.clone(),
                default_path: default.clone(),
                suggested_target_path,
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
            suggested_target_path: r"D:\Temp".to_string(),
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
    on_progress: Option<RedirectProgressCallback>,
) -> Result<RedirectResult> {
    let defs = known_folder_defs();
    let def = defs
        .iter()
        .find(|d| d.id == folder_id)
        .context("未知的 folder_id")?;

    let current = get_known_folder_path(&def.folder_id)?;
    let target = target_path_for(target_path, def);

    if current
        .to_string_lossy()
        .eq_ignore_ascii_case(&target.to_string_lossy())
    {
        return Ok(RedirectResult {
            success: true,
            moved_files: 0,
            moved_bytes: 0,
            source_path: current.to_string_lossy().to_string(),
            target_path: target.to_string_lossy().to_string(),
            requires_reboot: false,
            message: format!("{} 已经位于 {}", def.display_name, target.display()),
        });
    }

    emit_redirect_progress(
        on_progress.as_ref(),
        def.id,
        def.display_name,
        &current,
        &target,
        "preparing",
        5.0,
        0,
        0,
    );

    if !target.exists() {
        std::fs::create_dir_all(&target).context("创建目标目录失败")?;
    }

    let mut moved_files = 0u64;
    let mut moved_bytes = 0u64;

    if move_files {
        let source_str = current.to_string_lossy().to_string();
        let target_str = target.to_string_lossy().to_string();
        let (source_bytes_before, source_files_before) = dir_stats_blocking(&current);

        emit_redirect_progress(
            on_progress.as_ref(),
            def.id,
            def.display_name,
            &current,
            &target,
            "moving",
            20.0,
            0,
            0,
        );

        let output = hidden_command("robocopy")
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

        let code = output.status.code().unwrap_or(16);
        if code >= 8 {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(anyhow!(
                "robocopy 迁移失败，退出码 {}: {}{}",
                code,
                stderr,
                stdout
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (files, bytes) = parse_robocopy_summary(&stdout);
        moved_files = if files > 0 { files } else { source_files_before };
        moved_bytes = if bytes > 0 { bytes } else { source_bytes_before };
        emit_redirect_progress(
            on_progress.as_ref(),
            def.id,
            def.display_name,
            &current,
            &target,
            "moving",
            80.0,
            moved_files,
            moved_bytes,
        );
    }

    emit_redirect_progress(
        on_progress.as_ref(),
        def.id,
        def.display_name,
        &current,
        &target,
        "applying",
        90.0,
        moved_files,
        moved_bytes,
    );

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

    emit_redirect_progress(
        on_progress.as_ref(),
        def.id,
        def.display_name,
        &current,
        &target,
        "done",
        100.0,
        moved_files,
        moved_bytes,
    );

    Ok(RedirectResult {
        success: true,
        moved_files,
        moved_bytes,
        source_path: current.to_string_lossy().to_string(),
        target_path: target.to_string_lossy().to_string(),
        requires_reboot: false,
        message: format!(
            "{} 已重定向到 {}",
            def.display_name,
            target.display()
        ),
    })
}

pub fn folder_id_from_redirect_record(
    link_type: &str,
    source_path: &Path,
    target_path: &Path,
) -> Option<String> {
    if let Some((prefix, id)) = link_type.split_once(':') {
        if prefix == "KnownFolderRedirect" && !id.trim().is_empty() {
            return Some(id.trim().to_string());
        }
    }

    let source = source_path.to_string_lossy();
    let target = target_path.to_string_lossy();
    known_folder_defs()
        .into_iter()
        .find(|def| {
            let default = default_path_for(def.default_relative);
            source.eq_ignore_ascii_case(&default)
                || target.ends_with(def.default_relative)
                || source.ends_with(def.default_relative)
        })
        .map(|def| def.id.to_string())
}

pub async fn rollback_known_folder_redirect(
    folder_id: &str,
    source_path: &Path,
    target_path: &Path,
    on_progress: Option<RedirectProgressCallback>,
) -> Result<crate::migration::file_migrator::RollbackResult> {
    let start = Instant::now();
    let defs = known_folder_defs();
    let def = defs
        .iter()
        .find(|d| d.id == folder_id)
        .ok_or_else(|| anyhow!("未知的 folder_id: {folder_id}"))?;

    let current = get_known_folder_path(&def.folder_id)?;
    if !current
        .to_string_lossy()
        .eq_ignore_ascii_case(&target_path.to_string_lossy())
    {
        return Ok(crate::migration::file_migrator::RollbackResult {
            success: false,
            source_path: source_path.to_string_lossy().to_string(),
            target_path: target_path.to_string_lossy().to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some(format!(
                "Known folder current path changed: {}",
                current.display()
            )),
        });
    }

    if !current.exists() {
        return Ok(crate::migration::file_migrator::RollbackResult {
            success: false,
            source_path: source_path.to_string_lossy().to_string(),
            target_path: target_path.to_string_lossy().to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some("Target not found".to_string()),
        });
    }

    emit_redirect_progress(
        on_progress.as_ref(),
        def.id,
        def.display_name,
        &current,
        source_path,
        "restoring",
        10.0,
        0,
        0,
    );

    let restored = relocate_known_folder(folder_id, source_path, true, on_progress).await?;
    if !restored.success {
        return Ok(crate::migration::file_migrator::RollbackResult {
            success: false,
            source_path: source_path.to_string_lossy().to_string(),
            target_path: target_path.to_string_lossy().to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some(restored.message),
        });
    }

    Ok(crate::migration::file_migrator::RollbackResult {
        success: true,
        source_path: source_path.to_string_lossy().to_string(),
        target_path: target_path.to_string_lossy().to_string(),
        duration_ms: start.elapsed().as_millis() as u64,
        error: None,
    })
}

pub async fn restore_known_folder(
    folder_id: &str,
    on_progress: Option<RedirectProgressCallback>,
) -> Result<RedirectResult> {
    if folder_id == "temp" {
        let temp_default = format!(
            "{}\\AppData\\Local\\Temp",
            std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\User".into())
        );
        return relocate_temp(Path::new(&temp_default)).await;
    }

    let defs = known_folder_defs();
    let def = defs
        .iter()
        .find(|d| d.id == folder_id)
        .context("未知的 folder_id")?;
    let default_path = PathBuf::from(default_path_for(def.default_relative));
    relocate_known_folder(folder_id, &default_path, true, on_progress).await
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
        source_path: String::new(),
        target_path: target_str.clone(),
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
