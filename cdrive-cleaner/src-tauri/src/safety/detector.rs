use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use sysinfo::System;
use winreg::RegKey;
use winreg::enums::*;
use rayon::prelude::*;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Safe,
    Moderate,
    Risky,
    Dangerous,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSafety {
    pub risk_level: RiskLevel,
    pub safety_score: u8,
    pub can_migrate: bool,
    pub reasons: Vec<String>,
    pub recommendations: Vec<String>,
    pub app_type: String,
    pub subdirs_checked: usize,
    pub dangerous_subdirs: Vec<String>,
    pub risky_subdirs: Vec<String>,
}

#[derive(Debug, Clone)]
struct SubdirRisk {
    path: String,
    level: RiskLevel,
    reason: String,
}

const MAX_CHECK_DEPTH: usize = 4;
const CHECK_TIMEOUT_SECS: u64 = 5;

pub fn analyze_migration_safety(path: &Path, size: u64) -> Result<MigrationSafety, String> {
    let path_str = path.to_string_lossy().to_uppercase();
    
    let mut score: f32 = 70.0;
    let mut reasons = Vec::new();
    let mut recommendations = Vec::new();
    let mut dangerous_subdirs = Vec::new();
    let mut risky_subdirs = Vec::new();
    
    if let Some((is_critical, reason)) = is_system_critical_enhanced(&path_str) {
        if is_critical {
            return Ok(MigrationSafety {
                risk_level: RiskLevel::Dangerous,
                safety_score: 0,
                can_migrate: false,
                reasons: vec![reason],
                recommendations: vec!["此目录包含系统核心文件，迁移会导致系统崩溃".to_string()],
                app_type: "系统关键".to_string(),
                subdirs_checked: 0,
                dangerous_subdirs: vec![],
                risky_subdirs: vec![],
            });
        }
    }
    
    let subdir_risks = if path.is_dir() {
        analyze_subdirs_parallel(path, MAX_CHECK_DEPTH, CHECK_TIMEOUT_SECS)
    } else {
        vec![]
    };
    
    let subdirs_checked = subdir_risks.len();
    
    for risk in &subdir_risks {
        match risk.level {
            RiskLevel::Dangerous => {
                score -= 40.0;
                dangerous_subdirs.push(format!("{}: {}", risk.path, risk.reason));
                reasons.push(format!("危险子目录: {}", risk.reason));
            }
            RiskLevel::Risky => {
                score -= 20.0;
                risky_subdirs.push(format!("{}: {}", risk.path, risk.reason));
                reasons.push(format!("风险子目录: {}", risk.reason));
            }
            _ => {}
        }
    }
    
    if !dangerous_subdirs.is_empty() {
        recommendations.push(format!("发现 {} 个危险子目录，强烈建议不要迁移", dangerous_subdirs.len()));
    }
    if !risky_subdirs.is_empty() {
        recommendations.push(format!("发现 {} 个风险子目录，建议谨慎操作", risky_subdirs.len()));
    }
    
    let app_type = identify_app_type(path, &path_str);
    
    match app_type.as_str() {
        "Steam游戏" => {
            score += 20.0;
            reasons.push("Steam 游戏目录".to_string());
            recommendations.push("Steam 游戏可以安全迁移，迁移后需在 Steam 中重新指定游戏库位置".to_string());
        }
        "Epic游戏" => {
            score += 18.0;
            reasons.push("Epic 游戏目录".to_string());
            recommendations.push("Epic 游戏可以安全迁移".to_string());
        }
        "便携式应用" => {
            score += 25.0;
            reasons.push("便携式应用特征".to_string());
            recommendations.push("便携式应用可以安全迁移，无需额外配置".to_string());
        }
        "用户数据" => {
            score += 30.0;
            reasons.push("用户数据目录".to_string());
            recommendations.push("用户数据可以安全迁移".to_string());
        }
        "媒体文件" => {
            score += 25.0;
            reasons.push("媒体文件目录".to_string());
            recommendations.push("媒体文件可以安全迁移".to_string());
        }
        "临时文件" => {
            score += 28.0;
            reasons.push("临时文件目录".to_string());
            recommendations.push("临时文件可以安全迁移或直接删除".to_string());
        }
        _ => {}
    }
    
    if is_process_running(path) {
        score -= 35.0;
        reasons.push("有进程正在运行".to_string());
        recommendations.push("建议关闭相关程序后再迁移".to_string());
    }
    
    if is_file_locked(path) {
        score -= 30.0;
        reasons.push("文件被占用".to_string());
        recommendations.push("文件正在被使用，请关闭相关程序".to_string());
    }
    
    let registry_level = check_registry_dependency(path);
    match registry_level {
        0 => {
            score += 15.0;
            reasons.push("无注册表依赖".to_string());
        }
        1 => {
            score -= 5.0;
            reasons.push("低注册表依赖".to_string());
        }
        2 => {
            score -= 15.0;
            reasons.push("中等注册表依赖".to_string());
            recommendations.push("迁移后可能需要重新配置软件".to_string());
        }
        _ => {
            score -= 25.0;
            reasons.push("高注册表依赖".to_string());
            recommendations.push("不建议迁移，可能导致软件无法启动".to_string());
        }
    }
    
    if path_str.contains("PROGRAM FILES") {
        score -= 10.0;
        reasons.push("位于 Program Files".to_string());
    }
    
    if size > 10 * 1024 * 1024 * 1024 {
        score += 8.0;
        reasons.push("大型文件夹（通常是游戏或媒体）".to_string());
    }
    
    score = score.clamp(0.0, 100.0);
    let safety_score = score as u8;
    
    let risk_level = if safety_score < 30 {
        RiskLevel::Dangerous
    } else if safety_score < 50 {
        RiskLevel::Risky
    } else if safety_score < 75 {
        RiskLevel::Moderate
    } else {
        RiskLevel::Safe
    };
    
    let can_migrate = risk_level != RiskLevel::Dangerous;
    
    if reasons.is_empty() {
        reasons.push("未检测到明显风险".to_string());
    }
    
    if recommendations.is_empty() {
        recommendations.push("建议先备份重要数据".to_string());
        recommendations.push("迁移后可通过历史记录回滚".to_string());
    }
    
    Ok(MigrationSafety {
        risk_level,
        safety_score,
        can_migrate,
        reasons,
        recommendations,
        app_type,
        subdirs_checked,
        dangerous_subdirs,
        risky_subdirs,
    })
}

fn analyze_subdirs_parallel(path: &Path, max_depth: usize, timeout_secs: u64) -> Vec<SubdirRisk> {
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let timed_out = AtomicBool::new(false);
    
    let subdirs = collect_subdirs(path, max_depth, &start, &timeout, &timed_out);
    
    if timed_out.load(Ordering::Relaxed) {
        return vec![];
    }
    
    subdirs.par_iter()
        .filter_map(|subdir| {
            if start.elapsed() > timeout {
                timed_out.store(true, Ordering::Relaxed);
                return None;
            }
            quick_check_subdir(subdir)
        })
        .collect()
}

fn collect_subdirs(
    path: &Path, 
    max_depth: usize, 
    start: &Instant, 
    timeout: &Duration,
    timed_out: &AtomicBool
) -> Vec<PathBuf> {
    let mut subdirs = Vec::new();
    collect_subdirs_recursive(path, 0, max_depth, &mut subdirs, start, timeout, timed_out);
    subdirs
}

fn collect_subdirs_recursive(
    path: &Path,
    current_depth: usize,
    max_depth: usize,
    subdirs: &mut Vec<PathBuf>,
    start: &Instant,
    timeout: &Duration,
    timed_out: &AtomicBool,
) {
    if current_depth >= max_depth || start.elapsed() > *timeout || timed_out.load(Ordering::Relaxed) {
        return;
    }
    
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if start.elapsed() > *timeout {
                timed_out.store(true, Ordering::Relaxed);
                return;
            }
            
            let subpath = entry.path();
            if subpath.is_dir() {
                subdirs.push(subpath.clone());
                collect_subdirs_recursive(&subpath, current_depth + 1, max_depth, subdirs, start, timeout, timed_out);
            }
        }
    }
}

fn quick_check_subdir(path: &Path) -> Option<SubdirRisk> {
    let path_str = path.to_string_lossy().to_uppercase();
    
    if let Some((is_critical, reason)) = is_system_critical_enhanced(&path_str) {
        if is_critical {
            return Some(SubdirRisk {
                path: path.to_string_lossy().to_string(),
                level: RiskLevel::Dangerous,
                reason,
            });
        }
    }
    
    if is_process_running(path) {
        return Some(SubdirRisk {
            path: path.to_string_lossy().to_string(),
            level: RiskLevel::Risky,
            reason: "应用正在运行".to_string(),
        });
    }
    
    None
}

fn is_system_critical_enhanced(path_upper: &str) -> Option<(bool, String)> {
    let critical_patterns = [
        ("C:\\WINDOWS\\SYSTEM32", "Windows 系统核心目录"),
        ("C:\\WINDOWS\\SYSWOW64", "Windows 系统目录"),
        ("C:\\WINDOWS\\WINSXS", "Windows 组件存储"),
        ("C:\\PROGRAM FILES\\WINDOWSAPPS", "Windows 应用商店"),
        ("C:\\PROGRAMDATA\\MICROSOFT\\WINDOWS", "Windows 系统数据"),
        ("C:\\SYSTEM VOLUME INFORMATION", "系统卷信息"),
        ("C:\\$RECYCLE.BIN", "回收站"),
        ("\\APPDATA\\LOCAL\\MICROSOFT\\WINDOWS\\", "Windows 系统数据"),
        ("\\APPDATA\\ROAMING\\MICROSOFT\\WINDOWS\\", "Windows 用户配置"),
        ("\\APPDATA\\LOCAL\\PACKAGES\\", "UWP 应用数据"),
        ("\\APPDATA\\ROAMING\\MICROSOFT\\PROTECT\\", "Windows 凭据保护"),
        ("\\APPDATA\\LOCAL\\MICROSOFT\\CREDENTIALS\\", "系统凭据"),
        ("\\APPDATA\\LOCAL\\GOOGLE\\CHROME\\USER DATA\\DEFAULT\\", "Chrome 活动配置"),
        ("\\APPDATA\\LOCAL\\MICROSOFT\\EDGE\\USER DATA\\DEFAULT\\", "Edge 活动配置"),
    ];
    
    for (pattern, reason) in &critical_patterns {
        if path_upper.contains(pattern) {
            return Some((true, reason.to_string()));
        }
    }
    
    Some((false, String::new()))
}



fn identify_app_type(path: &Path, path_upper: &str) -> String {
    // Steam 游戏
    if path.join("steam_appid.txt").exists() || path_upper.contains("STEAMAPPS") || path_upper.contains("STEAM\\COMMON") {
        return "Steam游戏".to_string();
    }
    
    // Epic 游戏
    if path_upper.contains("EPIC GAMES") || path.join(".egstore").exists() {
        return "Epic游戏".to_string();
    }
    
    // 便携式应用
    if is_portable_app(path) {
        return "便携式应用".to_string();
    }
    
    // 用户数据
    if path_upper.contains("\\USERS\\") && (
        path_upper.contains("\\DOWNLOADS") ||
        path_upper.contains("\\DOCUMENTS") ||
        path_upper.contains("\\DESKTOP")
    ) {
        return "用户数据".to_string();
    }
    
    // 媒体文件
    if path_upper.contains("\\USERS\\") && (
        path_upper.contains("\\VIDEOS") ||
        path_upper.contains("\\PICTURES") ||
        path_upper.contains("\\MUSIC")
    ) {
        return "媒体文件".to_string();
    }
    
    // 临时文件
    if path_upper.contains("\\TEMP") || path_upper.contains("\\TMP") {
        return "临时文件".to_string();
    }
    
    // 游戏目录（通用）
    if path_upper.contains("\\GAMES\\") || path_upper.contains("\\GAME\\") {
        return "游戏".to_string();
    }
    
    "未知类型".to_string()
}

fn is_portable_app(path: &Path) -> bool {
    
    if path.join("portable.txt").exists() || 
       path.join("portable.ini").exists() ||
       path.join("App\\AppInfo\\appinfo.ini").exists() {
        return true;
    }
    
    if path.join("Data").exists() && path.join("App").exists() {
        return true;
    }
    
    false
}

fn is_process_running(path: &Path) -> bool {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    for (_pid, process) in sys.processes() {
        if let Some(exe) = process.exe() {
            if let Some(exe_path) = exe.as_os_str().to_str() {
                if exe_path.starts_with(&path.to_string_lossy().to_string()) {
                    return true;
                }
            }
        }
    }
    
    false
}

fn is_file_locked(path: &Path) -> bool {
    if path.is_dir() {
        let test_file = path.join(".migration_test");
        match fs::write(&test_file, b"test") {
            Ok(_) => {
                let _ = fs::remove_file(&test_file);
                false
            }
            Err(_) => true,
        }
    } else {
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .is_err()
    }
}

fn check_registry_dependency(path: &Path) -> u8 {
    let path_str = path.to_string_lossy().to_uppercase();
    
    if !path_str.contains("PROGRAM FILES") {
        return 0;
    }
    
    let hklm = match RegKey::predef(HKEY_LOCAL_MACHINE) {
        key => key,
    };
    
    let uninstall_paths = [
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    ];
    
    for reg_path in &uninstall_paths {
        if let Ok(key) = hklm.open_subkey(reg_path) {
            for subkey_name in key.enum_keys().filter_map(Result::ok) {
                if let Ok(subkey) = key.open_subkey(&subkey_name) {
                    if let Ok(install_loc) = subkey.get_value::<String, _>("InstallLocation") {
                        let install_loc_upper = install_loc.to_uppercase();
                        if path_str.starts_with(&install_loc_upper) {
                            if subkey.get_value::<String, _>("UninstallString").is_ok() {
                                return 3;
                            }
                            return 2;
                        }
                    }
                }
            }
        }
    }
    
    1
}
