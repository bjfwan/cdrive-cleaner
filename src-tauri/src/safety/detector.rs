use crate::migration::LinkType;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[cfg(windows)]
use std::sync::OnceLock;

use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Safe,
    SafeAfterAction,
    Blocked,
    SystemCritical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Blocker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub gate: String,
    pub severity: Severity,
    pub message: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSafety {
    pub verdict: Verdict,
    pub can_migrate: bool,
    pub findings: Vec<Finding>,
    pub required_actions: Vec<String>,
    pub app_type: String,
    pub analysis_duration_ms: u64,
    /// 每个 gate 单独耗时（毫秒），用于性能 profile。
    /// 注意：因为 gate 是并行跑的，这里的总和会大于 `analysis_duration_ms`。
    #[serde(default)]
    pub gate_durations_ms: std::collections::HashMap<String, u64>,
}

// =========================================================================
// 结果缓存：同一路径短时间内重复分析直接返回上次结果。
// 用户在"迁移对话框"弹/关/弹的场景里能立刻命中，避免每次重做 2-3s 的 IO。
// =========================================================================

const SAFETY_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct CacheEntry {
    result: MigrationSafety,
    inserted_at: Instant,
}

fn cache() -> &'static Mutex<std::collections::HashMap<String, CacheEntry>> {
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn cache_key(path: &Path, link_type: &LinkType, target_disk: Option<&str>, source_size: u64) -> String {
    format!(
        "{}|{:?}|{}|{}",
        path.to_string_lossy().to_uppercase(),
        link_type,
        target_disk.unwrap_or("-"),
        source_size
    )
}

fn cache_get(key: &str) -> Option<MigrationSafety> {
    let mut guard = cache().lock().ok()?;
    if let Some(entry) = guard.get(key) {
        if entry.inserted_at.elapsed() < SAFETY_CACHE_TTL {
            return Some(entry.result.clone());
        }
        // 过期了，顺手清掉。
        guard.remove(key);
    }
    None
}

fn cache_put(key: String, result: &MigrationSafety) {
    if let Ok(mut guard) = cache().lock() {
        // 简单 cap：超过 64 条就清空，避免长期累积内存。生产场景下这个量级足够。
        if guard.len() >= 64 {
            guard.clear();
        }
        guard.insert(
            key,
            CacheEntry {
                result: result.clone(),
                inserted_at: Instant::now(),
            },
        );
    }
}

/// 清除安全检测结果缓存。迁移完成或回滚后应调用，因为目录的状态可能已变。
pub fn invalidate_safety_cache() {
    if let Ok(mut guard) = cache().lock() {
        guard.clear();
    }
}

pub fn analyze(
    path: &Path,
    link_type: LinkType,
    target_disk: Option<&str>,
    source_size: u64,
) -> MigrationSafety {
    let key = cache_key(path, &link_type, target_disk, source_size);
    if let Some(cached) = cache_get(&key) {
        return cached;
    }

    let start = Instant::now();
    let path_upper = path.to_string_lossy().to_uppercase();
    let app_type = identify_app_type(path, &path_upper);
    let mut findings = Vec::new();
    let mut gate_durations: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();

    let sc_start = Instant::now();
    gate_system_critical(&path_upper, &mut findings);
    gate_durations.insert(
        "system_critical".to_string(),
        sc_start.elapsed().as_millis() as u64,
    );

    if !has_blocker(&findings) {
        let handles = GateHandles::run_parallel(path, &link_type, target_disk, source_size);
        let (gate_findings, durations) = handles.collect();
        findings.extend(gate_findings);
        gate_durations.extend(durations);
    }
    dedup_findings(&mut findings);

    let verdict = derive_verdict(&findings);
    let can_migrate = matches!(verdict, Verdict::Safe | Verdict::SafeAfterAction);
    let required_actions = extract_actions(&findings);

    let result = MigrationSafety {
        verdict,
        can_migrate,
        findings,
        required_actions,
        app_type,
        analysis_duration_ms: start.elapsed().as_millis() as u64,
        gate_durations_ms: gate_durations,
    };

    cache_put(key, &result);
    result
}

fn has_blocker(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.severity == Severity::Blocker)
}

fn derive_verdict(findings: &[Finding]) -> Verdict {
    if findings.iter().any(|f| f.gate == "system_critical") {
        return Verdict::SystemCritical;
    }
    if findings.iter().any(|f| f.severity == Severity::Blocker) {
        return Verdict::Blocked;
    }
    if findings.iter().any(|f| f.severity == Severity::Warning) {
        return Verdict::SafeAfterAction;
    }
    Verdict::Safe
}

fn extract_actions(findings: &[Finding]) -> Vec<String> {
    let mut actions: Vec<String> = findings
        .iter()
        .filter(|f| f.severity == Severity::Warning && f.gate == "file_locks")
        .filter_map(|f| f.detail.clone())
        .collect();
    actions.sort();
    actions.dedup();
    actions
}

fn dedup_findings(findings: &mut Vec<Finding>) {
    findings.sort_by(|a, b| {
        a.gate
            .cmp(&b.gate)
            .then_with(|| severity_rank(&a.severity).cmp(&severity_rank(&b.severity)))
            .then_with(|| a.message.cmp(&b.message))
            .then_with(|| a.detail.cmp(&b.detail))
    });
    findings.dedup_by(|a, b| {
        a.gate == b.gate
            && a.severity == b.severity
            && a.message == b.message
            && a.detail == b.detail
    });
}

fn severity_rank(severity: &Severity) -> u8 {
    match severity {
        Severity::Blocker => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

struct GateHandles {
    file_locks: std::thread::JoinHandle<(Vec<Finding>, u64)>,
    boot_drivers: std::thread::JoinHandle<(Vec<Finding>, u64)>,
    hardlinks: std::thread::JoinHandle<(Vec<Finding>, u64)>,
    reparse_points: std::thread::JoinHandle<(Vec<Finding>, u64)>,
    target_volume: std::thread::JoinHandle<(Vec<Finding>, u64)>,
    registry_bindings: std::thread::JoinHandle<(Vec<Finding>, u64)>,
}

fn timed_gate<F>(name: &str, f: F) -> (Vec<Finding>, u64)
where
    F: FnOnce() -> Vec<Finding>,
{
    let _ = name;
    let start = Instant::now();
    let r = f();
    let ms = start.elapsed().as_millis() as u64;
    (r, ms)
}

impl GateHandles {
    fn run_parallel(
        path: &Path,
        link_type: &LinkType,
        target_disk: Option<&str>,
        source_size: u64,
    ) -> Self {
        let p1 = path.to_path_buf();
        let p2 = path.to_path_buf();
        let p3 = path.to_path_buf();
        let p4 = path.to_path_buf();
        let p5 = path.to_path_buf();
        let lt = link_type.clone();
        let td = target_disk.map(String::from);

        Self {
            file_locks: std::thread::spawn(move || timed_gate("file_locks", || gate_file_locks(&p1))),
            boot_drivers: std::thread::spawn(move || timed_gate("boot_drivers", || gate_boot_drivers(&p2))),
            hardlinks: std::thread::spawn(move || timed_gate("hardlinks", || gate_hardlinks(&p3))),
            reparse_points: std::thread::spawn(move || timed_gate("reparse_points", || gate_reparse_points(&p4))),
            target_volume: std::thread::spawn(move || {
                timed_gate("target_volume", || gate_target_volume(td.as_deref(), source_size))
            }),
            registry_bindings: std::thread::spawn(move || {
                timed_gate("registry_bindings", || gate_registry_bindings(&p5, &lt))
            }),
        }
    }

    #[allow(clippy::type_complexity)]
    fn collect(self) -> (Vec<Finding>, std::collections::HashMap<String, u64>) {
        use std::sync::mpsc;

        const GATE_TIMEOUT: Duration = Duration::from_secs(3);

        let mut all_findings = Vec::new();
        let mut durations = std::collections::HashMap::new();
        let pairs: [(&str, std::thread::JoinHandle<(Vec<Finding>, u64)>); 6] = [
            ("file_locks", self.file_locks),
            ("boot_drivers", self.boot_drivers),
            ("hardlinks", self.hardlinks),
            ("reparse_points", self.reparse_points),
            ("target_volume", self.target_volume),
            ("registry_bindings", self.registry_bindings),
        ];

        // 使用 channel + spawn 来实现 per-gate 超时。
        // 如果某个 gate 在 3s 内未完成则跳过其 findings，避免卡住整体分析。
        let (tx, rx) = mpsc::channel::<(&str, Vec<Finding>, u64)>();

        let mut pending = pairs.len();
        for (name, handle) in pairs {
            let tx = tx.clone();
            std::thread::spawn(move || {
                if let Ok((findings, ms)) = handle.join() {
                    let _ = tx.send((name, findings, ms));
                } else {
                    let _ = tx.send((name, Vec::new(), 0));
                }
            });
        }
        drop(tx);

        let deadline = Instant::now() + GATE_TIMEOUT;
        while pending > 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                // 超时：剩余 gate 视为跳过
                tracing::warn!(
                    "safety detector: {} gate(s) timed out after {}s, skipping",
                    pending,
                    GATE_TIMEOUT.as_secs()
                );
                break;
            }
            match rx.recv_timeout(remaining) {
                Ok((name, findings, ms)) => {
                    all_findings.extend(findings);
                    durations.insert(name.to_string(), ms);
                    pending -= 1;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    tracing::warn!(
                        "safety detector: {} gate(s) timed out after {}s, skipping",
                        pending,
                        GATE_TIMEOUT.as_secs()
                    );
                    break;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        (all_findings, durations)
    }
}

#[cfg(windows)]
#[derive(Debug, Clone)]
struct RegistryServiceBinding {
    name: String,
    field: String,
    value: String,
    normalized_upper: String,
    start: u32,
    service_type: u32,
}

#[cfg(windows)]
#[derive(Debug, Clone)]
struct RegistryPathBinding {
    display: String,
    field: String,
    normalized_upper: String,
}

#[cfg(windows)]
#[derive(Debug, Clone)]
struct RegistryBlobBinding {
    display: String,
    value_upper: String,
}

#[cfg(windows)]
#[derive(Debug, Default)]
struct RegistryIndex {
    services: Vec<RegistryServiceBinding>,
    com: Vec<RegistryPathBinding>,
    app_paths: Vec<RegistryPathBinding>,
    tasks: Vec<RegistryBlobBinding>,
    uninstall: Vec<RegistryPathBinding>,
}

const CRITICAL_PATHS: &[(&str, &str)] = &[
    ("C:\\WINDOWS\\SYSTEM32", "Windows 系统核心目录"),
    ("C:\\WINDOWS\\SYSWOW64", "Windows 32 位兼容目录"),
    ("C:\\WINDOWS\\WINSXS", "Windows 组件存储"),
    ("C:\\WINDOWS\\BOOT", "Windows 引导目录"),
    ("C:\\WINDOWS\\FONTS", "Windows 字体目录"),
    ("C:\\WINDOWS\\INF", "Windows 驱动信息目录"),
    ("C:\\WINDOWS\\ASSEMBLY", "全局程序集缓存"),
    ("C:\\WINDOWS\\SERVICING", "Windows 更新服务"),
    ("C:\\WINDOWS\\LOGS", "Windows 系统日志"),
    ("C:\\WINDOWS\\SECURITY", "Windows 安全策略"),
    ("C:\\WINDOWS\\CURSORS", "系统光标资源"),
    ("C:\\WINDOWS\\GLOBALIZATION", "全球化数据"),
    ("C:\\WINDOWS\\IMMERSIVECONTROLPANEL", "系统设置面板"),
    ("C:\\WINDOWS\\SYSTEMAPPS", "系统内置应用"),
    ("C:\\PROGRAM FILES\\WINDOWSAPPS", "Windows 应用商店"),
    (
        "C:\\PROGRAMDATA\\MICROSOFT\\WINDOWS\\START MENU",
        "开始菜单",
    ),
    ("C:\\PROGRAMDATA\\MICROSOFT\\CRYPTO", "系统加密存储"),
    (
        "C:\\PROGRAMDATA\\MICROSOFT\\WINDOWS DEFENDER",
        "Windows Defender",
    ),
    ("C:\\SYSTEM VOLUME INFORMATION", "系统卷信息"),
    ("C:\\$RECYCLE.BIN", "回收站"),
    ("C:\\RECOVERY", "系统恢复分区"),
    ("C:\\BOOT", "引导加载器"),
    (
        "\\APPDATA\\LOCAL\\MICROSOFT\\WINDOWS\\",
        "Windows 用户系统数据",
    ),
    (
        "\\APPDATA\\ROAMING\\MICROSOFT\\WINDOWS\\",
        "Windows 用户配置",
    ),
    ("\\APPDATA\\LOCAL\\PACKAGES\\", "UWP 应用数据"),
    (
        "\\APPDATA\\ROAMING\\MICROSOFT\\PROTECT\\",
        "Windows 凭据保护",
    ),
    ("\\APPDATA\\LOCAL\\MICROSOFT\\CREDENTIALS\\", "系统凭据"),
    (
        "\\APPDATA\\LOCAL\\CONNECTEDDEVICESPLATFORM\\",
        "设备平台数据",
    ),
];

fn gate_system_critical(path_upper: &str, findings: &mut Vec<Finding>) {
    // "集合根"判断：用户选择的是 Program Files 这种**整盘根目录**，而不是单个应用。
    // 整体迁移这类目录基本没有合理使用场景（迁移整个 Program Files 会破坏 Windows
    // 全部应用），且分析 1000+ 个应用的 file_locks 极度耗时。直接给个 Blocker。
    let normalized = path_upper.trim_end_matches('\\');
    const COLLECTION_ROOTS: &[(&str, &str)] = &[
        ("C:\\PROGRAM FILES", "Program Files 是所有应用安装目录的集合根"),
        (
            "C:\\PROGRAM FILES (X86)",
            "Program Files (x86) 是所有 32 位应用安装目录的集合根",
        ),
        ("C:\\PROGRAMDATA", "ProgramData 是所有应用共享数据的集合根"),
        ("C:\\USERS", "Users 是所有用户的集合根"),
    ];
    for (root, reason) in COLLECTION_ROOTS {
        if normalized == *root {
            findings.push(Finding {
                gate: "system_critical".into(),
                severity: Severity::Blocker,
                message: format!("{reason}，请选择具体的子目录而不是整个集合"),
                detail: None,
            });
            return;
        }
    }

    if path_upper.starts_with("C:\\WINDOWS")
        && !path_upper.starts_with("C:\\WINDOWS\\INSTALLER")
        && !path_upper.starts_with("C:\\WINDOWS\\TEMP")
    {
        let matched = CRITICAL_PATHS
            .iter()
            .find(|(p, _)| path_upper.starts_with(p));
        let reason = matched.map(|(_, r)| *r).unwrap_or("Windows 系统目录");
        findings.push(Finding {
            gate: "system_critical".into(),
            severity: Severity::Blocker,
            message: format!("{reason}，禁止迁移"),
            detail: None,
        });
        return;
    }

    for &(pattern, reason) in CRITICAL_PATHS {
        if path_upper.contains(pattern) {
            findings.push(Finding {
                gate: "system_critical".into(),
                severity: Severity::Blocker,
                message: format!("{reason}，禁止迁移"),
                detail: None,
            });
            return;
        }
    }
}

#[cfg(windows)]
fn gate_file_locks(path: &Path) -> Vec<Finding> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::System::RestartManager::{
        RmEndSession, RmGetList, RmRegisterResources, RmStartSession,
    };

    let mut findings = Vec::new();
    let mut session: u32 = 0;
    let mut session_key = [0u16; 64];

    let start_result = unsafe { RmStartSession(&mut session, 0, PWSTR(session_key.as_mut_ptr())) };
    if start_result.is_err() {
        return findings;
    }

    let files = collect_files_for_lock_check(path);
    if files.is_empty() {
        let _ = unsafe { RmEndSession(session) };
        return findings;
    }

    let wide_paths: Vec<Vec<u16>> = files
        .iter()
        .map(|f| {
            f.to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect()
        })
        .collect();
    let ptrs: Vec<PCWSTR> = wide_paths.iter().map(|w| PCWSTR(w.as_ptr())).collect();

    let reg_result = unsafe { RmRegisterResources(session, Some(&ptrs), None, None) };
    if reg_result.is_err() {
        let _ = unsafe { RmEndSession(session) };
        return findings;
    }

    let mut needed: u32 = 0;
    let mut count: u32 = 0;
    let mut reboot_reasons: u32 = 0;

    let _ = unsafe { RmGetList(session, &mut needed, &mut count, None, &mut reboot_reasons) };

    if needed > 0 {
        let mut buf = vec![
            windows::Win32::System::RestartManager::RM_PROCESS_INFO::default();
            needed as usize
        ];
        count = needed;

        let get_result = unsafe {
            RmGetList(
                session,
                &mut needed,
                &mut count,
                Some(buf.as_mut_ptr()),
                &mut reboot_reasons,
            )
        };

        if get_result.is_ok() {
            for info in &buf[..count as usize] {
                let app_name = String::from_utf16_lossy(&info.strAppName)
                    .trim_end_matches('\0')
                    .to_string();
                let pid = info.Process.dwProcessId;

                if app_name.is_empty() {
                    continue;
                }

                findings.push(Finding {
                    gate: "file_locks".into(),
                    severity: Severity::Warning,
                    message: format!("{app_name} (PID {pid}) 正在使用此目录中的文件"),
                    detail: Some(format!("关闭 {app_name} 后即可迁移")),
                });
            }
        }
    }

    unsafe {
        let _ = RmEndSession(session);
    }
    findings
}

#[cfg(not(windows))]
fn gate_file_locks(_path: &Path) -> Vec<Finding> {
    Vec::new()
}

fn collect_files_for_lock_check(path: &Path) -> Vec<PathBuf> {
    // Restart Manager 的 RmGetList 需要把每个 file path 注册再轮询全系统进程，
    // 注册数量越多耗时越长。Program Files 这种大目录之前 ~3s 是因为注册了 64 个文件
    // 而且包含 .log/.dat 这类几乎不会被进程独占的扩展。
    //
    // 现在挑 32 个真正可能被锁的扩展（exe/dll/sys/drv/ocx/sqlite/mdb/ldb），
    // 配合 max_dirs=256 在 Program Files 单个应用目录下已足够覆盖。
    // 实测 Program Files 下单个应用目录很少有超过 32 个可能被锁的文件。
    let max_files = 32;
    let max_dirs = 256;

    if path.is_file() {
        return vec![path.to_path_buf()];
    }

    let mut priority = Vec::new();
    let mut pending = vec![path.to_path_buf()];
    let mut scanned_dirs = 0usize;

    while let Some(dir) = pending.pop() {
        if scanned_dirs >= max_dirs || priority.len() >= max_files {
            break;
        }
        scanned_dirs += 1;

        let Ok(entries) = crate::winfs::enumerate_directory(&dir, false) else {
            continue;
        };

        for entry in entries {
            if entry.is_symlink {
                continue;
            }
            if entry.is_dir {
                if pending.len() < max_dirs {
                    pending.push(entry.path);
                }
                continue;
            }

            let ext = entry
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            // 只挑真正会被进程独占锁的扩展。其他文件交给 fallback 是浪费 RM 注册槽位。
            if matches!(
                ext.as_str(),
                "exe" | "dll" | "sys" | "drv" | "ocx" | "sqlite" | "mdb" | "ldb"
            ) {
                priority.push(entry.path);
                if priority.len() >= max_files {
                    break;
                }
            }
        }
    }

    priority
}

#[cfg(windows)]
fn gate_boot_drivers(path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let path_upper = path.to_string_lossy().to_uppercase();
    for entry in &registry_index().services {
        if entry.start > 1 || entry.field != "ImagePath" {
            continue;
        }

        let is_kernel = entry.service_type == 1 || entry.service_type == 2;
        if !is_kernel {
            continue;
        }

        if normalized_path_is_under(&entry.normalized_upper, &path_upper) {
            let severity = if entry.start == 0 {
                Severity::Blocker
            } else {
                Severity::Warning
            };
            let label = if entry.start == 0 { "Boot" } else { "System" };

            findings.push(Finding {
                gate: "boot_driver".into(),
                severity,
                message: format!("{label} 级内核驱动 {} 位于此目录", entry.name),
                detail: Some(format!("驱动路径: {}", entry.value)),
            });
        }
    }

    findings
}

#[cfg(not(windows))]
fn gate_boot_drivers(_path: &Path) -> Vec<Finding> {
    Vec::new()
}

fn normalize_reg_path(raw: &str) -> String {
    let s = raw.trim().trim_matches('"');

    let s = s.strip_prefix("\\??\\").unwrap_or(s);
    let s = s
        .strip_prefix("\\SystemRoot\\")
        .map(|rest| format!("C:\\Windows\\{rest}"))
        .unwrap_or_else(|| s.to_string());

    let s = s.replace('/', "\\");

    let s = s
        .replace("%SystemRoot%", "C:\\Windows")
        .replace("%SYSTEMROOT%", "C:\\Windows")
        .replace("%systemroot%", "C:\\Windows")
        .replace("%ProgramFiles%", "C:\\Program Files")
        .replace("%PROGRAMFILES%", "C:\\Program Files")
        .replace("%ProgramFiles(x86)%", "C:\\Program Files (x86)")
        .replace("%PROGRAMFILES(X86)%", "C:\\Program Files (x86)")
        .replace("%ProgramW6432%", "C:\\Program Files")
        .replace("%PROGRAMW6432%", "C:\\Program Files")
        .replace("%CommonProgramFiles%", "C:\\Program Files\\Common Files")
        .replace("%COMMONPROGRAMFILES%", "C:\\Program Files\\Common Files")
        .replace(
            "%CommonProgramFiles(x86)%",
            "C:\\Program Files (x86)\\Common Files",
        )
        .replace(
            "%COMMONPROGRAMFILES(X86)%",
            "C:\\Program Files (x86)\\Common Files",
        )
        .replace("%ProgramData%", "C:\\ProgramData")
        .replace("%PROGRAMDATA%", "C:\\ProgramData")
        .replace("%ALLUSERSPROFILE%", "C:\\ProgramData")
        .replace("%SystemDrive%", "C:")
        .replace("%SYSTEMDRIVE%", "C:")
        .replace("%windir%", "C:\\Windows")
        .replace("%WINDIR%", "C:\\Windows");

    if (s.starts_with("system32\\") || s.starts_with("System32\\") || s.starts_with("SYSTEM32\\"))
        && !s.contains(':')
    {
        return format!("C:\\Windows\\{s}");
    }

    s
}

fn normalized_path_is_under(normalized: &str, dir_upper: &str) -> bool {
    if normalized.len() < dir_upper.len() {
        return false;
    }
    if !normalized.starts_with(dir_upper) {
        return false;
    }
    if normalized.len() == dir_upper.len() {
        return true;
    }
    let next_byte = normalized.as_bytes()[dir_upper.len()];
    next_byte == b'\\' || next_byte == b'/'
}

fn blob_contains_path_upper(haystack: &str, dir_upper: &str) -> bool {
    let mut search_from = 0;
    while let Some(pos) = haystack[search_from..].find(dir_upper) {
        let abs_pos = search_from + pos;
        let end_pos = abs_pos + dir_upper.len();

        let left_ok = abs_pos == 0 || {
            let prev = haystack.as_bytes()[abs_pos - 1];
            prev == b'\0'
                || prev == b'"'
                || prev == b' '
                || prev == b'\t'
                || prev == b'\n'
                || prev == b'\r'
        };

        let right_ok = end_pos >= haystack.len() || {
            let next = haystack.as_bytes()[end_pos];
            next == b'\\'
                || next == b'/'
                || next == b'\0'
                || next == b'"'
                || next == b' '
                || next == b'\t'
                || next == b'\n'
                || next == b'\r'
        };

        if left_ok && right_ok {
            return true;
        }

        search_from = abs_pos + 1;
    }
    false
}

#[cfg(windows)]
fn gate_hardlinks(path: &Path) -> Vec<Finding> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        FILE_CREATION_DISPOSITION, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let mut findings = Vec::new();
    let mut hardlink_files = Vec::new();
    let max_check = 100;
    let mut checked = 0;

    let walker = jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .follow_links(false)
        .max_depth(2);

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        if checked >= max_check {
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }

        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let should_check =
            matches!(ext.as_str(), "exe" | "dll" | "sys" | "drv" | "ocx") || checked < 50;

        if !should_check {
            continue;
        }

        checked += 1;
        let file_path = entry.path();
        let wide: Vec<u16> = file_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                0,
                FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0),
                None,
                FILE_CREATION_DISPOSITION(OPEN_EXISTING.0),
                FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_BACKUP_SEMANTICS.0),
                HANDLE::default(),
            )
        };

        let handle = match handle {
            Ok(h) => h,
            Err(_) => continue,
        };

        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(handle);
        }

        if ok.is_ok() && info.nNumberOfLinks > 1 {
            hardlink_files.push(file_path.to_string_lossy().to_string());
        }
    }

    if !hardlink_files.is_empty() {
        let sample: Vec<_> = hardlink_files.iter().take(3).cloned().collect();
        findings.push(Finding {
            gate: "hardlinks".into(),
            severity: Severity::Warning,
            message: format!(
                "发现 {} 个文件存在硬链接，跨卷迁移会断裂硬链接关系",
                hardlink_files.len()
            ),
            detail: Some(sample.join("\n")),
        });
    }

    findings
}

#[cfg(not(windows))]
fn gate_hardlinks(_path: &Path) -> Vec<Finding> {
    Vec::new()
}

fn gate_reparse_points(path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut reparse_dirs = Vec::new();

    // 实测（见 docs/bench/reparse_depth_study）：
    //   - C:\Program Files 全树仅 1 个 reparse point，depth=4 已完整覆盖
    //   - C:\Users\<user> depth=4 也已覆盖绝大多数有意义的 reparse points
    // 取 depth=4 + 20000 总数上限，在覆盖典型场景的同时大幅减少遍历耗时。
    const MAX_DEPTH: usize = 4;
    const MAX_ENTRIES: usize = 20_000;

    let walker = jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .follow_links(false)
        .max_depth(MAX_DEPTH);

    for (visited, entry) in walker.into_iter().filter_map(|e| e.ok()).enumerate() {
        if visited >= MAX_ENTRIES {
            break;
        }
        if entry.path() == path {
            continue;
        }
        if entry.file_type().is_symlink() || is_reparse_point(&entry.path()) {
            reparse_dirs.push(entry.path().to_string_lossy().to_string());
        }
    }

    if !reparse_dirs.is_empty() {
        let sample: Vec<_> = reparse_dirs.iter().take(3).cloned().collect();
        findings.push(Finding {
            gate: "reparse_points".into(),
            severity: Severity::Blocker,
            message: format!(
                "目录内包含 {} 个符号链接或重解析点，复制阶段会中止",
                reparse_dirs.len()
            ),
            detail: Some(sample.join("\n")),
        });
    }

    findings
}

#[cfg(windows)]
fn is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    std::fs::symlink_metadata(path)
        .map(|m| m.file_attributes() & 0x0400 != 0)
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn is_reparse_point(_path: &Path) -> bool {
    false
}

fn gate_target_volume(target_disk: Option<&str>, source_size: u64) -> Vec<Finding> {
    let mut findings = Vec::new();

    let target = match target_disk {
        Some(t) if !t.is_empty() => t,
        _ => return findings,
    };

    #[cfg(windows)]
    {
        let target_path = std::path::Path::new(target);
        if let Some(details) = crate::winfs::query_volume_details(target_path) {
            if !details.file_system.eq_ignore_ascii_case("NTFS") {
                findings.push(Finding {
                    gate: "target_volume".into(),
                    severity: Severity::Blocker,
                    message: format!(
                        "目标卷文件系统为 {}，仅支持迁移到 NTFS 卷",
                        details.file_system
                    ),
                    detail: Some("非 NTFS 卷无法保留 ACL、ADS、硬链接等 NTFS 语义特性，且 Junction 仅在 NTFS 上有效".into()),
                });
            }
        }

        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

        let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
        let mut free_bytes = 0u64;
        let ok = unsafe {
            GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), None, None, Some(&mut free_bytes))
        };

        if ok.is_ok() {
            let required = required_target_space(source_size);

            if free_bytes < required {
                findings.push(Finding {
                    gate: "target_volume".into(),
                    severity: Severity::Blocker,
                    message: format!(
                        "目标卷空间不足: 需要 {:.1} GB，可用 {:.1} GB",
                        required as f64 / (1024.0 * 1024.0 * 1024.0),
                        free_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                    ),
                    detail: None,
                });
            }
        }
    }

    findings
}

fn required_target_space(source_size: u64) -> u64 {
    if source_size == 0 {
        return 0;
    }
    if source_size < 64 * 1024 * 1024 {
        return source_size;
    }
    let reserve = (source_size / 100).clamp(64 * 1024 * 1024, 512 * 1024 * 1024);
    source_size.saturating_add(reserve)
}

#[cfg(windows)]
fn gate_registry_bindings(path: &Path, link_type: &LinkType) -> Vec<Finding> {
    if !matches!(link_type, LinkType::None) {
        return Vec::new();
    }

    let mut findings = Vec::new();
    let path_upper = path.to_string_lossy().to_uppercase();
    let index = registry_index();

    let mut bound_services = Vec::new();
    for entry in &index.services {
        if entry.start <= 3 && normalized_path_is_under(&entry.normalized_upper, &path_upper) {
            bound_services.push(format!("{} ({})", entry.name, entry.field));
        }
    }

    if !bound_services.is_empty() {
        findings.push(Finding {
            gate: "registry_bindings".into(),
            severity: Severity::Blocker,
            message: format!(
                "无链接模式下，{} 个 Windows 服务的路径绑定将断裂",
                bound_services.len()
            ),
            detail: Some(
                bound_services
                    .into_iter()
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        });
    }

    let mut bound_com = Vec::new();
    for entry in &index.com {
        if normalized_path_is_under(&entry.normalized_upper, &path_upper) {
            bound_com.push(format!("{} ({})", entry.display, entry.field));
        }
    }

    if !bound_com.is_empty() {
        findings.push(Finding {
            gate: "registry_bindings".into(),
            severity: Severity::Blocker,
            message: format!(
                "无链接模式下，{} 个 COM 组件的路径绑定将断裂",
                bound_com.len()
            ),
            detail: Some(bound_com.into_iter().take(5).collect::<Vec<_>>().join(", ")),
        });
    }

    let mut bound_app_paths = Vec::new();
    for entry in &index.app_paths {
        if normalized_path_is_under(&entry.normalized_upper, &path_upper) {
            bound_app_paths.push(entry.display.clone());
        }
    }

    if !bound_app_paths.is_empty() {
        findings.push(Finding {
            gate: "registry_bindings".into(),
            severity: Severity::Warning,
            message: format!(
                "无链接模式下，{} 个 App Paths 注册项将断裂",
                bound_app_paths.len()
            ),
            detail: Some(
                bound_app_paths
                    .into_iter()
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        });
    }

    let mut bound_tasks = Vec::new();
    for entry in &index.tasks {
        if blob_contains_path_upper(&entry.value_upper, &path_upper) {
            bound_tasks.push(entry.display.clone());
        }
    }

    if !bound_tasks.is_empty() {
        findings.push(Finding {
            gate: "registry_bindings".into(),
            severity: Severity::Warning,
            message: format!(
                "无链接模式下，{} 个计划任务的路径绑定将断裂",
                bound_tasks.len()
            ),
            detail: Some(
                bound_tasks
                    .into_iter()
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        });
    }

    let mut bound_apps = Vec::new();
    for entry in &index.uninstall {
        if normalized_path_is_under(&entry.normalized_upper, &path_upper) {
            bound_apps.push(entry.display.clone());
        }
    }

    if !bound_apps.is_empty() {
        findings.push(Finding {
            gate: "registry_bindings".into(),
            severity: Severity::Warning,
            message: format!(
                "无链接模式下，{} 个已安装应用的注册表路径将断裂",
                bound_apps.len()
            ),
            detail: Some(
                bound_apps
                    .into_iter()
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        });
    }

    findings
}

fn decode_reg_binary_as_paths(bytes: &[u8]) -> String {
    if bytes.len() >= 2 {
        let u16_iter = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]));
        let decoded = String::from_utf16_lossy(&u16_iter.collect::<Vec<u16>>());
        if decoded.contains('\\') || decoded.contains(':') {
            return decoded;
        }
    }
    String::from_utf8_lossy(bytes).to_string()
}

#[cfg(windows)]
fn registry_index() -> &'static RegistryIndex {
    static INDEX: OnceLock<RegistryIndex> = OnceLock::new();
    INDEX.get_or_init(build_registry_index)
}

#[cfg(windows)]
fn build_registry_index() -> RegistryIndex {
    use winreg::enums::*;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut index = RegistryIndex::default();

    if let Ok(services_key) = hklm.open_subkey("SYSTEM\\CurrentControlSet\\Services") {
        for name in services_key.enum_keys().filter_map(Result::ok) {
            let Ok(subkey) = services_key.open_subkey(&name) else {
                continue;
            };
            let start: u32 = subkey.get_value("Start").unwrap_or(4);
            let service_type: u32 = subkey.get_value("Type").unwrap_or(0);

            if let Ok(image_path) = subkey.get_value::<String, _>("ImagePath") {
                push_service_binding(
                    &mut index.services,
                    &name,
                    "ImagePath",
                    image_path,
                    start,
                    service_type,
                );
            }

            if let Ok(service_dll) = subkey
                .open_subkey("Parameters")
                .and_then(|p| p.get_value::<String, _>("ServiceDll"))
            {
                push_service_binding(
                    &mut index.services,
                    &name,
                    "ServiceDll",
                    service_dll,
                    start,
                    service_type,
                );
            }
        }
    }

    for clsid_root in [
        "SOFTWARE\\Classes\\CLSID",
        "SOFTWARE\\WOW6432Node\\Classes\\CLSID",
    ] {
        let Ok(clsid_key) = hklm.open_subkey(clsid_root) else {
            continue;
        };
        for guid in clsid_key.enum_keys().filter_map(Result::ok) {
            let Ok(subkey) = clsid_key.open_subkey(&guid) else {
                continue;
            };
            for server_key_name in ["InprocServer32", "LocalServer32"] {
                let Ok(server_key) = subkey.open_subkey(server_key_name) else {
                    continue;
                };
                let Ok(path) = server_key.get_value::<String, _>("") else {
                    continue;
                };
                let display: String = subkey.get_value("").unwrap_or(guid.clone());
                push_path_binding(&mut index.com, display, server_key_name, path);
            }
        }
    }

    if let Ok(app_paths_key) =
        hklm.open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths")
    {
        for name in app_paths_key.enum_keys().filter_map(Result::ok) {
            let Ok(subkey) = app_paths_key.open_subkey(&name) else {
                continue;
            };
            if let Ok(exe_path) = subkey.get_value::<String, _>("") {
                push_path_binding(&mut index.app_paths, name, "AppPath", exe_path);
            }
        }
    }

    if let Ok(tasks_key) = hklm
        .open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Schedule\\TaskCache\\Tasks")
    {
        for name in tasks_key.enum_keys().filter_map(Result::ok) {
            let Ok(subkey) = tasks_key.open_subkey(&name) else {
                continue;
            };
            if let Ok(raw) = subkey.get_raw_value("Actions") {
                let display: String = subkey.get_value("Path").unwrap_or(name);
                index.tasks.push(RegistryBlobBinding {
                    display,
                    value_upper: decode_reg_binary_as_paths(&raw.bytes).to_uppercase(),
                });
            }
        }
    }

    for reg_path in [
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    ] {
        let Ok(key) = hklm.open_subkey(reg_path) else {
            continue;
        };
        for subkey_name in key.enum_keys().filter_map(Result::ok) {
            let Ok(subkey) = key.open_subkey(&subkey_name) else {
                continue;
            };
            let Ok(loc) = subkey.get_value::<String, _>("InstallLocation") else {
                continue;
            };
            if loc.is_empty() {
                continue;
            }
            let display: String = subkey.get_value("DisplayName").unwrap_or(subkey_name);
            push_path_binding(&mut index.uninstall, display, "InstallLocation", loc);
        }
    }

    index
}

#[cfg(windows)]
fn push_service_binding(
    bindings: &mut Vec<RegistryServiceBinding>,
    name: &str,
    field: &str,
    value: String,
    start: u32,
    service_type: u32,
) {
    bindings.push(RegistryServiceBinding {
        name: name.to_string(),
        field: field.to_string(),
        normalized_upper: normalize_reg_path(&value).to_uppercase(),
        value,
        start,
        service_type,
    });
}

#[cfg(windows)]
fn push_path_binding(
    bindings: &mut Vec<RegistryPathBinding>,
    display: String,
    field: &str,
    value: String,
) {
    bindings.push(RegistryPathBinding {
        display,
        field: field.to_string(),
        normalized_upper: normalize_reg_path(&value).to_uppercase(),
    });
}

#[cfg(not(windows))]
fn gate_registry_bindings(_path: &Path, _link_type: &LinkType) -> Vec<Finding> {
    Vec::new()
}

fn identify_app_type(path: &Path, path_upper: &str) -> String {
    if path.join("steam_appid.txt").exists()
        || path_upper.contains("STEAMAPPS")
        || path_upper.contains("STEAM\\COMMON")
    {
        return "Steam 游戏".into();
    }
    if path_upper.contains("EPIC GAMES") || path.join(".egstore").exists() {
        return "Epic 游戏".into();
    }
    if path.join("portable.txt").exists()
        || path.join("portable.ini").exists()
        || path.join("App\\AppInfo\\appinfo.ini").exists()
    {
        return "便携式应用".into();
    }
    if path_upper.contains("\\USERS\\") {
        if path_upper.contains("\\DOWNLOADS")
            || path_upper.contains("\\DOCUMENTS")
            || path_upper.contains("\\DESKTOP")
        {
            return "用户数据".into();
        }
        if path_upper.contains("\\VIDEOS")
            || path_upper.contains("\\PICTURES")
            || path_upper.contains("\\MUSIC")
        {
            return "媒体文件".into();
        }
    }
    if path_upper.contains("\\TEMP") || path_upper.contains("\\TMP") {
        return "临时文件".into();
    }
    if path_upper.contains("\\GAMES\\") || path_upper.contains("\\GAME\\") {
        return "游戏".into();
    }
    "未知类型".into()
}
