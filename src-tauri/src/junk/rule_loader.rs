use serde::Deserialize;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use super::{JunkCategory, JunkRiskLevel, JunkRule};

// All rule sources: root sample + categorized files under clean/
static CLEAN_RULES_ROOT: &str = include_str!("../../rules/clean_rules.json");
static CLEAN_RULES_SYSTEM: &str = include_str!("../../rules/clean/system.json");
static CLEAN_RULES_BROWSERS: &str = include_str!("../../rules/clean/browsers.json");
static CLEAN_RULES_COMMUNICATION: &str = include_str!("../../rules/clean/communication.json");
static CLEAN_RULES_DEV_TOOLS: &str = include_str!("../../rules/clean/dev_tools.json");
static CLEAN_RULES_GAMES: &str = include_str!("../../rules/clean/games.json");
static CLEAN_RULES_MULTIMEDIA: &str = include_str!("../../rules/clean/multimedia.json");
static CLEAN_RULES_OFFICE: &str = include_str!("../../rules/clean/office.json");
static CLEAN_RULES_SECURITY: &str = include_str!("../../rules/clean/security.json");
static CLEAN_RULES_DRIVERS_OEM: &str = include_str!("../../rules/clean/drivers_oem.json");

static ALL_RULE_SOURCES: &[&str] = &[
    CLEAN_RULES_ROOT,
    CLEAN_RULES_SYSTEM,
    CLEAN_RULES_BROWSERS,
    CLEAN_RULES_COMMUNICATION,
    CLEAN_RULES_DEV_TOOLS,
    CLEAN_RULES_GAMES,
    CLEAN_RULES_MULTIMEDIA,
    CLEAN_RULES_OFFICE,
    CLEAN_RULES_SECURITY,
    CLEAN_RULES_DRIVERS_OEM,
];
#[derive(Debug, Clone, Deserialize)]
pub struct CleanRuleConfig {
    pub id: String,
    pub app_id: String,
    pub detect: Vec<String>,
    pub category: JsonCategory,
    pub name: String,
    pub description: String,
    pub targets: Vec<TargetConfig>,
    pub risk: JsonRisk,
    pub requires_admin: bool,
    pub default_selected: bool,
    pub clean_subdirs_only: bool,
    /// Optional rule-level recursion cap when patterns are used.
    /// See `JunkRule::max_depth` for semantics.
    #[serde(default)]
    pub max_depth: Option<u32>,
    /// User-facing safety explanation: "why deleting this is OK".
    /// Populated by the rule-audit pass; missing entries surface as
    /// "未补充" placeholder in the UI so authors notice the gap.
    #[serde(default)]
    pub why_safe: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TargetConfig {
    pub base: String,
    pub subdirs: Option<Vec<String>>,
    pub patterns: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonCategory {
    SystemTemp,
    BrowserCache,
    WindowsUpdate,
    ThumbnailCache,
    RecycleBin,
    CrashDump,
    AppLogs,
    FontCache,
    AppCache,
    DevCache,
    InstallerCache,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonRisk {
    Safe,
    Caution,
    Risky,
}

// ─── Environment Variable Expansion ─────────────────────────────────────────

/// Expand `%VAR%` placeholders in a path string using actual environment variables.
fn expand_env_vars(input: &str) -> Option<String> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '%' {
            let mut var_name = String::new();
            let mut found_end = false;
            for c in chars.by_ref() {
                if c == '%' {
                    found_end = true;
                    break;
                }
                var_name.push(c);
            }
            if !found_end || var_name.is_empty() {
                return None;
            }
            let val = std::env::var(&var_name).ok()?;
            let val = val.trim().trim_end_matches(['\\', '/']);
            if val.is_empty() {
                return None;
            }
            result.push_str(val);
        } else {
            result.push(ch);
        }
    }

    Some(result)
}

// ─── Category & Risk Mapping ─────────────────────────────────────────────────

impl JsonCategory {
    fn to_junk_category(self) -> JunkCategory {
        match self {
            JsonCategory::SystemTemp | JsonCategory::InstallerCache => JunkCategory::SystemTemp,
            JsonCategory::BrowserCache | JsonCategory::AppCache | JsonCategory::DevCache => {
                JunkCategory::BrowserCache
            }
            JsonCategory::WindowsUpdate => JunkCategory::WindowsUpdate,
            JsonCategory::ThumbnailCache => JunkCategory::ThumbnailCache,
            JsonCategory::RecycleBin => JunkCategory::RecycleBin,
            JsonCategory::CrashDump => JunkCategory::CrashDump,
            JsonCategory::AppLogs => JunkCategory::AppLogs,
            JsonCategory::FontCache => JunkCategory::FontCache,
        }
    }
}

impl JsonRisk {
    fn to_junk_risk(self) -> JunkRiskLevel {
        match self {
            JsonRisk::Safe => JunkRiskLevel::Safe,
            JsonRisk::Caution => JunkRiskLevel::Caution,
            JsonRisk::Risky => JunkRiskLevel::Risky,
        }
    }
}
struct DetectCounters {
    exists_calls: AtomicUsize,
    exists_hits: AtomicUsize,
    exists_total_ns: AtomicU64,
}

impl DetectCounters {
    const fn new() -> Self {
        Self {
            exists_calls: AtomicUsize::new(0),
            exists_hits: AtomicUsize::new(0),
            exists_total_ns: AtomicU64::new(0),
        }
    }
}

/// Check if any detect path exists on the filesystem.
/// Used only by tests; production scanning calls `rule_is_active` against
/// pre-expanded paths from the prepared cache.
#[cfg(test)]
#[allow(dead_code)]
fn is_rule_active(detect_paths: &[String], counters: &DetectCounters) -> bool {
    detect_paths.iter().any(|raw| {
        expand_env_vars(raw)
            .map(|expanded| {
                let t0 = Instant::now();
                let exists = Path::new(&expanded).exists();
                let elapsed_ns = t0.elapsed().as_nanos() as u64;
                counters.exists_calls.fetch_add(1, Ordering::Relaxed);
                counters.exists_total_ns.fetch_add(elapsed_ns, Ordering::Relaxed);
                if exists {
                    counters.exists_hits.fetch_add(1, Ordering::Relaxed);
                }
                exists
            })
            .unwrap_or(false)
    })
}

// ─── Target → Paths Resolution ──────────────────────────────────────────────

/// Resolve a single TargetConfig into a list of concrete filesystem paths.
fn resolve_target(target: &TargetConfig) -> Vec<String> {
    let base = match expand_env_vars(&target.base) {
        Some(b) => b,
        None => return vec![],
    };

    match &target.subdirs {
        Some(subdirs) if !subdirs.is_empty() => subdirs
            .iter()
            .map(|sub| {
                let sub = sub.trim_start_matches(['\\', '/']);
                format!("{}\\{}", base, sub)
            })
            .collect(),
        _ => vec![base],
    }
}

// ─── Safety Net: Detect Drive-Root Targets ──────────────────────────────────

/// Default depth cap auto-applied when a rule's resolved target is a drive root
/// and patterns are non-empty (which would otherwise trigger a full-drive walk).
const DRIVE_ROOT_DEFAULT_MAX_DEPTH: u32 = 2;

/// Returns `true` if `path` represents a Windows drive root after env-var
/// expansion, e.g. `C:`, `C:\`, `c:/`. We strip trailing separators before
/// checking so both `%SYSTEMDRIVE%` (= `C:`) and `%SYSTEMDRIVE%\\` (= `C:\\`)
/// are detected.
fn is_drive_root(path: &str) -> bool {
    let trimmed = path.trim_end_matches(['\\', '/']);
    // Match exactly `<letter>:`
    matches!(
        trimmed.as_bytes(),
        [b'A'..=b'Z' | b'a'..=b'z', b':']
    )
}

/// Decide the effective `max_depth` for a rule, honoring the explicit
/// configuration but auto-injecting a cap when the rule would otherwise scan
/// an entire drive. Logs a warning when the safety net engages so authors
/// notice the implicit limit.
fn effective_max_depth(
    rule_id: &str,
    paths: &[String],
    patterns_count: usize,
    explicit: Option<u32>,
) -> Option<u32> {
    if explicit.is_some() {
        return explicit;
    }
    // Safety net only fires when patterns are used — pattern-less rules go
    // through cheaper code paths in the scanner.
    if patterns_count == 0 {
        return None;
    }
    if paths.iter().any(|p| is_drive_root(p)) {
        tracing::warn!(
            target: "junk_perf",
            "[rule_loader] safety-net: rule id={} has drive-root base + patterns; auto-applying max_depth={}",
            rule_id,
            DRIVE_ROOT_DEFAULT_MAX_DEPTH,
        );
        return Some(DRIVE_ROOT_DEFAULT_MAX_DEPTH);
    }
    None
}
struct PreparedRule {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    category: JunkCategory,
    risk_level: JunkRiskLevel,
    requires_admin: bool,
    default_selected: bool,
    clean_subdirs_only: bool,
    max_depth: Option<u32>,
    /// Pre-leaked why_safe string; None when the rule's JSON config did not
    /// provide one. Surfaces in the UI as a "未补充" placeholder.
    why_safe: Option<&'static str>,
    /// Pre-expanded detect paths. None entries arose from missing env vars at
    /// preparation time; we keep them to log "this rule's detect path could
    /// not resolve" only once.
    detect_paths: Vec<String>,
    /// Pre-resolved target paths (env-var expanded, subdirs joined).
    target_paths: Vec<String>,
    /// Pattern slices — leaked once, shared by every per-scan `JunkRule` clone.
    patterns: &'static [&'static str],
}

/// Single, lifetime-of-process cache populated by `prepare_all_rules` exactly
/// once. All `Box::leak` calls happen during the initial population; subsequent
/// scans only borrow from this static state.
static PREPARED_RULES: OnceLock<Vec<PreparedRule>> = OnceLock::new();

/// Parse + env-expand + leak all immutable rule data exactly once.
/// Returns a borrowed slice into the process-lifetime cache.
fn prepared_rules() -> &'static [PreparedRule] {
    PREPARED_RULES.get_or_init(prepare_all_rules)
}

fn prepare_all_rules() -> Vec<PreparedRule> {
    let t_total = Instant::now();
    let mut all_configs: Vec<CleanRuleConfig> = Vec::with_capacity(700);

    let t_parse = Instant::now();
    for source in ALL_RULE_SOURCES {
        match serde_json::from_str::<Vec<CleanRuleConfig>>(source) {
            Ok(configs) => all_configs.extend(configs),
            Err(e) => {
                eprintln!("[rule_loader] warning: failed to parse a rule file: {}", e);
            }
        }
    }
    let parse_ms = t_parse.elapsed().as_millis();
    let total_configs = all_configs.len();

    let mut prepared: Vec<PreparedRule> = Vec::with_capacity(all_configs.len());
    let mut empty_paths_count = 0usize;

    for cfg in all_configs {
        // Resolve target paths up front — drops rules whose env vars are
        // missing and lets us evaluate the safety net before leaking strings.
        let target_paths: Vec<String> = cfg.targets.iter().flat_map(resolve_target).collect();
        if target_paths.is_empty() {
            empty_paths_count += 1;
            continue;
        }

        // Pre-expand detect paths for cheaper per-scan probing.
        let detect_paths: Vec<String> = cfg
            .detect
            .iter()
            .filter_map(|raw| expand_env_vars(raw))
            .collect();

        // Collect glob patterns — leaked exactly once per pattern across the
        // entire process lifetime.
        let patterns_owned: Vec<&'static str> = cfg
            .targets
            .iter()
            .filter_map(|t| t.patterns.as_ref())
            .flatten()
            .map(|p| -> &'static str { Box::leak(p.clone().into_boxed_str()) })
            .collect();

        // Compute effective max_depth (and emit any safety-net warning) before
        // moving rule id into the leaked string so logs can reference it.
        let max_depth = effective_max_depth(
            &cfg.id,
            &target_paths,
            patterns_owned.len(),
            cfg.max_depth,
        );

        // One-time leak of immutable strings. These outlive all scans.
        let id: &'static str = Box::leak(cfg.id.into_boxed_str());
        let name: &'static str = Box::leak(cfg.name.into_boxed_str());
        let description: &'static str = Box::leak(cfg.description.into_boxed_str());
        let patterns: &'static [&'static str] =
            Box::leak(patterns_owned.into_boxed_slice());
        let why_safe: Option<&'static str> = cfg
            .why_safe
            .filter(|s| !s.trim().is_empty())
            .map(|s| -> &'static str { Box::leak(s.into_boxed_str()) });

        prepared.push(PreparedRule {
            id,
            name,
            description,
            category: cfg.category.to_junk_category(),
            risk_level: cfg.risk.to_junk_risk(),
            requires_admin: cfg.requires_admin,
            default_selected: cfg.default_selected,
            clean_subdirs_only: cfg.clean_subdirs_only,
            max_depth,
            why_safe,
            detect_paths,
            target_paths,
            patterns,
        });
    }

    tracing::info!(
        target: "junk_perf",
        "[rule_loader] PREPARE one-time: parse={}ms total={}ms configs={} prepared={} empty_paths_dropped={}",
        parse_ms,
        t_total.elapsed().as_millis(),
        total_configs,
        prepared.len(),
        empty_paths_count,
    );

    prepared
}

/// Cheap per-scan rule-active check: just probes existence of pre-expanded
/// detect paths. No allocation, no env-var work.
fn rule_is_active(detect_paths: &[String], counters: &DetectCounters) -> bool {
    detect_paths.iter().any(|expanded| {
        let t0 = Instant::now();
        let exists = Path::new(expanded).exists();
        let elapsed_ns = t0.elapsed().as_nanos() as u64;
        counters.exists_calls.fetch_add(1, Ordering::Relaxed);
        counters.exists_total_ns.fetch_add(elapsed_ns, Ordering::Relaxed);
        if exists {
            counters.exists_hits.fetch_add(1, Ordering::Relaxed);
        }
        exists
    })
}
pub fn load_json_rules() -> Vec<JunkRule> {
    let t_total = Instant::now();
    let prepared = prepared_rules();

    let counters = DetectCounters::new();
    let t_detect_phase = Instant::now();

    let mut rules: Vec<JunkRule> = Vec::with_capacity(prepared.len() / 2);
    let mut active_count = 0usize;

    for p in prepared {
        if !rule_is_active(&p.detect_paths, &counters) {
            continue;
        }
        active_count += 1;

        rules.push(JunkRule {
            id: p.id,
            category: p.category,
            name: p.name,
            description: p.description,
            paths: p.target_paths.clone(),
            patterns: p.patterns.to_vec(),
            risk_level: p.risk_level,
            requires_admin: p.requires_admin,
            default_selected: p.default_selected,
            clean_subdirs_only: p.clean_subdirs_only,
            max_depth: p.max_depth,
            why_safe: p.why_safe,
        });
    }

    let detect_phase_ms = t_detect_phase.elapsed().as_millis();
    let total_ms = t_total.elapsed().as_millis();
    let exists_calls = counters.exists_calls.load(Ordering::Relaxed);
    let exists_hits = counters.exists_hits.load(Ordering::Relaxed);
    let exists_total_ns = counters.exists_total_ns.load(Ordering::Relaxed);
    let avg_exists_us = if exists_calls > 0 {
        (exists_total_ns / exists_calls as u64) / 1_000
    } else {
        0
    };

    tracing::info!(
        target: "junk_perf",
        "[rule_loader] total={}ms detect_phase={}ms prepared={} active={} produced={} | exists_calls={} hits={} avg_exists_us={} sum_exists_ms={}",
        total_ms,
        detect_phase_ms,
        prepared.len(),
        active_count,
        rules.len(),
        exists_calls,
        exists_hits,
        avg_exists_us,
        exists_total_ns / 1_000_000,
    );

    rules
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_json_sources_parse_without_panic() {
        let mut total = 0usize;
        for source in ALL_RULE_SOURCES {
            let configs: Vec<CleanRuleConfig> =
                serde_json::from_str(source).expect("parse failed");
            total += configs.len();
        }
        assert!(total >= 500, "expected >=500 total rules, got {}", total);
    }

    #[test]
    fn expand_env_vars_basic() {
        std::env::set_var("__RULE_LOADER_TEST__", "C:\\TestDir");
        let result = expand_env_vars("%__RULE_LOADER_TEST__%\\Sub").unwrap();
        assert_eq!(result, "C:\\TestDir\\Sub");
        std::env::remove_var("__RULE_LOADER_TEST__");
    }

    #[test]
    fn expand_env_vars_returns_none_for_missing() {
        std::env::remove_var("__RULE_LOADER_MISSING_VAR__");
        assert!(expand_env_vars("%__RULE_LOADER_MISSING_VAR__%\\X").is_none());
    }

    #[test]
    fn json_rule_ids_are_unique() {
        let mut all_configs: Vec<CleanRuleConfig> = Vec::new();
        for source in ALL_RULE_SOURCES {
            let configs: Vec<CleanRuleConfig> =
                serde_json::from_str(source).expect("parse failed");
            all_configs.extend(configs);
        }
        let mut ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for cfg in &all_configs {
            assert!(ids.insert(&cfg.id), "duplicate JSON rule id: {}", cfg.id);
        }
    }

    #[test]
    fn load_json_rules_does_not_panic() {
        let _rules = load_json_rules();
    }

    #[test]
    fn is_drive_root_recognizes_windows_drive_roots() {
        assert!(is_drive_root("C:"));
        assert!(is_drive_root("C:\\"));
        assert!(is_drive_root("C:/"));
        assert!(is_drive_root("c:"));
        assert!(is_drive_root("Z:\\\\"));
        assert!(!is_drive_root("C:\\Windows"));
        assert!(!is_drive_root("C:\\Found.000"));
        assert!(!is_drive_root(""));
        assert!(!is_drive_root("\\\\server\\share"));
    }

    #[test]
    fn effective_max_depth_respects_explicit_value() {
        let paths = vec!["C:\\".to_string()];
        let depth = effective_max_depth("explicit_rule", &paths, 1, Some(3));
        assert_eq!(depth, Some(3));
    }

    #[test]
    fn effective_max_depth_skips_when_no_patterns() {
        let paths = vec!["C:\\".to_string()];
        let depth = effective_max_depth("no_patterns", &paths, 0, None);
        assert_eq!(depth, None);
    }

    #[test]
    fn effective_max_depth_applies_safety_net_for_drive_root() {
        let paths = vec!["C:\\".to_string()];
        let depth = effective_max_depth("drive_root_rule", &paths, 1, None);
        assert_eq!(depth, Some(DRIVE_ROOT_DEFAULT_MAX_DEPTH));
    }

    #[test]
    fn effective_max_depth_skips_safety_net_for_normal_paths() {
        let paths = vec!["C:\\Users\\test\\AppData\\Local".to_string()];
        let depth = effective_max_depth("normal_rule", &paths, 1, None);
        assert_eq!(depth, None);
    }

    /// The prepared-rules cache is populated once per process. Calling
    /// `load_json_rules` repeatedly should not re-parse JSON, re-leak strings,
    /// or invoke `Box::leak` again. This is observable by checking that the
    /// number of prepared rules is stable and that string addresses are
    /// pointer-stable across calls.
    #[test]
    fn load_json_rules_uses_static_cache_on_repeated_calls() {
        let first = load_json_rules();
        let second = load_json_rules();

        assert_eq!(
            first.len(),
            second.len(),
            "rule count must be deterministic across calls",
        );
        if first.is_empty() {
            return; // No rules active on this CI runner; trivially passes.
        }
        // `id` is a `&'static str` — if the cache is bypassed we'd be leaking
        // a fresh allocation per call, so addresses would diverge. The cache
        // hands out the same pointers.
        let addr_first = first[0].id.as_ptr();
        let addr_second = second[0].id.as_ptr();
        assert_eq!(
            addr_first, addr_second,
            "expected stable string pointers from prepared cache, but addresses differ",
        );
    }
    #[test]
    fn json_rules_avoid_implicit_full_drive_scans() {
        let mut all_configs: Vec<CleanRuleConfig> = Vec::new();
        for source in ALL_RULE_SOURCES {
            let configs: Vec<CleanRuleConfig> =
                serde_json::from_str(source).expect("parse failed");
            all_configs.extend(configs);
        }

        let mut offenders: Vec<String> = Vec::new();
        for cfg in &all_configs {
            if cfg.max_depth.is_some() {
                continue;
            }
            let has_patterns = cfg
                .targets
                .iter()
                .any(|t| t.patterns.as_ref().is_some_and(|p| !p.is_empty()));
            if !has_patterns {
                continue;
            }
            for target in &cfg.targets {
                // Use the literal `base` text plus a synthetic env-var
                // expansion so this test is portable: we treat any
                // `%SYSTEMDRIVE%` as `C:` for analysis purposes.
                let synthetic = target.base.replace("%SYSTEMDRIVE%", "C:");
                if is_drive_root(&synthetic) {
                    offenders.push(format!(
                        "rule id={} target.base={} (resolves to drive root, has patterns, no max_depth)",
                        cfg.id, target.base
                    ));
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "Found {} rule(s) that would scan a whole drive without explicit max_depth:\n  {}",
            offenders.len(),
            offenders.join("\n  ")
        );
    }
}
