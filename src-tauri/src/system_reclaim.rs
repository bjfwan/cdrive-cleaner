use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::Path;
use tokio::process::Command;

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
pub struct ReclaimResult {
    pub success: bool,
    pub freed_bytes: u64,
    pub requires_reboot: bool,
    pub message: String,
}

#[derive(Serialize, Clone)]
pub struct ReclaimOpportunity {
    pub id: String,
    pub label: String,
    pub description: String,
    pub current_size: u64,
    pub reclaimable_size: u64,
    pub requires_admin: bool,
    pub requires_reboot: bool,
    pub reversible: bool,
    pub risk_level: String,
}

fn is_elevated() -> bool {
    crate::commands::is_elevated()
}

fn dir_size(path: &Path) -> u64 {
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

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn parse_dism_freed_bytes(output: &str) -> u64 {
    for line in output.lines() {
        let lower = line.to_lowercase();
        if lower.contains("mb") || lower.contains("gb") || lower.contains("kb") {
            if let Some(num) = extract_number_from_line(line) {
                if lower.contains("gb") {
                    return num * 1024 * 1024 * 1024;
                } else if lower.contains("mb") {
                    return num * 1024 * 1024;
                } else if lower.contains("kb") {
                    return num * 1024;
                }
            }
        }
    }
    0
}

fn extract_number_from_line(line: &str) -> Option<u64> {
    let mut num_str = String::new();
    for ch in line.chars() {
        if ch.is_ascii_digit() {
            num_str.push(ch);
        } else if !num_str.is_empty() && (ch == '.' || ch == ',') {
            continue;
        } else if !num_str.is_empty() {
            break;
        }
    }
    num_str.parse().ok()
}

/// 清理 Windows Update 组件存储
pub async fn cleanup_windows_update(force: bool) -> Result<ReclaimResult> {
    if !is_elevated() {
        bail!("需要管理员权限");
    }

    let dism_path = Path::new(r"C:\Windows\System32\Dism.exe");
    if !dism_path.exists() {
        return cleanup_software_distribution_fallback().await;
    }

    let mut args = vec![
        "/Online",
        "/Cleanup-Image",
        "/StartComponentCleanup",
    ];
    if force {
        args.push("/ResetBase");
    }

    let output = hidden_command("Dism.exe")
        .args(&args)
        .output()
        .await
        .context("无法启动 Dism")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        if stdout.contains("0x800f0806") || stderr.contains("0x800f0806") {
            return cleanup_software_distribution_fallback().await;
        }
        bail!("Dism 失败: {}", stderr);
    }

    let freed = parse_dism_freed_bytes(&stdout);
    Ok(ReclaimResult {
        success: true,
        freed_bytes: freed,
        requires_reboot: false,
        message: if force {
            "Windows Update 组件已清理（含 ResetBase，不可回退）".into()
        } else {
            "Windows Update 组件已清理".into()
        },
    })
}

async fn cleanup_software_distribution_fallback() -> Result<ReclaimResult> {
    let download_path = Path::new(r"C:\Windows\SoftwareDistribution\Download");
    if !download_path.exists() {
        return Ok(ReclaimResult {
            success: true,
            freed_bytes: 0,
            requires_reboot: false,
            message: "SoftwareDistribution\\Download 不存在".into(),
        });
    }

    let size_before = dir_size(download_path);
    let entries: Vec<_> = std::fs::read_dir(download_path)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            let _ = std::fs::remove_dir_all(&path);
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }

    let size_after = dir_size(download_path);
    let freed = size_before.saturating_sub(size_after);

    Ok(ReclaimResult {
        success: true,
        freed_bytes: freed,
        requires_reboot: false,
        message: "已清理 SoftwareDistribution\\Download（Dism 不可用，使用 fallback）".into(),
    })
}

/// 清理 Delivery Optimization 缓存
pub async fn cleanup_delivery_optimization() -> Result<ReclaimResult> {
    if !is_elevated() {
        bail!("需要管理员权限");
    }

    let cache_path = Path::new(
        r"C:\Windows\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache",
    );

    if !cache_path.exists() {
        return Ok(ReclaimResult {
            success: true,
            freed_bytes: 0,
            requires_reboot: false,
            message: "DeliveryOptimization 缓存目录不存在".into(),
        });
    }

    let size_before = dir_size(cache_path);
    let entries: Vec<_> = std::fs::read_dir(cache_path)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            let _ = std::fs::remove_dir_all(&path);
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }

    let size_after = dir_size(cache_path);
    let freed = size_before.saturating_sub(size_after);

    Ok(ReclaimResult {
        success: true,
        freed_bytes: freed,
        requires_reboot: false,
        message: "DeliveryOptimization 缓存已清理".into(),
    })
}

/// 禁用休眠，删除 hiberfil.sys
pub async fn disable_hibernation() -> Result<ReclaimResult> {
    if !is_elevated() {
        bail!("需要管理员权限");
    }

    let hiberfil = Path::new(r"C:\hiberfil.sys");
    let size = file_size(hiberfil);

    let output = hidden_command("powercfg")
        .args(["-h", "off"])
        .output()
        .await
        .context("无法执行 powercfg")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("powercfg -h off 失败: {}", stderr);
    }

    Ok(ReclaimResult {
        success: true,
        freed_bytes: size,
        requires_reboot: false,
        message: format!("休眠已禁用，释放 {} 字节", size),
    })
}

/// 启用休眠
pub async fn enable_hibernation() -> Result<ReclaimResult> {
    if !is_elevated() {
        bail!("需要管理员权限");
    }

    let output = hidden_command("powercfg")
        .args(["-h", "on"])
        .output()
        .await
        .context("无法执行 powercfg")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("powercfg -h on 失败: {}", stderr);
    }

    Ok(ReclaimResult {
        success: true,
        freed_bytes: 0,
        requires_reboot: false,
        message: "休眠已重新启用".into(),
    })
}

/// 清理系统还原点
pub async fn cleanup_restore_points(keep_latest: bool) -> Result<ReclaimResult> {
    if !is_elevated() {
        bail!("需要管理员权限");
    }

    let args = if keep_latest {
        vec!["delete", "shadows", "/for=C:", "/oldest", "/quiet"]
    } else {
        vec!["delete", "shadows", "/for=C:", "/all", "/quiet"]
    };

    let output = hidden_command("vssadmin")
        .args(&args)
        .output()
        .await
        .context("无法执行 vssadmin")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No items found") || stdout.contains("No items found") {
            return Ok(ReclaimResult {
                success: true,
                freed_bytes: 0,
                requires_reboot: false,
                message: "没有可删除的还原点".into(),
            });
        }
        bail!("vssadmin 失败: {}", stderr);
    }

    let freed = parse_vssadmin_freed(&stdout);
    Ok(ReclaimResult {
        success: true,
        freed_bytes: freed,
        requires_reboot: false,
        message: if keep_latest {
            "已删除旧还原点（保留最新）".into()
        } else {
            "已删除全部还原点".into()
        },
    })
}

fn parse_vssadmin_freed(output: &str) -> u64 {
    for line in output.lines() {
        let lower = line.to_lowercase();
        if lower.contains("shadow copy storage") || lower.contains("freed") {
            if let Some(num) = extract_number_from_line(line) {
                if lower.contains("gb") {
                    return num * 1024 * 1024 * 1024;
                } else if lower.contains("mb") {
                    return num * 1024 * 1024;
                }
                return num;
            }
        }
    }
    0
}

/// 将页面文件迁移到另一个磁盘
pub async fn relocate_pagefile(target_drive: &str) -> Result<ReclaimResult> {
    if !is_elevated() {
        bail!("需要管理员权限");
    }

    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let mm_key = hklm
        .open_subkey_with_flags(
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management",
            winreg::enums::KEY_READ | winreg::enums::KEY_WRITE,
        )
        .context("无法打开 Memory Management 注册表")?;

    let paging_files: Vec<String> = mm_key
        .get_value::<Vec<String>, _>("PagingFiles")
        .unwrap_or_default();

    let pagefile_path = Path::new(r"C:\pagefile.sys");
    let current_size = file_size(pagefile_path);

    let target = format!("{}:\\pagefile.sys", target_drive.trim_end_matches(':'));
    let mut new_entries: Vec<String> = Vec::new();
    let mut found_c = false;

    for entry in &paging_files {
        let lower = entry.to_lowercase();
        if lower.starts_with("c:") {
            found_c = true;
            let parts: Vec<&str> = entry.split_whitespace().collect();
            if parts.len() >= 3 {
                new_entries.push(format!("{} {} {}", target, parts[1], parts[2]));
            } else {
                new_entries.push(format!("{} 0 0", target));
            }
        } else {
            new_entries.push(entry.clone());
        }
    }

    if !found_c {
        new_entries.push(format!("{} 0 0", target));
    }

    mm_key
        .set_value("PagingFiles", &new_entries)
        .context("无法写入 PagingFiles 注册表")?;

    Ok(ReclaimResult {
        success: true,
        freed_bytes: current_size,
        requires_reboot: true,
        message: format!(
            "页面文件将在重启后迁移到 {}（当前大小 {} 字节）",
            target, current_size
        ),
    })
}

/// 获取所有可回收项目及其状态
pub async fn get_reclaim_opportunities() -> Result<Vec<ReclaimOpportunity>> {
    let mut ops = Vec::new();

    let wu_size = dir_size(Path::new(r"C:\Windows\SoftwareDistribution\Download"));
    ops.push(ReclaimOpportunity {
        id: "windows_update".into(),
        label: "Windows Update 清理".into(),
        description: "清理 Windows Update 组件存储和下载缓存".into(),
        current_size: wu_size,
        reclaimable_size: wu_size,
        requires_admin: true,
        requires_reboot: false,
        reversible: false,
        risk_level: "caution".into(),
    });

    let do_path = Path::new(
        r"C:\Windows\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache",
    );
    let do_size = dir_size(do_path);
    ops.push(ReclaimOpportunity {
        id: "delivery_optimization".into(),
        label: "Delivery Optimization 缓存".into(),
        description: "清理 Windows 更新分发优化缓存".into(),
        current_size: do_size,
        reclaimable_size: do_size,
        requires_admin: true,
        requires_reboot: false,
        reversible: false,
        risk_level: "safe".into(),
    });

    let hiberfil = Path::new(r"C:\hiberfil.sys");
    let hib_size = file_size(hiberfil);
    let hib_exists = hiberfil.exists();
    ops.push(ReclaimOpportunity {
        id: "hibernation".into(),
        label: "休眠文件".into(),
        description: "禁用休眠以删除 hiberfil.sys".into(),
        current_size: hib_size,
        reclaimable_size: if hib_exists { hib_size } else { 0 },
        requires_admin: true,
        requires_reboot: false,
        reversible: true,
        risk_level: "safe".into(),
    });

    let vss_size = estimate_vss_size().await;
    ops.push(ReclaimOpportunity {
        id: "restore_points".into(),
        label: "系统还原点".into(),
        description: "删除旧的系统还原点".into(),
        current_size: vss_size,
        reclaimable_size: vss_size,
        requires_admin: true,
        requires_reboot: false,
        reversible: false,
        risk_level: "caution".into(),
    });

    let pagefile_size = file_size(Path::new(r"C:\pagefile.sys"));
    ops.push(ReclaimOpportunity {
        id: "pagefile".into(),
        label: "页面文件迁移".into(),
        description: "将页面文件迁移到其他磁盘".into(),
        current_size: pagefile_size,
        reclaimable_size: pagefile_size,
        requires_admin: true,
        requires_reboot: true,
        reversible: true,
        risk_level: "caution".into(),
    });

    Ok(ops)
}

async fn estimate_vss_size() -> u64 {
    let output = hidden_command("vssadmin")
        .args(["list", "shadowstorage"])
        .output()
        .await;

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return 0,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let lower = line.to_lowercase();
        if lower.contains("used shadow copy storage") {
            if let Some(num) = extract_number_from_line(line) {
                if lower.contains("gb") {
                    return num * 1024 * 1024 * 1024;
                } else if lower.contains("mb") {
                    return num * 1024 * 1024;
                } else if lower.contains("kb") {
                    return num * 1024;
                }
                return num;
            }
        }
    }
    0
}
