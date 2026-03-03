use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;
use sysinfo::System;
use winreg::RegKey;
use winreg::enums::*;

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
}

/// 主入口：分析迁移安全性
pub fn analyze_migration_safety(path: &Path, size: u64) -> Result<MigrationSafety, String> {
    let path_str = path.to_string_lossy().to_uppercase();
    
    let mut score: f32 = 70.0;  // 基础分
    let mut reasons = Vec::new();
    let mut recommendations = Vec::new();
    
    // === 第一层：系统关键路径检测 ===
    if is_system_critical(&path_str) {
        return Ok(MigrationSafety {
            risk_level: RiskLevel::Dangerous,
            safety_score: 0,
            can_migrate: false,
            reasons: vec!["系统关键目录，禁止迁移".to_string()],
            recommendations: vec!["此目录包含系统核心文件，迁移会导致系统崩溃".to_string()],
            app_type: "系统关键".to_string(),
        });
    }
    
    // === 第二层：应用类型识别 ===
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
    
    // === 第三层：运行时检测 ===
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
    
    // === 第四层：注册表依赖检测 ===
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
    
    // === 第五层：路径特征 ===
    if path_str.contains("PROGRAM FILES") {
        score -= 10.0;
        reasons.push("位于 Program Files".to_string());
    }
    
    // === 第六层：大小加成 ===
    if size > 10 * 1024 * 1024 * 1024 {
        score += 8.0;
        reasons.push("大型文件夹（通常是游戏或媒体）".to_string());
    }
    
    // 限制范围
    score = score.clamp(0.0, 100.0);
    let safety_score = score as u8;
    
    // 确定风险等级
    let risk_level = if safety_score < 30 {
        RiskLevel::Dangerous
    } else if safety_score < 50 {
        RiskLevel::Risky
    } else if safety_score < 75 {
        RiskLevel::Moderate
    } else {
        RiskLevel::Safe
    };
    
    // 确定是否可以迁移
    let can_migrate = risk_level != RiskLevel::Dangerous;
    
    // 如果没有原因，添加默认原因
    if reasons.is_empty() {
        reasons.push("未检测到明显风险".to_string());
    }
    
    // 如果没有建议，添加默认建议
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
    })
}

/// 检测是否是系统关键路径
fn is_system_critical(path_upper: &str) -> bool {
    let critical_paths = [
        "C:\\WINDOWS\\SYSTEM32",
        "C:\\WINDOWS\\SYSWOW64",
        "C:\\WINDOWS\\WINSXS",
        "C:\\PROGRAM FILES\\WINDOWSAPPS",
        "C:\\PROGRAMDATA\\MICROSOFT\\WINDOWS",
        "C:\\SYSTEM VOLUME INFORMATION",
        "C:\\$RECYCLE.BIN",
    ];
    
    critical_paths.iter().any(|&critical| path_upper.starts_with(critical))
}

/// 识别应用类型
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

/// 检测便携式应用特征
fn is_portable_app(path: &Path) -> bool {
    // 检查便携式标记文件
    if path.join("portable.txt").exists() || 
       path.join("portable.ini").exists() ||
       path.join("App\\AppInfo\\appinfo.ini").exists() {
        return true;
    }
    
    // 检查是否有 Data 目录（便携式应用常见结构）
    if path.join("Data").exists() && path.join("App").exists() {
        return true;
    }
    
    false
}

/// 检测进程是否正在运行
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

/// 检测文件是否被占用
fn is_file_locked(path: &Path) -> bool {
    // 如果是目录，检查是否可以创建临时文件
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
        // 如果是文件，尝试以独占模式打开
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .is_err()
    }
}

/// 检测注册表依赖（返回 0-3 的等级）
fn check_registry_dependency(path: &Path) -> u8 {
    let path_str = path.to_string_lossy().to_uppercase();
    
    // 如果不在 Program Files，通常没有注册表依赖
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
                            // 检查是否有卸载程序（表示深度集成）
                            if subkey.get_value::<String, _>("UninstallString").is_ok() {
                                return 3; // 高依赖
                            }
                            return 2; // 中等依赖
                        }
                    }
                }
            }
        }
    }
    
    // 在 Program Files 但没找到注册表项，可能是便携式或绿色软件
    1
}
