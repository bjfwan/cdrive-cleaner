use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use sysinfo::System;
use tauri::{AppHandle, Emitter};
use winreg::enums::*;
use winreg::RegKey;
use crate::winfs;

#[derive(Clone, serde::Serialize)]
pub struct SafetyAnalysisProgress {
    pub scanned_dirs: usize,
    pub elapsed_ms: u64,
    pub dirs_per_second: f64,
}

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

/// 预构建的环境快照，避免重复收集系统信息
struct EnvironmentSnapshot {
    process_paths: Vec<String>,
    registry_entries: Vec<RegistryEntry>,
}

struct RegistryEntry {
    install_location: String,
    has_uninstall: bool,
}

struct CachedEnvironmentSnapshot {
    created_at: Instant,
    snapshot: Arc<EnvironmentSnapshot>,
}

#[derive(Clone)]
struct CachedSafetyResult {
    created_at: Instant,
    modified_time: Option<u64>,
    size: u64,
    result: MigrationSafety,
}

impl EnvironmentSnapshot {
    fn new() -> Self {
        Self {
            process_paths: Self::collect_process_paths(),
            registry_entries: Self::collect_registry_entries(),
        }
    }

    fn collect_process_paths() -> Vec<String> {
        let mut sys = System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        sys.processes()
            .values()
            .filter_map(|p| p.exe().map(|e| e.to_string_lossy().to_uppercase()))
            .collect()
    }

    fn collect_registry_entries() -> Vec<RegistryEntry> {
        let mut entries = Vec::new();
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let uninstall_paths = [
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
            "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        ];
        for reg_path in &uninstall_paths {
            if let Ok(key) = hklm.open_subkey(reg_path) {
                for subkey_name in key.enum_keys().filter_map(Result::ok) {
                    if let Ok(subkey) = key.open_subkey(&subkey_name) {
                        if let Ok(install_loc) = subkey.get_value::<String, _>("InstallLocation") {
                            if !install_loc.is_empty() {
                                entries.push(RegistryEntry {
                                    install_location: install_loc.to_uppercase(),
                                    has_uninstall: subkey.get_value::<String, _>("UninstallString").is_ok(),
                                });
                            }
                        }
                    }
                }
            }
        }
        entries
    }

    fn is_process_running(&self, path_upper: &str) -> bool {
        self.process_paths.iter().any(|exe| exe.starts_with(path_upper))
    }

    fn check_registry_dependency(&self, path_upper: &str) -> u8 {
        if !path_upper.contains("PROGRAM FILES") {
            return 0;
        }
        for entry in &self.registry_entries {
            if path_upper.starts_with(&entry.install_location) {
                return if entry.has_uninstall { 3 } else { 2 };
            }
        }
        1
    }
}

const MAX_CHECK_DEPTH: usize = 3;
const CHECK_TIMEOUT_SECS: u64 = 20;
const ENV_CACHE_TTL_SECS: u64 = 15;
const SAFETY_CACHE_TTL_SECS: u64 = 30;
const MAX_SAFETY_CACHE_ENTRIES: usize = 256;

fn environment_cache() -> &'static Mutex<Option<CachedEnvironmentSnapshot>> {
    static CACHE: OnceLock<Mutex<Option<CachedEnvironmentSnapshot>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn safety_cache() -> &'static Mutex<HashMap<String, CachedSafetyResult>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedSafetyResult>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_environment_snapshot() -> Arc<EnvironmentSnapshot> {
    let cache = environment_cache();
    let mut guard = cache.lock().unwrap();

    if let Some(cached) = guard.as_ref() {
        if cached.created_at.elapsed() <= Duration::from_secs(ENV_CACHE_TTL_SECS) {
            return Arc::clone(&cached.snapshot);
        }
    }

    let snapshot = Arc::new(EnvironmentSnapshot::new());
    *guard = Some(CachedEnvironmentSnapshot {
        created_at: Instant::now(),
        snapshot: Arc::clone(&snapshot),
    });
    snapshot
}

fn get_path_modified_time(path: &Path) -> Option<u64> {
    fs::symlink_metadata(path).ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
}

fn get_cached_safety_result(path: &Path, size: u64, modified_time: Option<u64>) -> Option<MigrationSafety> {
    let key = path.to_string_lossy().to_string();
    let cache = safety_cache();
    let mut guard = cache.lock().unwrap();

    if let Some(cached) = guard.get(&key) {
        let is_fresh = cached.created_at.elapsed() <= Duration::from_secs(SAFETY_CACHE_TTL_SECS);
        if is_fresh && cached.size == size && cached.modified_time == modified_time {
            return Some(cached.result.clone());
        }
    }

    guard.remove(&key);
    None
}

fn cache_safety_result(path: &Path, size: u64, modified_time: Option<u64>, result: &MigrationSafety) {
    let key = path.to_string_lossy().to_string();
    let cache = safety_cache();
    let mut guard = cache.lock().unwrap();

    if guard.len() >= MAX_SAFETY_CACHE_ENTRIES {
        let mut entries: Vec<_> = guard.iter()
            .map(|(cache_key, cached)| (cache_key.clone(), cached.created_at))
            .collect();
        entries.sort_by_key(|(_, created_at)| *created_at);
        let remove_count = guard.len().saturating_sub(MAX_SAFETY_CACHE_ENTRIES - 1);
        for (old_key, _) in entries.into_iter().take(remove_count) {
            guard.remove(&old_key);
        }
    }

    guard.insert(key, CachedSafetyResult {
        created_at: Instant::now(),
        modified_time,
        size,
        result: result.clone(),
    });
}

fn should_scan_subdirs(path_upper: &str, app_type: &str) -> bool {
    if path_upper.contains("PROGRAM FILES") || path_upper.contains("\\APPDATA\\") {
        return true;
    }

    !matches!(app_type, "Steam游戏" | "Epic游戏" | "便携式应用" | "用户数据" | "媒体文件" | "临时文件")
}

pub fn analyze_migration_safety(path: &Path, size: u64, app: AppHandle) -> Result<MigrationSafety, String> {
    let path_str = path.to_string_lossy().to_uppercase();
    let modified_time = get_path_modified_time(path);

    if let Some((true, reason)) = is_system_critical(&path_str) {
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

    let _ = app.emit("safety-analysis-progress", SafetyAnalysisProgress {
        scanned_dirs: 0,
        elapsed_ms: 0,
        dirs_per_second: 0.0,
    });

    if let Some(cached) = get_cached_safety_result(path, size, modified_time) {
        let _ = app.emit("safety-analysis-progress", SafetyAnalysisProgress {
            scanned_dirs: cached.subdirs_checked,
            elapsed_ms: 0,
            dirs_per_second: 0.0,
        });
        return Ok(cached);
    }

    let app_type = identify_app_type(path, &path_str);
    let env = get_environment_snapshot();

    let mut score: f32 = 70.0;
    let mut reasons = Vec::new();
    let mut recommendations = Vec::new();
    let mut dangerous_subdirs = Vec::new();
    let mut risky_subdirs = Vec::new();

    let subdir_risks = if path.is_dir() && should_scan_subdirs(&path_str, &app_type) {
        analyze_subdirs_parallel(path, &env, app.clone())
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

    if env.is_process_running(&path_str) {
        score -= 35.0;
        reasons.push("有进程正在运行".to_string());
        recommendations.push("建议关闭相关程序后再迁移".to_string());
    }

    if is_file_locked(path) {
        score -= 30.0;
        reasons.push("文件被占用".to_string());
        recommendations.push("文件正在被使用，请关闭相关程序".to_string());
    }

    match env.check_registry_dependency(&path_str) {
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

    let risk_level = match safety_score {
        0..=29 => RiskLevel::Dangerous,
        30..=49 => RiskLevel::Risky,
        50..=74 => RiskLevel::Moderate,
        _ => RiskLevel::Safe,
    };
    let can_migrate = risk_level != RiskLevel::Dangerous;

    if reasons.is_empty() {
        reasons.push("未检测到明显风险".to_string());
    }
    if recommendations.is_empty() {
        recommendations.push("建议先备份重要数据".to_string());
        recommendations.push("迁移后可通过历史记录回滚".to_string());
    }

    let result = MigrationSafety {
        risk_level,
        safety_score,
        can_migrate,
        reasons,
        recommendations,
        app_type,
        subdirs_checked,
        dangerous_subdirs,
        risky_subdirs,
    };

    cache_safety_result(path, size, modified_time, &result);

    Ok(result)
}

fn analyze_subdirs_parallel(path: &Path, env: &Arc<EnvironmentSnapshot>, app: AppHandle) -> Vec<SubdirRisk> {
    let start = Instant::now();
    let timeout = Duration::from_secs(CHECK_TIMEOUT_SECS);
    let timed_out = AtomicBool::new(false);
    let scanned_count = Arc::new(AtomicUsize::new(0));

    let subdirs = collect_subdirs(path, MAX_CHECK_DEPTH, &start, &timeout, &timed_out);
    if timed_out.load(Ordering::Relaxed) {
        return vec![];
    }

    let scanned_clone = Arc::clone(&scanned_count);
    let app_clone = app.clone();
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    let progress_handle = std::thread::spawn(move || {
        let thread_start = Instant::now();
        let mut last_count = 0;
        loop {
            std::thread::sleep(Duration::from_millis(200));
            if should_stop_clone.load(Ordering::Relaxed) {
                break;
            }
            let current = scanned_clone.load(Ordering::Relaxed);
            if current != last_count {
                let elapsed = thread_start.elapsed().as_millis() as u64;
                let _ = app_clone.emit("safety-analysis-progress", SafetyAnalysisProgress {
                    scanned_dirs: current,
                    elapsed_ms: elapsed,
                    dirs_per_second: if elapsed > 0 { current as f64 / (elapsed as f64 / 1000.0) } else { 0.0 },
                });
                last_count = current;
            }
        }
    });

    let env_ref = Arc::clone(env);
    let results: Vec<SubdirRisk> = subdirs.par_iter()
        .filter_map(|subdir| {
            if start.elapsed() > timeout {
                timed_out.store(true, Ordering::Relaxed);
                return None;
            }
            let result = quick_check_subdir(subdir, &env_ref);
            scanned_count.fetch_add(1, Ordering::Relaxed);
            result
        })
        .collect();

    should_stop.store(true, Ordering::Relaxed);
    let _ = progress_handle.join();

    let elapsed = start.elapsed().as_millis() as u64;
    let final_count = scanned_count.load(Ordering::Relaxed);
    let _ = app.emit("safety-analysis-progress", SafetyAnalysisProgress {
        scanned_dirs: final_count,
        elapsed_ms: elapsed,
        dirs_per_second: if elapsed > 0 { final_count as f64 / (elapsed as f64 / 1000.0) } else { 0.0 },
    });

    results
}

fn collect_subdirs(path: &Path, max_depth: usize, start: &Instant, timeout: &Duration, timed_out: &AtomicBool) -> Vec<PathBuf> {
    let mut subdirs = Vec::new();
    collect_subdirs_native(path, 0, max_depth, &mut subdirs, start, timeout, timed_out);

    subdirs
}

fn collect_subdirs_native(
    path: &Path,
    depth: usize,
    max_depth: usize,
    subdirs: &mut Vec<PathBuf>,
    start: &Instant,
    timeout: &Duration,
    timed_out: &AtomicBool,
) {
    if depth >= max_depth || start.elapsed() > *timeout || timed_out.load(Ordering::Relaxed) {
        return;
    }

    let entries = match winfs::enumerate_directory(path, false) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries {
        if start.elapsed() > *timeout || timed_out.load(Ordering::Relaxed) {
            timed_out.store(true, Ordering::Relaxed);
            return;
        }

        if entry.is_symlink || !entry.is_dir {
            continue;
        }

        subdirs.push(entry.path.clone());
        collect_subdirs_native(&entry.path, depth + 1, max_depth, subdirs, start, timeout, timed_out);
    }
}

fn quick_check_subdir(path: &Path, env: &EnvironmentSnapshot) -> Option<SubdirRisk> {
    let path_str = path.to_string_lossy().to_uppercase();

    if let Some((true, reason)) = is_system_critical(&path_str) {
        return Some(SubdirRisk {
            path: path.to_string_lossy().to_string(),
            level: RiskLevel::Dangerous,
            reason,
        });
    }

    if env.is_process_running(&path_str) {
        return Some(SubdirRisk {
            path: path.to_string_lossy().to_string(),
            level: RiskLevel::Risky,
            reason: "应用正在运行".to_string(),
        });
    }

    None
}

fn is_system_critical(path_upper: &str) -> Option<(bool, String)> {
    const CRITICAL_PATTERNS: &[(&str, &str)] = &[
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

    for &(pattern, reason) in CRITICAL_PATTERNS {
        if path_upper.contains(pattern) {
            return Some((true, reason.to_string()));
        }
    }
    Some((false, String::new()))
}

fn identify_app_type(path: &Path, path_upper: &str) -> String {
    if path.join("steam_appid.txt").exists() || path_upper.contains("STEAMAPPS") || path_upper.contains("STEAM\\COMMON") {
        return "Steam游戏".to_string();
    }
    if path_upper.contains("EPIC GAMES") || path.join(".egstore").exists() {
        return "Epic游戏".to_string();
    }
    if is_portable_app(path) {
        return "便携式应用".to_string();
    }
    if path_upper.contains("\\USERS\\") && (path_upper.contains("\\DOWNLOADS") || path_upper.contains("\\DOCUMENTS") || path_upper.contains("\\DESKTOP")) {
        return "用户数据".to_string();
    }
    if path_upper.contains("\\USERS\\") && (path_upper.contains("\\VIDEOS") || path_upper.contains("\\PICTURES") || path_upper.contains("\\MUSIC")) {
        return "媒体文件".to_string();
    }
    if path_upper.contains("\\TEMP") || path_upper.contains("\\TMP") {
        return "临时文件".to_string();
    }
    if path_upper.contains("\\GAMES\\") || path_upper.contains("\\GAME\\") {
        return "游戏".to_string();
    }
    "未知类型".to_string()
}

fn is_portable_app(path: &Path) -> bool {
    path.join("portable.txt").exists()
        || path.join("portable.ini").exists()
        || path.join("App\\AppInfo\\appinfo.ini").exists()
        || (path.join("Data").exists() && path.join("App").exists())
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
        fs::OpenOptions::new().write(true).open(path).is_err()
    }
}
