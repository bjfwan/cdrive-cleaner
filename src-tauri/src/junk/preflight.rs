use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use super::rules::{all_rules, JunkRiskLevel};

pub const RECENCY_THRESHOLD: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Deserialize)]
pub struct DryRunInput {
    pub path: String,
    pub rule_id: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileItemPreview {
    pub path: String,
    pub size_mb: u64,
    pub mtime_iso: String,
    pub rule_id: String,
    pub is_symlink: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DrySkipReason {
    pub path: String,
    pub rule_id: String,
    pub reason: &'static str,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DryRunReport {
    pub will_delete: Vec<FileItemPreview>,
    pub will_skip: Vec<DrySkipReason>,
    pub estimated_freed_mb: u64,
    pub estimated_seconds: u64,
}

pub fn run_dry_run(items: Vec<DryRunInput>) -> DryRunReport {
    let blocked_rules = parent_child_lockset();
    let mut will_delete: Vec<FileItemPreview> = Vec::with_capacity(items.len());
    let mut will_skip: Vec<DrySkipReason> = Vec::new();

    for item in items {
        if blocked_rules.contains(item.rule_id.as_str()) {
            will_skip.push(DrySkipReason {
                path: item.path.clone(),
                rule_id: item.rule_id.clone(),
                reason: "parent_child_conflict",
                detail: Some(
                    "此规则的清理路径覆盖了其他更细粒度规则的目标，跳过避免误删".into(),
                ),
            });
            continue;
        }

        let path = Path::new(&item.path);
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(_) => {
                will_skip.push(DrySkipReason {
                    path: item.path,
                    rule_id: item.rule_id,
                    reason: "not_found",
                    detail: None,
                });
                continue;
            }
        };

        let is_symlink = metadata.file_type().is_symlink();
        if let Some(age) = recently_touched(&metadata) {
            will_skip.push(DrySkipReason {
                path: item.path.clone(),
                rule_id: item.rule_id.clone(),
                reason: "too_recent",
                detail: Some(format_age_detail(age)),
            });
            continue;
        }

        if !is_symlink {
            if let Some(reason) = check_lock(path) {
                will_skip.push(DrySkipReason {
                    path: item.path.clone(),
                    rule_id: item.rule_id.clone(),
                    reason: reason.code,
                    detail: reason.detail,
                });
                continue;
            }
        }

        let size_mb = item.size_bytes / (1024 * 1024);
        let mtime_iso = mtime_to_iso(&metadata);
        will_delete.push(FileItemPreview {
            path: item.path,
            size_mb,
            mtime_iso,
            rule_id: item.rule_id,
            is_symlink,
        });
    }

    let estimated_freed_mb: u64 = will_delete.iter().map(|p| p.size_mb).sum();
    let estimated_seconds = estimate_seconds(&will_delete);

    DryRunReport {
        will_delete,
        will_skip,
        estimated_freed_mb,
        estimated_seconds,
    }
}

fn recently_touched(metadata: &std::fs::Metadata) -> Option<Duration> {
    let now = SystemTime::now();
    let mut newest: Option<SystemTime> = None;
    for ts in [metadata.modified().ok(), metadata.accessed().ok()] {
        if let Some(t) = ts {
            newest = Some(match newest {
                Some(prev) if prev > t => prev,
                _ => t,
            });
        }
    }
    let stamp = newest?;
    let age = now.duration_since(stamp).ok()?;
    if age < RECENCY_THRESHOLD {
        Some(age)
    } else {
        None
    }
}

fn format_age_detail(age: Duration) -> String {
    let secs = age.as_secs();
    if secs < 60 {
        format!("{} 秒前还在使用", secs)
    } else {
        format!("{} 分钟前还在使用", secs / 60)
    }
}

fn mtime_to_iso(metadata: &std::fs::Metadata) -> String {
    let modified = match metadata.modified().ok() {
        Some(t) => t,
        None => return String::new(),
    };
    let duration_since_epoch = match modified.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    format_unix_seconds_local(duration_since_epoch.as_secs() as i64)
}

fn format_unix_seconds_local(unix_secs: i64) -> String {
    let secs_per_day: i64 = 86_400;
    let days_since_epoch = unix_secs.div_euclid(secs_per_day);
    let day_secs = unix_secs.rem_euclid(secs_per_day);
    let (y, m, d) = civil_from_days(days_since_epoch);
    let hh = day_secs / 3600;
    let mm = (day_secs % 3600) / 60;
    let ss = day_secs % 60;
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y, m, d, hh, mm, ss
    )
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn estimate_seconds(items: &[FileItemPreview]) -> u64 {
    let total_mb: u64 = items.iter().map(|i| i.size_mb).sum();
    let mb_secs = total_mb / 80;
    let item_secs = (items.len() as u64) / 50;
    mb_secs.saturating_add(item_secs).max(1)
}

struct LockHit {
    code: &'static str,
    detail: Option<String>,
}

#[cfg(windows)]
fn check_lock(path: &Path) -> Option<LockHit> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, DELETE, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    const ERROR_ACCESS_DENIED_W32: u32 = 5;
    const ERROR_SHARING_VIOLATION_W32: u32 = 32;
    const ERROR_LOCK_VIOLATION_W32: u32 = 33;
    const ERROR_FILE_NOT_FOUND_W32: u32 = 2;
    const ERROR_PATH_NOT_FOUND_W32: u32 = 3;

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            DELETE.0,
            FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0),
            None,
            windows::Win32::Storage::FileSystem::FILE_CREATION_DISPOSITION(OPEN_EXISTING.0),
            FILE_FLAGS_AND_ATTRIBUTES(FILE_FLAG_BACKUP_SEMANTICS.0),
            windows::Win32::Foundation::HANDLE::default(),
        )
    };
    match handle {
        Ok(h) => {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(h);
            }
            None
        }
        Err(e) => {
            let win32 = (e.code().0 as u32) & 0xFFFF;
            match win32 {
                ERROR_SHARING_VIOLATION_W32 | ERROR_LOCK_VIOLATION_W32 => Some(LockHit {
                    code: "locked_by_process",
                    detail: Some("文件被其他进程独占打开".into()),
                }),
                ERROR_ACCESS_DENIED_W32 => Some(LockHit {
                    code: "permission_denied",
                    detail: Some("权限不足，建议以管理员运行".into()),
                }),
                ERROR_FILE_NOT_FOUND_W32 | ERROR_PATH_NOT_FOUND_W32 => Some(LockHit {
                    code: "not_found",
                    detail: None,
                }),
                other => Some(LockHit {
                    code: "locked_by_process",
                    detail: Some(format!("无法以删除权限打开（Win32 错误 {}）", other)),
                }),
            }
        }
    }
}

#[cfg(not(windows))]
fn check_lock(_path: &Path) -> Option<LockHit> {
    None
}

fn parent_child_lockset() -> &'static HashSet<&'static str> {
    static CACHE: OnceLock<HashSet<&'static str>> = OnceLock::new();
    CACHE.get_or_init(build_parent_child_lockset)
}

fn build_parent_child_lockset() -> HashSet<&'static str> {
    let rules = all_rules();
    let mut blocked: HashSet<&'static str> = HashSet::new();

    let prepared: Vec<(&'static str, JunkRiskLevel, Vec<String>)> = rules
        .iter()
        .map(|r| {
            let normalized: Vec<String> = r
                .paths
                .iter()
                .map(|p| normalize_for_compare(p))
                .filter(|p| !p.is_empty())
                .collect();
            (r.id, r.risk_level, normalized)
        })
        .collect();

    for (outer_id, outer_risk, outer_paths) in &prepared {
        if !matches!(outer_risk, JunkRiskLevel::Safe) {
            continue;
        }
        if outer_paths.is_empty() {
            continue;
        }
        for (inner_id, inner_risk, inner_paths) in &prepared {
            if outer_id == inner_id {
                continue;
            }
            if !matches!(inner_risk, JunkRiskLevel::Safe) {
                continue;
            }
            for op in outer_paths {
                for ip in inner_paths {
                    if path_is_strict_ancestor(op, ip) {
                        blocked.insert(*outer_id);
                    }
                }
            }
        }
    }

    blocked
}

fn normalize_for_compare(raw: &str) -> String {
    let mut s = raw.replace('/', "\\");
    while s.ends_with('\\') {
        s.pop();
    }
    s.to_uppercase()
}

pub(crate) fn path_is_strict_ancestor(parent: &str, child: &str) -> bool {
    if parent.is_empty() || child.is_empty() {
        return false;
    }
    if child.len() <= parent.len() {
        return false;
    }
    if !child.starts_with(parent) {
        return false;
    }
    matches!(child.as_bytes().get(parent.len()), Some(&b'\\'))
}

#[allow(dead_code)]
pub(crate) fn collect_rule_paths() -> Vec<(&'static str, Vec<String>)> {
    all_rules()
        .into_iter()
        .map(|r| (r.id, r.paths))
        .collect()
}

#[allow(dead_code)]
pub(crate) fn target_paths_for(rule_id: &str) -> Option<Vec<PathBuf>> {
    all_rules()
        .into_iter()
        .find(|r| r.id == rule_id)
        .map(|r| r.paths.iter().map(PathBuf::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestor_basic() {
        assert!(path_is_strict_ancestor(
            "C:\\USERS\\X\\APPDATA\\LOCAL\\FOO",
            "C:\\USERS\\X\\APPDATA\\LOCAL\\FOO\\CACHE"
        ));
        assert!(!path_is_strict_ancestor(
            "C:\\USERS\\X\\APPDATA\\LOCAL\\FOO",
            "C:\\USERS\\X\\APPDATA\\LOCAL\\FOO"
        ));
        assert!(!path_is_strict_ancestor(
            "C:\\USERS\\X\\APPDATA\\LOCAL\\FOO",
            "C:\\USERS\\X\\APPDATA\\LOCAL\\FOOBAR"
        ));
        assert!(!path_is_strict_ancestor("", "C:\\X"));
    }

    #[test]
    fn normalize_strips_trailing_backslash() {
        assert_eq!(normalize_for_compare("C:\\Foo\\Bar\\"), "C:\\FOO\\BAR");
        assert_eq!(normalize_for_compare("C:\\Foo/Bar"), "C:\\FOO\\BAR");
    }

    #[test]
    fn lockset_smoke() {
        let _ = parent_child_lockset();
    }

    #[test]
    fn recency_within_threshold_blocks() {
        use std::fs::OpenOptions;
        let tmp = std::env::temp_dir().join("junk_preflight_recency_test");
        let _ = std::fs::remove_file(&tmp);
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .unwrap();
        let meta = std::fs::symlink_metadata(&tmp).unwrap();
        let result = recently_touched(&meta);
        assert!(result.is_some(), "freshly created file should be flagged");
        let _ = std::fs::remove_file(&tmp);
    }

    #[cfg(windows)]
    #[test]
    fn lock_check_returns_none_for_unlocked_file() {
        let tmp = std::env::temp_dir().join("junk_preflight_unlocked_test");
        let _ = std::fs::remove_file(&tmp);
        std::fs::write(&tmp, b"hello").unwrap();
        let verdict = check_lock(&tmp);
        assert!(verdict.is_none(), "writable closed file should not be locked");
        let _ = std::fs::remove_file(&tmp);
    }

    #[cfg(windows)]
    #[test]
    fn lock_check_detects_held_file() {
        use std::fs::OpenOptions;
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        const FILE_SHARE_WRITE: u32 = 0x2;
        let tmp = std::env::temp_dir().join("junk_preflight_held_test");
        let _ = std::fs::remove_file(&tmp);
        std::fs::write(&tmp, b"hold-me").unwrap();
        let handle = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&tmp)
            .unwrap();
        let verdict = check_lock(&tmp);
        assert!(verdict.is_some(), "shared-no-delete handle should register as locked");
        drop(handle);
        let _ = std::fs::remove_file(&tmp);
    }
}
