//! JSON-based clean rule loader.
//!
//! Loads rules from `clean_rules.json` (embedded via `include_str!`),
//! expands environment variables, runs detect checks, and converts
//! active rules into the existing `JunkRule` format for downstream compatibility.

use serde::Deserialize;
use std::path::Path;

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

// ─── JSON Schema Types ───────────────────────────────────────────────────────

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

// ─── Detect Logic ────────────────────────────────────────────────────────────

/// Check if any detect path exists on the filesystem.
fn is_rule_active(detect_paths: &[String]) -> bool {
    detect_paths.iter().any(|raw| {
        expand_env_vars(raw)
            .map(|expanded| Path::new(&expanded).exists())
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

// ─── Public API ──────────────────────────────────────────────────────────────

/// Parse all embedded JSON rule files, detect active rules, and convert them to `JunkRule`.
/// Returns only rules whose detect paths exist on this machine.
pub fn load_json_rules() -> Vec<JunkRule> {
    let mut all_configs: Vec<CleanRuleConfig> = Vec::with_capacity(700);

    for source in ALL_RULE_SOURCES {
        match serde_json::from_str::<Vec<CleanRuleConfig>>(source) {
            Ok(configs) => all_configs.extend(configs),
            Err(e) => {
                eprintln!("[rule_loader] warning: failed to parse a rule file: {}", e);
            }
        }
    }

    let mut rules = Vec::new();

    for cfg in all_configs {
        if !is_rule_active(&cfg.detect) {
            continue;
        }

        let paths: Vec<String> = cfg.targets.iter().flat_map(resolve_target).collect();
        if paths.is_empty() {
            continue;
        }

        // Collect glob patterns from all targets
        let patterns: Vec<&'static str> = cfg
            .targets
            .iter()
            .filter_map(|t| t.patterns.as_ref())
            .flatten()
            .map(|p| -> &'static str { Box::leak(p.clone().into_boxed_str()) })
            .collect();

        // Leak the owned strings to get &'static str (required by JunkRule).
        // This is acceptable because rules are loaded once at startup.
        let id: &'static str = Box::leak(cfg.id.into_boxed_str());
        let name: &'static str = Box::leak(cfg.name.into_boxed_str());
        let description: &'static str = Box::leak(cfg.description.into_boxed_str());

        rules.push(JunkRule {
            id,
            category: cfg.category.to_junk_category(),
            name,
            description,
            paths,
            patterns,
            risk_level: cfg.risk.to_junk_risk(),
            requires_admin: cfg.requires_admin,
            default_selected: cfg.default_selected,
            clean_subdirs_only: cfg.clean_subdirs_only,
        });
    }

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
}
