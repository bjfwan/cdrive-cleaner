use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rayon::prelude::*;
use serde::Serialize;

use crate::junk::rules::{all_rules, JunkCategory, JunkRiskLevel, JunkRule};
use crate::winfs;

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
    pub why_safe: Option<&'static str>,
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

    tracing::info!(
        target: "junk_perf",
        "[scan_blocking] applicable_rules={} (filtered from {} total)",
        scanned_rules,
        scanned_rules + skipped_rules.len(),
    );

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

    let results: Vec<(Vec<JunkItem>, Option<String>, RulePerf)> = applicable
        .par_iter()
        .map(|rule| {
            let t_rule = Instant::now();
            let outcome = process_rule(rule);
            let rule_elapsed_ms = t_rule.elapsed().as_millis() as u64;
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
                        Some(mut guard)
                            if guard.elapsed().as_millis() >= PROGRESS_THROTTLE_MS =>
                        {
                            *guard = Instant::now();
                            true
                        }
                        _ => false,
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

            let perf = RulePerf {
                rule_id: rule.id,
                rule_name: rule.name,
                elapsed_ms: rule_elapsed_ms,
                items: rule_count,
                size: rule_size,
                paths_count: rule.paths.len(),
                patterns_count: rule.patterns.len(),
                clean_subdirs_only: rule.clean_subdirs_only,
            };

            (outcome.0, outcome.1, perf)
        })
        .collect();

    let mut items: Vec<JunkItem> = Vec::new();
    let mut perf_records: Vec<RulePerf> = Vec::with_capacity(results.len());
    for (rule_items, missing, perf) in results {
        items.extend(rule_items);
        if let Some(id) = missing {
            skipped_rules.push(id);
        }
        perf_records.push(perf);
    }

    let items = dedupe_items_by_path(items);
    let total_size: u64 = items.iter().map(|i| i.size).sum();
    let total_count = items.len();

    // Emit a perf summary: top-15 slowest rules and aggregate stats.
    log_rule_perf_summary(&perf_records, start.elapsed().as_millis() as u64);

    Ok(JunkScanResult {
        items,
        total_size,
        total_count,
        scanned_rules,
        skipped_rules,
        scan_duration_ms: start.elapsed().as_millis() as u64,
    })
}

fn dedupe_items_by_path(items: Vec<JunkItem>) -> Vec<JunkItem> {
    let mut deduped: Vec<JunkItem> = Vec::with_capacity(items.len());
    let mut seen: HashMap<String, usize> = HashMap::new();

    for item in items {
        let key = normalize_path_key(&item.path);
        if let Some(&idx) = seen.get(&key) {
            if should_replace_item(&deduped[idx], &item) {
                deduped[idx] = item;
            }
        } else {
            seen.insert(key, deduped.len());
            deduped.push(item);
        }
    }

    deduped
}

fn normalize_path_key(path: &str) -> String {
    let normalized = path.replace('/', "\\");
    normalized.trim_end_matches('\\').to_ascii_lowercase()
}

fn should_replace_item(existing: &JunkItem, candidate: &JunkItem) -> bool {
    let existing_risk = risk_rank(existing.risk_level);
    let candidate_risk = risk_rank(candidate.risk_level);
    if candidate_risk != existing_risk {
        return candidate_risk > existing_risk;
    }

    if candidate.default_selected != existing.default_selected {
        return !candidate.default_selected;
    }

    candidate.rule_id.len() > existing.rule_id.len()
}

fn risk_rank(risk: JunkRiskLevel) -> u8 {
    match risk {
        JunkRiskLevel::Safe => 0,
        JunkRiskLevel::Caution => 1,
        JunkRiskLevel::Risky => 2,
    }
}

/// Per-rule timing record collected during the scan; used only for diagnostics.
struct RulePerf {
    rule_id: &'static str,
    rule_name: &'static str,
    elapsed_ms: u64,
    items: usize,
    size: u64,
    paths_count: usize,
    patterns_count: usize,
    clean_subdirs_only: bool,
}

fn log_rule_perf_summary(records: &[RulePerf], total_scan_ms: u64) {
    if records.is_empty() {
        return;
    }
    let mut sorted: Vec<&RulePerf> = records.iter().collect();
    sorted.sort_by_key(|r| std::cmp::Reverse(r.elapsed_ms));

    let total_rule_ms: u64 = records.iter().map(|r| r.elapsed_ms).sum();
    let max_ms = sorted.first().map(|r| r.elapsed_ms).unwrap_or(0);
    let p50 = sorted
        .get(records.len() / 2)
        .map(|r| r.elapsed_ms)
        .unwrap_or(0);
    let p95 = sorted
        .get(records.len() * 5 / 100)
        .map(|r| r.elapsed_ms)
        .unwrap_or(0);

    tracing::info!(
        target: "junk_perf",
        "[scan_blocking] done total={}ms sum_rule_time={}ms (parallelism_factor={:.2}) max_rule_ms={} p50={} p95={}",
        total_scan_ms,
        total_rule_ms,
        if total_scan_ms > 0 { total_rule_ms as f64 / total_scan_ms as f64 } else { 0.0 },
        max_ms,
        p50,
        p95,
    );

    for (i, r) in sorted.iter().take(15).enumerate() {
        tracing::info!(
            target: "junk_perf",
            "[scan_blocking]  #{:02} {}ms id={} items={} size={} paths={} patterns={} subdirs_only={} name={}",
            i + 1,
            r.elapsed_ms,
            r.rule_id,
            r.items,
            r.size,
            r.paths_count,
            r.patterns_count,
            r.clean_subdirs_only,
            r.rule_name,
        );
    }
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
        // Recursive traversal using winfs::enumerate_directory for pattern matching.
        // Each entry already carries size from the native FindFirstFileEx call,
        // avoiding a separate stat() per file.
        //
        // Depth tracking: pending stores (path, depth_below_root) where depth=0
        // is the rule's base directory. When `rule.max_depth` is set we stop
        // queuing children once the next level would exceed the cap. This is
        // the primary defense against accidental full-drive walks.
        let mut pending: Vec<(PathBuf, u32)> = vec![(path.to_path_buf(), 0)];
        while let Some((dir, depth)) = pending.pop() {
            match winfs::enumerate_directory(&dir, false) {
                Ok(entries) => {
                    let can_descend = match rule.max_depth {
                        Some(cap) => depth < cap,
                        None => true,
                    };
                    for entry in entries {
                        if entry.is_dir && !entry.is_symlink {
                            if can_descend {
                                pending.push((entry.path, depth + 1));
                            }
                        } else if !entry.is_dir {
                            let matched = rule
                                .patterns
                                .iter()
                                .any(|p| simple_glob(&entry.name, p));
                            if matched {
                                items.push(JunkItem {
                                    rule_id: rule.id.to_string(),
                                    category: rule.category,
                                    category_name: category_name.clone(),
                                    rule_name: rule.name.to_string(),
                                    path: entry.path.to_string_lossy().to_string(),
                                    size: entry.size,
                                    file_count: 1,
                                    is_directory: false,
                                    risk_level: rule.risk_level,
                                    default_selected: rule.default_selected,
                                    why_safe: rule.why_safe,
                                });
                            }
                        }
                    }
                }
                Err(err) => {
                    tracing::debug!(
                        "[junk-scan] enumerate_directory path={:?} err={}",
                        dir,
                        err
                    );
                    continue;
                }
            }
        }
        return;
    }

    if rule.clean_subdirs_only {
        // Single-level enumeration: each top-level child becomes a JunkItem.
        // Sub-directories are sized via dir_size_and_count (also winfs-based).
        let entries = match winfs::enumerate_directory(path, false) {
            Ok(e) => e,
            Err(err) => {
                tracing::debug!(
                    "[junk-scan] enumerate_directory path={:?} err={}",
                    path,
                    err
                );
                return;
            }
        };
        for entry in entries {
            let is_dir = entry.is_dir && !entry.is_symlink;
            let (size, count) = if is_dir {
                dir_size_and_count(&entry.path)
            } else {
                (entry.size, 1usize)
            };
            items.push(JunkItem {
                rule_id: rule.id.to_string(),
                category: rule.category,
                category_name: category_name.clone(),
                rule_name: rule.name.to_string(),
                path: entry.path.to_string_lossy().to_string(),
                size,
                file_count: count,
                is_directory: is_dir,
                risk_level: rule.risk_level,
                default_selected: rule.default_selected,
                why_safe: rule.why_safe,
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
        why_safe: rule.why_safe,
    });
}

fn dir_size_and_count(path: &Path) -> (u64, usize) {
    let mut size: u64 = 0;
    let mut count: usize = 0;
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        match winfs::enumerate_directory(&dir, false) {
            Ok(entries) => {
                for entry in entries {
                    if entry.is_dir && !entry.is_symlink {
                        pending.push(entry.path);
                    } else if !entry.is_dir {
                        size = size.saturating_add(entry.size);
                        count += 1;
                    }
                }
            }
            Err(err) => {
                tracing::debug!(
                    "[junk-scan] dir_size_and_count enumerate err dir={:?} err={}",
                    dir,
                    err
                );
                continue;
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
