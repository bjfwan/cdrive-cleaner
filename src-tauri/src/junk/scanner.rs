use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rayon::prelude::*;
use serde::Serialize;

use crate::junk::rules::{all_rules, JunkCategory, JunkRiskLevel, JunkRule};

/// Progress event emitted while scanning junk rules.
///
/// Field names are serialized as snake_case via `serde(rename_all)` defaults
/// (struct field idents are already snake_case) so the front-end TS contract
/// is `{ current_rule, current_rule_name, completed_rules, total_rules,
/// found_items, found_size }`.
#[derive(Debug, Clone, Serialize)]
pub struct JunkScanProgress {
    pub current_rule: String,
    pub current_rule_name: String,
    pub completed_rules: usize,
    pub total_rules: usize,
    pub found_items: usize,
    pub found_size: u64,
}

/// Callback used by `scan_junk_blocking` to report incremental progress.
///
/// Wrapped in `Arc<dyn Fn>` so it can be cheaply shared across rayon worker
/// threads without owning a tauri-specific type inside the scanner module.
pub type JunkScanProgressCallback = Arc<dyn Fn(JunkScanProgress) + Send + Sync + 'static>;

/// Minimum interval between throttled progress emits, in milliseconds.
const PROGRESS_THROTTLE_MS: u128 = 80;

#[derive(Debug, Clone, Serialize)]
pub struct JunkItem {
    pub rule_id: String,
    pub category: JunkCategory,
    pub category_name: String,
    pub rule_name: String,
    pub path: String,
    pub size: u64,
    pub file_count: usize,
    pub is_directory: bool,
    pub risk_level: JunkRiskLevel,
    pub default_selected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct JunkScanResult {
    pub items: Vec<JunkItem>,
    pub total_size: u64,
    pub total_count: usize,
    pub scanned_rules: usize,
    pub skipped_rules: Vec<String>,
    pub scan_duration_ms: u64,
}

pub fn scan_junk_blocking(
    categories: Option<Vec<JunkCategory>>,
    is_admin: bool,
    on_progress: Option<JunkScanProgressCallback>,
) -> anyhow::Result<JunkScanResult> {
    let start = Instant::now();
    let rules = all_rules();

    let category_filter: Option<HashSet<JunkCategory>> =
        categories.map(|v| v.into_iter().collect());

    let mut applicable: Vec<JunkRule> = Vec::new();
    let mut skipped_rules: Vec<String> = Vec::new();

    for rule in rules {
        if let Some(set) = category_filter.as_ref() {
            if !set.contains(&rule.category) {
                continue;
            }
        }
        if rule.requires_admin && !is_admin {
            skipped_rules.push(rule.id.to_string());
            continue;
        }
        applicable.push(rule);
    }

    let scanned_rules = applicable.len();

    // Shared atomic counters keep cross-thread accumulation lock-free.
    let completed = Arc::new(AtomicUsize::new(0));
    let found_items = Arc::new(AtomicUsize::new(0));
    let found_size = Arc::new(AtomicU64::new(0));
    // `last_emit` is the throttle gate; `Mutex` is fine because contention is
    // bounded by the number of rules and emit cadence.
    let last_emit = Arc::new(Mutex::new(Instant::now()));

    // Initial 0/N emit so the front-end immediately knows total_rules.
    if let Some(cb) = on_progress.as_ref() {
        cb(JunkScanProgress {
            current_rule: String::new(),
            current_rule_name: String::new(),
            completed_rules: 0,
            total_rules: scanned_rules,
            found_items: 0,
            found_size: 0,
        });
        // Reset throttle window so the first per-rule emit isn't gated on the
        // initial event's timestamp.
        if let Ok(mut guard) = last_emit.lock() {
            *guard = Instant::now();
        }
    }

    let results: Vec<(Vec<JunkItem>, Option<String>)> = applicable
        .par_iter()
        .map(|rule| {
            let outcome = process_rule(rule);
            let (rule_items, _) = &outcome;

            // Per-rule accumulation: do this regardless of whether a callback
            // is registered; the cost is two atomic adds per rule.
            let rule_size: u64 = rule_items.iter().map(|i| i.size).sum();
            let rule_count = rule_items.len();
            let total_size_now = found_size.fetch_add(rule_size, Ordering::Relaxed) + rule_size;
            let total_items_now =
                found_items.fetch_add(rule_count, Ordering::Relaxed) + rule_count;
            let done_now = completed.fetch_add(1, Ordering::Relaxed) + 1;

            if let Some(cb) = on_progress.as_ref() {
                let is_last = done_now >= scanned_rules;
                let should_emit = if is_last {
                    true
                } else {
                    // Throttle: only emit when at least PROGRESS_THROTTLE_MS
                    // has passed since the last emit. `lock().ok()` avoids
                    // panicking if a worker poisoned the mutex.
                    match last_emit.lock().ok() {
                        Some(mut guard) => {
                            if guard.elapsed().as_millis() >= PROGRESS_THROTTLE_MS {
                                *guard = Instant::now();
                                true
                            } else {
                                false
                            }
                        }
                        None => false,
                    }
                };
                if should_emit {
                    cb(JunkScanProgress {
                        current_rule: rule.id.to_string(),
                        current_rule_name: rule.name.to_string(),
                        completed_rules: done_now,
                        total_rules: scanned_rules,
                        found_items: total_items_now,
                        found_size: total_size_now,
                    });
                }
            }

            outcome
        })
        .collect();

    let mut items: Vec<JunkItem> = Vec::new();
    for (rule_items, missing) in results {
        items.extend(rule_items);
        if let Some(id) = missing {
            skipped_rules.push(id);
        }
    }

    let total_size: u64 = items.iter().map(|i| i.size).sum();
    let total_count = items.len();

    Ok(JunkScanResult {
        items,
        total_size,
        total_count,
        scanned_rules,
        skipped_rules,
        scan_duration_ms: start.elapsed().as_millis() as u64,
    })
}

fn process_rule(rule: &JunkRule) -> (Vec<JunkItem>, Option<String>) {
    let mut items: Vec<JunkItem> = Vec::new();
    let mut found_any = false;

    for raw_path in &rule.paths {
        let resolved = resolve_path_glob(raw_path);
        for path in resolved {
            if !path.exists() {
                continue;
            }
            found_any = true;
            scan_path_for_rule(&path, rule, &mut items);
        }
    }

    let missing = if found_any {
        None
    } else {
        Some(rule.id.to_string())
    };
    (items, missing)
}

fn scan_path_for_rule(path: &Path, rule: &JunkRule, items: &mut Vec<JunkItem>) {
    let category_name = category_label(rule.category).to_string();

    if !rule.patterns.is_empty() {
        for entry in walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| match e {
                Ok(e) => Some(e),
                Err(err) => {
                    tracing::debug!(
                        "[junk-scan] walkdir entry error path={:?} err={}",
                        path,
                        err
                    );
                    None
                }
            })
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            let matched = rule
                .patterns
                .iter()
                .any(|p| simple_glob(&name, p));
            if !matched {
                continue;
            }
            let size = match entry.metadata() {
                Ok(m) => m.len(),
                Err(err) => {
                    tracing::debug!(
                        "[junk-scan] metadata path={:?} err={}",
                        entry.path(),
                        err
                    );
                    0
                }
            };
            items.push(JunkItem {
                rule_id: rule.id.to_string(),
                category: rule.category,
                category_name: category_name.clone(),
                rule_name: rule.name.to_string(),
                path: entry.path().to_string_lossy().to_string(),
                size,
                file_count: 1,
                is_directory: false,
                risk_level: rule.risk_level,
                default_selected: rule.default_selected,
            });
        }
        return;
    }

    if rule.clean_subdirs_only {
        let read = match std::fs::read_dir(path) {
            Ok(r) => r,
            Err(err) => {
                tracing::debug!(
                    "[junk-scan] read_dir path={:?} err={}",
                    path,
                    err
                );
                return;
            }
        };
        for entry in read.filter_map(|e| match e {
            Ok(e) => Some(e),
            Err(err) => {
                tracing::debug!(
                    "[junk-scan] read_dir entry path={:?} err={}",
                    path,
                    err
                );
                None
            }
        }) {
            let entry_path = entry.path();
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(err) => {
                    tracing::debug!(
                        "[junk-scan] metadata path={:?} err={}",
                        entry_path,
                        err
                    );
                    continue;
                }
            };
            let is_dir = meta.is_dir();
            let (size, count) = if is_dir {
                dir_size_and_count(&entry_path)
            } else {
                (meta.len(), 1usize)
            };
            items.push(JunkItem {
                rule_id: rule.id.to_string(),
                category: rule.category,
                category_name: category_name.clone(),
                rule_name: rule.name.to_string(),
                path: entry_path.to_string_lossy().to_string(),
                size,
                file_count: count,
                is_directory: is_dir,
                risk_level: rule.risk_level,
                default_selected: rule.default_selected,
            });
        }
        return;
    }

    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(err) => {
            tracing::debug!(
                "[junk-scan] metadata path={:?} err={}",
                path,
                err
            );
            return;
        }
    };
    let is_dir = meta.is_dir();
    let (size, count) = if is_dir {
        dir_size_and_count(path)
    } else {
        (meta.len(), 1usize)
    };
    items.push(JunkItem {
        rule_id: rule.id.to_string(),
        category: rule.category,
        category_name,
        rule_name: rule.name.to_string(),
        path: path.to_string_lossy().to_string(),
        size,
        file_count: count,
        is_directory: is_dir,
        risk_level: rule.risk_level,
        default_selected: rule.default_selected,
    });
}

fn dir_size_and_count(path: &Path) -> (u64, usize) {
    let mut size: u64 = 0;
    let mut count: usize = 0;
    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| match e {
            Ok(e) => Some(e),
            Err(err) => {
                tracing::debug!(
                    "[junk-scan] dir size walkdir path={:?} err={}",
                    path,
                    err
                );
                None
            }
        })
    {
        if !entry.file_type().is_file() {
            continue;
        }
        match entry.metadata() {
            Ok(m) => {
                size = size.saturating_add(m.len());
                count += 1;
            }
            Err(err) => {
                tracing::debug!(
                    "[junk-scan] dir size metadata path={:?} err={}",
                    entry.path(),
                    err
                );
            }
        }
    }
    (size, count)
}

fn resolve_path_glob(raw: &str) -> Vec<PathBuf> {
    let normalized = raw.replace('/', "\\");

    let star_pos = match normalized.find('*') {
        Some(p) => p,
        None => return vec![PathBuf::from(normalized)],
    };

    let prefix_end = normalized[..star_pos].rfind('\\').unwrap_or(0);
    let suffix_start = match normalized[star_pos..].find('\\') {
        Some(p) => star_pos + p,
        None => normalized.len(),
    };

    let prefix_str = &normalized[..prefix_end];
    let prefix_path = if prefix_str.ends_with(':') {
        PathBuf::from(format!("{}\\", prefix_str))
    } else if prefix_str.is_empty() {
        return Vec::new();
    } else {
        PathBuf::from(prefix_str)
    };

    let glob_start = if normalized.as_bytes().get(prefix_end) == Some(&b'\\') {
        prefix_end + 1
    } else {
        prefix_end
    };
    let glob_segment = &normalized[glob_start..suffix_start];
    let suffix = &normalized[suffix_start..];

    let read = match std::fs::read_dir(&prefix_path) {
        Ok(r) => r,
        Err(err) => {
            tracing::debug!(
                "[junk-scan] glob read_dir prefix={:?} err={}",
                prefix_path,
                err
            );
            return Vec::new();
        }
    };

    let mut results: Vec<PathBuf> = Vec::new();
    for entry in read.filter_map(|e| match e {
        Ok(e) => Some(e),
        Err(err) => {
            tracing::debug!(
                "[junk-scan] glob entry prefix={:?} err={}",
                prefix_path,
                err
            );
            None
        }
    }) {
        let name = match entry.file_name().into_string() {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !simple_glob(&name, glob_segment) {
            continue;
        }
        let combined = format!("{}\\{}{}", prefix_str, name, suffix);
        if combined.contains('*') {
            results.extend(resolve_path_glob(&combined));
        } else {
            results.push(PathBuf::from(combined));
        }
    }
    results
}

pub fn simple_glob(name: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        return name == pattern;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    let mut cursor = 0usize;

    for (idx, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if idx == 0 {
            if !name.starts_with(part) {
                return false;
            }
            cursor = part.len();
        } else if idx == parts.len() - 1 {
            if name.len() < cursor + part.len() {
                return false;
            }
            if !name[cursor..].ends_with(part) {
                return false;
            }
        } else {
            match name[cursor..].find(part) {
                Some(pos) => cursor += pos + part.len(),
                None => return false,
            }
        }
    }
    true
}

fn category_label(category: JunkCategory) -> &'static str {
    match category {
        JunkCategory::SystemTemp => "系统临时文件",
        JunkCategory::BrowserCache => "浏览器缓存",
        JunkCategory::WindowsUpdate => "Windows 更新残留",
        JunkCategory::ThumbnailCache => "缩略图缓存",
        JunkCategory::RecycleBin => "回收站",
        JunkCategory::CrashDump => "崩溃转储",
        JunkCategory::AppLogs => "应用日志",
        JunkCategory::FontCache => "字体缓存",
    }
}

#[cfg(test)]
mod tests {
    use super::simple_glob;

    #[test]
    fn glob_exact_match() {
        assert!(simple_glob("foo.txt", "foo.txt"));
        assert!(!simple_glob("foo.txt", "bar.txt"));
    }

    #[test]
    fn glob_prefix_suffix() {
        assert!(simple_glob("thumbcache_32.db", "thumbcache_*.db"));
        assert!(simple_glob("a.log", "*.log"));
        assert!(simple_glob("setup.exe", "setup.*"));
        assert!(!simple_glob("a.txt", "*.log"));
    }

    #[test]
    fn glob_star_only() {
        assert!(simple_glob("anything", "*"));
        assert!(simple_glob("", "*"));
    }

    #[test]
    fn glob_middle() {
        assert!(simple_glob("foo_v1_bar.tmp", "foo_*_bar.tmp"));
        assert!(!simple_glob("foo_bar.tmp", "foo_*_baz.tmp"));
    }
}
