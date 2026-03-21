use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;
use crate::migration::LinkType;

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
}

pub fn analyze(
    path: &Path,
    link_type: LinkType,
    target_disk: Option<&str>,
    source_size: u64,
) -> MigrationSafety {
    let start = Instant::now();
    let path_upper = path.to_string_lossy().to_uppercase();
    let app_type = identify_app_type(path, &path_upper);
    let mut findings = Vec::new();

    gate_system_critical(&path_upper, &mut findings);

    if !has_blocker(&findings) {
        let handles = GateHandles::run_parallel(path, &link_type, target_disk, source_size);
        findings.extend(handles.collect());
    }

    let verdict = derive_verdict(&findings);
    let can_migrate = matches!(verdict, Verdict::Safe | Verdict::SafeAfterAction);
    let required_actions = extract_actions(&findings);

    MigrationSafety {
        verdict,
        can_migrate,
        findings,
        required_actions,
        app_type,
        analysis_duration_ms: start.elapsed().as_millis() as u64,
    }
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
    findings
        .iter()
        .filter(|f| f.severity == Severity::Warning && f.gate == "file_locks")
        .filter_map(|f| f.detail.clone())
        .collect()
}

struct GateHandles {
    file_locks: std::thread::JoinHandle<Vec<Finding>>,
    boot_drivers: std::thread::JoinHandle<Vec<Finding>>,
    hardlinks: std::thread::JoinHandle<Vec<Finding>>,
    reparse_points: std::thread::JoinHandle<Vec<Finding>>,
    target_volume: std::thread::JoinHandle<Vec<Finding>>,
    registry_bindings: std::thread::JoinHandle<Vec<Finding>>,
}

impl GateHandles {
    fn run_parallel(path: &Path, link_type: &LinkType, target_disk: Option<&str>, source_size: u64) -> Self {
        let p1 = path.to_path_buf();
        let p2 = path.to_path_buf();
        let p3 = path.to_path_buf();
        let p4 = path.to_path_buf();
        let p5 = path.to_path_buf();
        let lt = link_type.clone();
        let td = target_disk.map(String::from);

        Self {
            file_locks: std::thread::spawn(move || gate_file_locks(&p1)),
            boot_drivers: std::thread::spawn(move || gate_boot_drivers(&p2)),
            hardlinks: std::thread::spawn(move || gate_hardlinks(&p3)),
            reparse_points: std::thread::spawn(move || gate_reparse_points(&p4)),
            target_volume: std::thread::spawn(move || gate_target_volume(td.as_deref(), source_size)),
            registry_bindings: std::thread::spawn(move || gate_registry_bindings(&p5, &lt)),
        }
    }

    fn collect(self) -> Vec<Finding> {
        let mut all = Vec::new();
        for handle in [
            self.file_locks,
            self.boot_drivers,
            self.hardlinks,
            self.reparse_points,
            self.target_volume,
            self.registry_bindings,
        ] {
            if let Ok(findings) = handle.join() {
                all.extend(findings);
            }
        }
        all
    }
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
    ("C:\\PROGRAMDATA\\MICROSOFT\\WINDOWS\\START MENU", "开始菜单"),
    ("C:\\PROGRAMDATA\\MICROSOFT\\CRYPTO", "系统加密存储"),
    ("C:\\PROGRAMDATA\\MICROSOFT\\WINDOWS DEFENDER", "Windows Defender"),
    ("C:\\SYSTEM VOLUME INFORMATION", "系统卷信息"),
    ("C:\\$RECYCLE.BIN", "回收站"),
    ("C:\\RECOVERY", "系统恢复分区"),
    ("C:\\BOOT", "引导加载器"),
    ("\\APPDATA\\LOCAL\\MICROSOFT\\WINDOWS\\", "Windows 用户系统数据"),
    ("\\APPDATA\\ROAMING\\MICROSOFT\\WINDOWS\\", "Windows 用户配置"),
    ("\\APPDATA\\LOCAL\\PACKAGES\\", "UWP 应用数据"),
    ("\\APPDATA\\ROAMING\\MICROSOFT\\PROTECT\\", "Windows 凭据保护"),
    ("\\APPDATA\\LOCAL\\MICROSOFT\\CREDENTIALS\\", "系统凭据"),
    ("\\APPDATA\\LOCAL\\CONNECTEDDEVICESPLATFORM\\", "设备平台数据"),
];

fn gate_system_critical(path_upper: &str, findings: &mut Vec<Finding>) {
    if path_upper.starts_with("C:\\WINDOWS")
        && !path_upper.starts_with("C:\\WINDOWS\\INSTALLER")
        && !path_upper.starts_with("C:\\WINDOWS\\TEMP")
    {
        let matched = CRITICAL_PATHS.iter().find(|(p, _)| path_upper.starts_with(p));
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
    use windows::Win32::System::RestartManager::{
        RmEndSession, RmGetList, RmRegisterResources, RmStartSession,
    };
    use windows::core::{PCWSTR, PWSTR};

    let mut findings = Vec::new();
    let mut session: u32 = 0;
    let mut session_key = [0u16; 64];

    let start_result = unsafe {
        RmStartSession(&mut session, 0, PWSTR(session_key.as_mut_ptr()))
    };
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
        .map(|f| f.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect())
        .collect();
    let ptrs: Vec<PCWSTR> = wide_paths.iter().map(|w| PCWSTR(w.as_ptr())).collect();

    let reg_result = unsafe {
        RmRegisterResources(session, Some(&ptrs), None, None)
    };
    if reg_result.is_err() {
        let _ = unsafe { RmEndSession(session) };
        return findings;
    }

    let mut needed: u32 = 0;
    let mut count: u32 = 0;
    let mut reboot_reasons: u32 = 0;

    let _ = unsafe {
        RmGetList(session, &mut needed, &mut count, None, &mut reboot_reasons)
    };

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

    unsafe { let _ = RmEndSession(session); }
    findings
}

#[cfg(not(windows))]
fn gate_file_locks(_path: &Path) -> Vec<Finding> {
    Vec::new()
}

fn collect_files_for_lock_check(path: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let max_files = 64;

    if path.is_file() {
        return vec![path.to_path_buf()];
    }

    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return files,
    };

    for entry in entries.flatten() {
        if files.len() >= max_files {
            break;
        }
        let p = entry.path();
        if p.is_file() {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if matches!(ext.as_str(), "exe" | "dll" | "sys" | "db" | "lock" | "log" | "dat" | "mdb" | "ldb") {
                files.push(p);
            }
        }
    }

    if files.is_empty() {
        for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
            if files.len() >= max_files {
                break;
            }
            let p = entry.path();
            if p.is_file() {
                files.push(p);
            }
        }
    }

    files
}

#[cfg(windows)]
fn gate_boot_drivers(path: &Path) -> Vec<Finding> {
    use winreg::enums::*;
    use winreg::RegKey;

    let mut findings = Vec::new();
    let path_upper = path.to_string_lossy().to_uppercase();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    let services_key = match hklm.open_subkey("SYSTEM\\CurrentControlSet\\Services") {
        Ok(k) => k,
        Err(_) => return findings,
    };

    for name in services_key.enum_keys().filter_map(Result::ok) {
        let subkey = match services_key.open_subkey(&name) {
            Ok(k) => k,
            Err(_) => continue,
        };

        let start: u32 = match subkey.get_value("Start") {
            Ok(v) => v,
            Err(_) => continue,
        };

        if start > 1 {
            continue;
        }

        let svc_type: u32 = subkey.get_value("Type").unwrap_or(0);
        let is_kernel = svc_type == 1 || svc_type == 2;
        if !is_kernel {
            continue;
        }

        let image_path: String = match subkey.get_value("ImagePath") {
            Ok(v) => v,
            Err(_) => continue,
        };

        let resolved = resolve_driver_path(&image_path);
        if path_is_under(&resolved, &path_upper) {
            let severity = if start == 0 {
                Severity::Blocker
            } else {
                Severity::Warning
            };
            let label = if start == 0 { "Boot" } else { "System" };

            findings.push(Finding {
                gate: "boot_driver".into(),
                severity,
                message: format!(
                    "{label} 级内核驱动 {name} 位于此目录",
                ),
                detail: Some(format!("驱动路径: {image_path}")),
            });
        }
    }

    findings
}

#[cfg(not(windows))]
fn gate_boot_drivers(_path: &Path) -> Vec<Finding> {
    Vec::new()
}

fn resolve_driver_path(image_path: &str) -> String {
    normalize_reg_path(image_path)
}

fn normalize_reg_path(raw: &str) -> String {
    let s = raw.trim().trim_matches('"');

    let s = s.strip_prefix("\\??\\").unwrap_or(s);
    let s = s.strip_prefix("\\SystemRoot\\")
        .map(|rest| format!("C:\\Windows\\{rest}"))
        .unwrap_or_else(|| s.to_string());

    let s = s.replace("%SystemRoot%", "C:\\Windows")
        .replace("%SYSTEMROOT%", "C:\\Windows")
        .replace("%systemroot%", "C:\\Windows")
        .replace("%ProgramFiles%", "C:\\Program Files")
        .replace("%PROGRAMFILES%", "C:\\Program Files")
        .replace("%ProgramFiles(x86)%", "C:\\Program Files (x86)")
        .replace("%PROGRAMFILES(X86)%", "C:\\Program Files (x86)")
        .replace("%windir%", "C:\\Windows")
        .replace("%WINDIR%", "C:\\Windows");

    if (s.starts_with("system32\\") || s.starts_with("System32\\") || s.starts_with("SYSTEM32\\"))
        && !s.contains(':')
    {
        return format!("C:\\Windows\\{s}");
    }

    s
}

fn path_is_under(candidate: &str, dir_upper: &str) -> bool {
    let normalized = normalize_reg_path(candidate).to_uppercase();
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

#[cfg(windows)]
fn gate_hardlinks(path: &Path) -> Vec<Finding> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE,
        FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        FILE_CREATION_DISPOSITION,
    };
    use windows::core::PCWSTR;
    use std::os::windows::ffi::OsStrExt;

    let mut findings = Vec::new();
    let mut hardlink_files = Vec::new();
    let max_check = 200;
    let mut checked = 0;

    let walker = jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .follow_links(false)
        .max_depth(3);

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        if checked >= max_check {
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }

        let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let should_check = matches!(ext.as_str(), "exe" | "dll" | "sys" | "drv" | "ocx")
            || checked < 50;

        if !should_check {
            continue;
        }

        checked += 1;
        let file_path = entry.path();
        let wide: Vec<u16> = file_path.as_os_str()
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
        unsafe { let _ = windows::Win32::Foundation::CloseHandle(handle); }

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

    let walker = jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .follow_links(false);

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
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

        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        use windows::core::PCWSTR;

        let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
        let mut free_bytes = 0u64;
        let ok = unsafe {
            GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), None, None, Some(&mut free_bytes))
        };

        if ok.is_ok() {
            let required = if source_size > 0 {
                source_size + 100 * 1024 * 1024
            } else {
                100 * 1024 * 1024
            };

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

#[cfg(windows)]
fn gate_registry_bindings(path: &Path, link_type: &LinkType) -> Vec<Finding> {
    if !matches!(link_type, LinkType::None) {
        return Vec::new();
    }

    use winreg::enums::*;
    use winreg::RegKey;

    let mut findings = Vec::new();
    let path_upper = path.to_string_lossy().to_uppercase();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    let mut bound_services = Vec::new();
    if let Ok(services_key) = hklm.open_subkey("SYSTEM\\CurrentControlSet\\Services") {
        for name in services_key.enum_keys().filter_map(Result::ok) {
            if let Ok(subkey) = services_key.open_subkey(&name) {
                let start: u32 = subkey.get_value("Start").unwrap_or(4);
                if start > 3 {
                    continue;
                }

                if let Ok(image_path) = subkey.get_value::<String, _>("ImagePath") {
                    if path_is_under(&image_path, &path_upper) {
                        bound_services.push(format!("{name} (ImagePath)"));
                    }
                }

                if let Ok(service_dll) = subkey
                    .open_subkey("Parameters")
                    .and_then(|p| p.get_value::<String, _>("ServiceDll"))
                {
                    if path_is_under(&service_dll, &path_upper) {
                        bound_services.push(format!("{name} (ServiceDll)"));
                    }
                }
            }
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
            detail: Some(bound_services.into_iter().take(5).collect::<Vec<_>>().join(", ")),
        });
    }

    let mut bound_com = Vec::new();
    let clsid_paths = [
        "SOFTWARE\\Classes\\CLSID",
        "SOFTWARE\\WOW6432Node\\Classes\\CLSID",
    ];
    for clsid_root in &clsid_paths {
        let clsid_key = match hklm.open_subkey(clsid_root) {
            Ok(k) => k,
            Err(_) => continue,
        };
        for guid in clsid_key.enum_keys().filter_map(Result::ok) {
            let subkey = match clsid_key.open_subkey(&guid) {
                Ok(k) => k,
                Err(_) => continue,
            };

            for server_key_name in ["InprocServer32", "LocalServer32"] {
                if let Ok(server_key) = subkey.open_subkey(server_key_name) {
                    if let Ok(dll_path) = server_key.get_value::<String, _>("") {
                        if path_is_under(&dll_path, &path_upper) {
                            let display: String = subkey.get_value("").unwrap_or(guid.clone());
                            bound_com.push(format!("{display} ({server_key_name})"));
                        }
                    }
                }
            }
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
    if let Ok(app_paths_key) = hklm.open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths") {
        for name in app_paths_key.enum_keys().filter_map(Result::ok) {
            if let Ok(subkey) = app_paths_key.open_subkey(&name) {
                if let Ok(exe_path) = subkey.get_value::<String, _>("") {
                    if path_is_under(&exe_path, &path_upper) {
                        bound_app_paths.push(name);
                    }
                }
            }
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
            detail: Some(bound_app_paths.into_iter().take(5).collect::<Vec<_>>().join(", ")),
        });
    }

    let mut bound_tasks = Vec::new();
    let task_path = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Schedule\\TaskCache\\Tasks";
    if let Ok(tasks_key) = hklm.open_subkey(task_path) {
        for name in tasks_key.enum_keys().filter_map(Result::ok) {
            if let Ok(subkey) = tasks_key.open_subkey(&name) {
                if let Ok(raw) = subkey.get_raw_value("Actions") {
                    let utf16_str = decode_reg_binary_as_paths(&raw.bytes);
                    if path_is_under(&utf16_str, &path_upper) {
                        let display: String = subkey.get_value("Path").unwrap_or(name);
                        bound_tasks.push(display);
                    }
                }
            }
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
            detail: Some(bound_tasks.into_iter().take(5).collect::<Vec<_>>().join(", ")),
        });
    }

    let uninstall_paths = [
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    ];
    let mut bound_apps = Vec::new();
    for reg_path in &uninstall_paths {
        if let Ok(key) = hklm.open_subkey(reg_path) {
            for subkey_name in key.enum_keys().filter_map(Result::ok) {
                if let Ok(subkey) = key.open_subkey(&subkey_name) {
                    if let Ok(loc) = subkey.get_value::<String, _>("InstallLocation") {
                        if !loc.is_empty() && loc.to_uppercase().starts_with(&path_upper) {
                            let display: String = subkey
                                .get_value("DisplayName")
                                .unwrap_or(subkey_name);
                            bound_apps.push(display);
                        }
                    }
                }
            }
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
            detail: Some(bound_apps.into_iter().take(5).collect::<Vec<_>>().join(", ")),
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

