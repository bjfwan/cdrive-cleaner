use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use super::scan_index::IndexedScanResult;
use crate::commands::DiskInfo;

use aho_corasick::{AhoCorasick, MatchKind};

#[derive(Debug, Clone, Serialize)]
pub struct SpaceBreakdown {
    pub disk_path: String,
    pub disk_total: u64,
    pub disk_used: u64,
    pub disk_free: u64,
    pub system_reserved_bytes: u64,
    pub categories: Vec<BreakdownCategory>,
    pub actionable_total: u64,
    pub non_actionable_total: u64,
    pub analysis_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BreakdownCategory {
    pub id: String,
    pub label: String,
    pub description: String,
    pub size: u64,
    pub percentage: f64,
    pub item_count: usize,
    pub actionable: String,
    pub color: String,
    pub top_items: Vec<BreakdownItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BreakdownItem {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub item_type: String,
    pub can_migrate: bool,
    pub can_delete: bool,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileExplanation {
    pub path: String,
    pub explanation: String,
    pub app_name: Option<String>,
    pub safe_to_delete: bool,
    pub will_regenerate: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelocatableProgram {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub file_count: usize,
    pub verdict: String,
    pub verdict_reason: String,
    pub can_migrate: bool,
    pub app_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalanceSuggestion {
    pub source_disk: String,
    pub target_disk: String,
    pub current_source_used: u64,
    pub current_target_used: u64,
    pub projected_source_used: u64,
    pub projected_target_used: u64,
    pub total_movable: u64,
    pub suggested_items: Vec<BalanceItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalanceItem {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub category: String,
    pub action: String,
    pub priority: u8,
}


#[derive(Deserialize)]
struct ExplainRule {
    pattern: String,
    #[serde(rename = "mode")]
    match_mode: MatchMode,
    explanation: String,
    app_name: Option<String>,
    safe_to_delete: bool,
    will_regenerate: bool,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MatchMode {
    Contains,
    EndsWith,
    StartsWith,
}

static RULES_JSON: &str = include_str!("../../rules/explain_rules.json");

/// Parsed explain rules, partitioned by match mode for the AC engine.
struct ExplainEngine {
    /// All rules in original order (for starts_with / ends_with fallback).
    all_rules: Vec<ExplainRule>,
    /// Aho-Corasick automaton built from `contains` patterns only.
    ac: AhoCorasick,
    /// Index mapping: ac pattern index → position in `all_rules`.
    ac_to_rule: Vec<usize>,
    /// Indices of `starts_with` rules in `all_rules`.
    starts_with_indices: Vec<usize>,
    /// Indices of `ends_with` rules in `all_rules`.
    ends_with_indices: Vec<usize>,
}

fn explain_engine() -> &'static ExplainEngine {
    static ENGINE: OnceLock<ExplainEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let all_rules: Vec<ExplainRule> =
            serde_json::from_str(RULES_JSON).expect("invalid explain_rules.json");

        let mut ac_patterns: Vec<&str> = Vec::new();
        let mut ac_to_rule: Vec<usize> = Vec::new();
        let mut starts_with_indices: Vec<usize> = Vec::new();
        let mut ends_with_indices: Vec<usize> = Vec::new();

        for (i, rule) in all_rules.iter().enumerate() {
            match rule.match_mode {
                MatchMode::Contains => {
                    ac_patterns.push(&rule.pattern);
                    ac_to_rule.push(i);
                }
                MatchMode::StartsWith => starts_with_indices.push(i),
                MatchMode::EndsWith => ends_with_indices.push(i),
            }
        }

        let ac = AhoCorasick::builder()
            .match_kind(MatchKind::Standard)
            .build(&ac_patterns)
            .expect("failed to build AhoCorasick automaton");

        ExplainEngine {
            all_rules,
            ac,
            ac_to_rule,
            starts_with_indices,
            ends_with_indices,
        }
    })
}

fn drive_relative_path(trimmed: &str) -> Option<&str> {
    let bytes = trimmed.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' {
        return None;
    }
    if bytes.len() == 2 {
        return Some("");
    }
    if bytes.get(2) != Some(&b'\\') {
        return None;
    }
    Some(&trimmed[3..])
}

fn fixed_explanation(
    path: &str,
    explanation: &str,
    app_name: Option<&str>,
    safe_to_delete: bool,
    will_regenerate: bool,
) -> FileExplanation {
    FileExplanation {
        path: path.to_string(),
        explanation: explanation.to_string(),
        app_name: app_name.map(str::to_string),
        safe_to_delete,
        will_regenerate,
    }
}

fn known_windows_path_explanation(path: &str, trimmed: &str) -> Option<FileExplanation> {
    let rel = drive_relative_path(trimmed)?;
    match rel {
        "windows" => Some(fixed_explanation(
            path,
            "Windows 系统目录，包含系统组件、驱动和服务文件，不建议手动删除或搬迁",
            Some("Windows"),
            false,
            false,
        )),
        "pagefile.sys" => Some(fixed_explanation(
            path,
            "Windows 虚拟内存分页文件，由系统管理，不应手动删除",
            Some("Windows"),
            false,
            false,
        )),
        "hiberfil.sys" => Some(fixed_explanation(
            path,
            "Windows 休眠文件，可通过关闭休眠释放空间，不应直接删除",
            Some("Windows"),
            false,
            false,
        )),
        "swapfile.sys" => Some(fixed_explanation(
            path,
            "Windows 应用交换文件，由系统管理，不应手动删除",
            Some("Windows"),
            false,
            false,
        )),
        "$recycle.bin" => Some(fixed_explanation(
            path,
            "Windows 回收站数据，可通过清空回收站释放空间",
            Some("Windows"),
            false,
            false,
        )),
        "system volume information" => Some(fixed_explanation(
            path,
            "系统还原点和卷影副本数据，由 Windows 保护和管理",
            Some("Windows"),
            false,
            false,
        )),
        "recovery" => Some(fixed_explanation(
            path,
            "Windows 恢复环境文件，用于系统修复和重置",
            Some("Windows"),
            false,
            false,
        )),
        "boot" | "efi" => Some(fixed_explanation(
            path,
            "系统启动文件目录，删除或迁移可能导致无法启动",
            Some("Windows"),
            false,
            false,
        )),
        "$windows.~bt" | "$windows.~ws" | "$winreagent" | "$sysreset" | "windows.old" => {
            Some(fixed_explanation(
                path,
                "Windows 升级或重置残留目录，建议通过系统清理功能处理",
                Some("Windows"),
                false,
                false,
            ))
        }
        "perflogs" => Some(fixed_explanation(
            path,
            "Windows 性能监视器日志目录，通常体积很小",
            Some("Windows"),
            false,
            false,
        )),
        _ => None,
    }
}


/// 给任意路径生成一句话解释。
pub fn explain_path(path: &str) -> FileExplanation {
    let path_lower = path.to_ascii_lowercase().replace('/', "\\");
    let trimmed = path_lower.trim_end_matches('\\');
    if let Some(explanation) = known_windows_path_explanation(path, trimmed) {
        return explanation;
    }
    if trimmed == "c:\\program files" {
        return FileExplanation {
            path: path.to_string(),
            explanation: "Program Files 是 64 位应用安装目录集合根，请选择具体应用目录而不是整体搬迁".to_string(),
            app_name: Some("Windows".to_string()),
            safe_to_delete: false,
            will_regenerate: false,
        };
    }
    if trimmed == "c:\\program files (x86)" {
        return FileExplanation {
            path: path.to_string(),
            explanation: "Program Files (x86) 是 32 位应用安装目录集合根，请选择具体应用目录而不是整体搬迁".to_string(),
            app_name: Some("Windows".to_string()),
            safe_to_delete: false,
            will_regenerate: false,
        };
    }
    let engine = explain_engine();

    // 1) AC automaton: O(path_length) for all `contains` rules
    //    We find ALL matches and pick the one with the lowest original rule index
    //    to preserve "first rule wins" priority semantics from the linear scan.
    let mut best_ac: Option<usize> = None;
    for mat in engine.ac.find_overlapping_iter(&path_lower) {
        let rule_idx = engine.ac_to_rule[mat.pattern().as_usize()];
        match best_ac {
            None => best_ac = Some(rule_idx),
            Some(current) if rule_idx < current => best_ac = Some(rule_idx),
            _ => {}
        }
    }
    if let Some(rule_idx) = best_ac {
        let rule = &engine.all_rules[rule_idx];
        return FileExplanation {
            path: path.to_string(),
            explanation: rule.explanation.clone(),
            app_name: rule.app_name.clone(),
            safe_to_delete: rule.safe_to_delete,
            will_regenerate: rule.will_regenerate,
        };
    }

    // 2) Fallback: startsWith linear scan (typically few rules)
    for &idx in &engine.starts_with_indices {
        let rule = &engine.all_rules[idx];
        if path_lower.starts_with(&rule.pattern) {
            return FileExplanation {
                path: path.to_string(),
                explanation: rule.explanation.clone(),
                app_name: rule.app_name.clone(),
                safe_to_delete: rule.safe_to_delete,
                will_regenerate: rule.will_regenerate,
            };
        }
    }

    // 3) Fallback: endsWith linear scan (typically few rules)
    for &idx in &engine.ends_with_indices {
        let rule = &engine.all_rules[idx];
        if path_lower.ends_with(&rule.pattern) {
            return FileExplanation {
                path: path.to_string(),
                explanation: rule.explanation.clone(),
                app_name: rule.app_name.clone(),
                safe_to_delete: rule.safe_to_delete,
                will_regenerate: rule.will_regenerate,
            };
        }
    }

    FileExplanation {
        path: path.to_string(),
        explanation: "未识别的文件或目录".to_string(),
        app_name: None,
        safe_to_delete: false,
        will_regenerate: false,
    }
}

struct CategoryMeta {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    actionable: &'static str,
    color: &'static str,
}

const CATEGORY_META: &[CategoryMeta] = &[
    CategoryMeta { id: "system", label: "系统文件", description: "Windows 系统核心文件", actionable: "none", color: "#6b7280" },
    CategoryMeta { id: "programs", label: "安装程序", description: "Program Files 下的应用程序", actionable: "partial", color: "#3b82f6" },
    CategoryMeta { id: "user_data", label: "用户数据", description: "文档、桌面、下载等个人文件", actionable: "none", color: "#10b981" },
    CategoryMeta { id: "app_data", label: "应用数据", description: "应用配置和运行数据", actionable: "partial", color: "#8b5cf6" },
    CategoryMeta { id: "app_cache", label: "应用缓存", description: "浏览器缓存、IDE 缓存等", actionable: "full", color: "#f59e0b" },
    CategoryMeta { id: "dev_tools", label: "开发工具", description: "node_modules、包管理缓存等", actionable: "full", color: "#d97706" },
    CategoryMeta { id: "temp", label: "临时文件", description: "系统/用户临时文件、日志、崩溃转储", actionable: "full", color: "#ef4444" },
    CategoryMeta { id: "games", label: "游戏", description: "Steam / Epic / Game Pass 游戏库", actionable: "full", color: "#ec4899" },
    CategoryMeta { id: "system_reserved", label: "系统保留/卷元数据", description: "NTFS 元数据、MFT/USN、卷影副本和分配簇差额", actionable: "none", color: "#64748b" },
    CategoryMeta { id: "other", label: "其他", description: "未分类的文件和目录", actionable: "unknown", color: "#9ca3af" },
];

fn get_user_profile_lower() -> &'static str {
    static PROFILE: OnceLock<String> = OnceLock::new();
    PROFILE.get_or_init(|| {
        std::env::var("USERPROFILE")
            .unwrap_or_else(|_| "C:\\Users\\Default".to_string())
            .to_ascii_lowercase()
            .replace('/', "\\")
    })
}

fn classify_path(path_lower: &str) -> &'static str {
    if path_lower == "c:\\windows"
        || path_lower.starts_with("c:\\windows\\")
        || path_lower == "c:\\$recycle.bin"
        || path_lower.starts_with("c:\\$recycle.bin\\")
        || path_lower == "c:\\system volume information"
        || path_lower.starts_with("c:\\system volume information\\")
        || path_lower == "c:\\recovery"
        || path_lower.starts_with("c:\\recovery\\")
        || path_lower == "c:\\boot"
        || path_lower.starts_with("c:\\boot\\")
        || path_lower == "c:\\efi"
        || path_lower.starts_with("c:\\efi\\")
        || path_lower == "c:\\$windows.~bt"
        || path_lower.starts_with("c:\\$windows.~bt\\")
        || path_lower == "c:\\$windows.~ws"
        || path_lower.starts_with("c:\\$windows.~ws\\")
        || path_lower == "c:\\$winreagent"
        || path_lower.starts_with("c:\\$winreagent\\")
        || path_lower == "c:\\$sysreset"
        || path_lower.starts_with("c:\\$sysreset\\")
        || path_lower == "c:\\windows.old"
        || path_lower.starts_with("c:\\windows.old\\")
        || path_lower == "c:\\msocache"
        || path_lower.starts_with("c:\\msocache\\")
    {
        return "system";
    }

    if path_lower.starts_with("c:\\programdata\\microsoft\\windows\\")
    {
        return "system";
    }

    if path_lower.ends_with("\\pagefile.sys")
        || path_lower.ends_with("\\hiberfil.sys")
        || path_lower.ends_with("\\swapfile.sys")
    {
        return "system";
    }

    if path_lower.starts_with("c:\\program files\\") || path_lower.starts_with("c:\\program files (x86)\\")
        || path_lower == "c:\\program files" || path_lower == "c:\\program files (x86)"
    {
        if path_lower.contains("\\windowsapps") {
            return "system";
        }
        if path_lower.contains("\\steam\\steamapps\\")
            || path_lower.contains("\\epic games\\")
            || path_lower.contains("\\xbox games\\")
        {
            return "games";
        }
        return "programs";
    }

    if path_lower.starts_with("c:\\programdata\\package cache")
        || path_lower == "c:\\programdata\\package cache"
    {
        return "programs";
    }

    let user_profile = get_user_profile_lower();

    let user_data_dirs = [
        "\\documents", "\\desktop", "\\downloads", "\\pictures",
        "\\videos", "\\music", "\\onedrive", "\\contacts",
        "\\favorites", "\\links", "\\saved games", "\\searches",
    ];
    for dir in &user_data_dirs {
        let prefix = format!("{}{}", user_profile, dir);
        if path_lower == prefix || path_lower.starts_with(&format!("{}\\", prefix)) {
            return "user_data";
        }
    }

    if path_lower.contains("\\steam\\steamapps\\")
        || path_lower.contains("\\epic games\\")
        || path_lower.contains("\\xboxgames\\")
        || path_lower.contains("\\xbox games\\")
        || path_lower.contains("\\gog games\\")
        || path_lower.contains("\\riot games\\")
        || path_lower.contains("\\ubisoft\\ubisoft game launcher\\games\\")
        || path_lower.contains("\\origin games\\")
        || path_lower.contains("\\ea games\\")
        || path_lower.contains("\\battle.net\\games\\")
    {
        return "games";
    }

    let temp_indicators = [
        "\\local\\temp\\", "\\local\\temp",
        "\\windows\\temp\\", "\\windows\\temp",
        "\\crashdumps\\", "\\crashdumps",
        "\\crash\\", "\\logs\\", "\\log\\",
    ];
    for indicator in &temp_indicators {
        if path_lower.contains(indicator) || path_lower.ends_with(indicator.trim_end_matches('\\')) {
            return "temp";
        }
    }

    // Root-level driver/OEM installation residue -> temp
    if path_lower == "c:\\intel" || path_lower.starts_with("c:\\intel\\")
        || path_lower == "c:\\amd" || path_lower.starts_with("c:\\amd\\")
        || path_lower == "c:\\nvidia" || path_lower.starts_with("c:\\nvidia\\")
        || path_lower == "c:\\dell" || path_lower.starts_with("c:\\dell\\")
        || path_lower == "c:\\hp" || path_lower.starts_with("c:\\hp\\")
        || path_lower == "c:\\swsetup" || path_lower.starts_with("c:\\swsetup\\")
        || path_lower == "c:\\drivers" || path_lower.starts_with("c:\\drivers\\")
        || path_lower == "c:\\perflogs" || path_lower.starts_with("c:\\perflogs\\")
    {
        return "temp";
    }

    let cache_indicators = [
        "\\cache\\", "\\cache2\\", "\\gpucache\\", "\\code cache\\",
        "\\cacheddata\\", "\\cachedextensionvsixs\\", "\\shader cache\\",
        "\\nv_cache\\", "\\dxcache\\", "\\deliveryoptimization\\",
        "\\inetcache\\", "\\webcache\\", "\\browsercache\\",
        "\\httpcache\\", "\\shadercache\\",
    ];
    for indicator in &cache_indicators {
        if path_lower.contains(indicator) {
            return "app_cache";
        }
    }
    if path_lower.ends_with("\\cache") || path_lower.ends_with("\\gpucache")
        || path_lower.ends_with("\\code cache") || path_lower.ends_with("\\deliveryoptimization")
        || path_lower.ends_with("\\inetcache") || path_lower.ends_with("\\webcache")
    {
        return "app_cache";
    }

    let dev_indicators = [
        "\\node_modules\\", "\\node_modules",
        "\\.pnpm-store\\", "\\.pnpm-store",
        "\\.npm\\", "\\.npm",
        "\\.yarn\\", "\\.yarn",
        "\\.cargo\\registry\\", "\\.cargo\\registry",
        "\\target\\debug\\", "\\target\\release\\",
        "\\.gradle\\caches\\", "\\.gradle\\caches",
        "\\.m2\\repository\\", "\\.m2\\repository",
        "__pycache__",
        "\\.rustup\\",
        "\\.nuget\\",
        "\\.conda\\",
        "\\.virtualenvs\\",
        "\\.bun\\", "\\.deno\\",
        "\\go\\pkg\\",
        "\\.jdks\\", "\\.sdkman\\",
        "\\.docker\\", "\\.kube\\",
        "\\android\\sdk\\",
        "\\.pyenv\\", "\\.nvm\\", "\\.fnm\\",
    ];
    for indicator in &dev_indicators {
        if path_lower.contains(indicator) || path_lower.ends_with(indicator.trim_end_matches('\\')) {
            return "dev_tools";
        }
    }

    let appdata_lower = format!("{}\\appdata\\", user_profile);
    let local_appdata = format!("{}\\appdata\\local\\", user_profile);
    let roaming_appdata = format!("{}\\appdata\\roaming\\", user_profile);
    if path_lower.starts_with(&local_appdata) || path_lower.starts_with(&roaming_appdata) || path_lower.starts_with(&appdata_lower) {
        return "app_data";
    }

    let dotdirs = ["\\.vscode\\", "\\.vscode", "\\.kiro\\", "\\.kiro", "\\.trae\\", "\\.trae"];
    for d in &dotdirs {
        let check = format!("{}{}", user_profile, d);
        if path_lower.starts_with(&check) || path_lower == check.trim_end_matches('\\') {
            return "app_data";
        }
    }

    if path_lower.starts_with("c:\\programdata\\") || path_lower == "c:\\programdata" {
        return "app_data";
    }

    if path_lower.starts_with("c:\\users\\") {
        return "app_data";
    }

    "other"
}

fn add_to_buckets(
    path: &str,
    name: &str,
    size: u64,
    is_dir: bool,
    bucket_sizes: &mut std::collections::HashMap<&'static str, u64>,
    buckets: &mut std::collections::HashMap<&'static str, Vec<(String, String, u64, bool)>>,
) {
    let path_lower = path.to_ascii_lowercase().replace('/', "\\");
    let category = classify_path(&path_lower);
    *bucket_sizes.entry(category).or_insert(0) += size;
    let items = buckets.entry(category).or_default();
    if items.len() < 10 || size > items.last().map(|i| i.2).unwrap_or(0) {
        items.push((path.to_string(), name.to_string(), size, is_dir));
        items.sort_by_key(|i| std::cmp::Reverse(i.2));
        if items.len() > 10 {
            items.truncate(10);
        }
    }
}

/// 把整棵目录树按"归属"分成 7~9 个大桶。
pub fn analyze_space_breakdown(indexed: &IndexedScanResult, disk_total: u64, disk_used: u64) -> SpaceBreakdown {
    let started = std::time::Instant::now();
    let disk_path = indexed.root_path().to_string();

    let mut buckets: std::collections::HashMap<&'static str, Vec<(String, String, u64, bool)>> =
        std::collections::HashMap::new();
    let mut bucket_sizes: std::collections::HashMap<&'static str, u64> =
        std::collections::HashMap::new();

    for child in indexed.root_children() {
        if child.name == "根目录文件" {
            for file in indexed.large_files_iter() {
                let file_path = std::path::Path::new(&file.path);
                if let Some(parent) = file_path.parent() {
                    if parent.to_string_lossy().to_ascii_lowercase().replace('/', "\\").trim_end_matches('\\') == disk_path.to_ascii_lowercase().trim_end_matches('\\') {
                        add_to_buckets(&file.path, &file.name, file.size, false, &mut bucket_sizes, &mut buckets);
                    }
                }
            }
            let accounted: u64 = indexed.large_files_iter()
                .filter(|f| {
                    if let Some(parent) = std::path::Path::new(&f.path).parent() {
                        parent.to_string_lossy().to_ascii_lowercase().replace('/', "\\").trim_end_matches('\\') == disk_path.to_ascii_lowercase().trim_end_matches('\\')
                    } else {
                        false
                    }
                })
                .map(|f| f.size)
                .sum();
            let remaining = child.size.saturating_sub(accounted);
            if remaining > 0 {
                add_to_buckets(&child.path, "根目录其他文件", remaining, false, &mut bucket_sizes, &mut buckets);
            }
            continue;
        }

        let name_lower = child.name.to_ascii_lowercase();
        if name_lower == "users" {
            if let Some(user_dirs) = indexed.children_of(&child.path) {
                for user_dir in user_dirs {
                    if let Some(sub_dirs) = indexed.children_of(&user_dir.path) {
                        for sub in sub_dirs {
                            add_to_buckets(&sub.path, &sub.name, sub.size, true, &mut bucket_sizes, &mut buckets);
                        }
                    } else {
                        add_to_buckets(&user_dir.path, &user_dir.name, user_dir.size, true, &mut bucket_sizes, &mut buckets);
                    }
                }
            } else {
                add_to_buckets(&child.path, &child.name, child.size, true, &mut bucket_sizes, &mut buckets);
            }
        } else if name_lower == "programdata" {
            if let Some(pd_dirs) = indexed.children_of(&child.path) {
                for sub in pd_dirs {
                    add_to_buckets(&sub.path, &sub.name, sub.size, true, &mut bucket_sizes, &mut buckets);
                }
            } else {
                add_to_buckets(&child.path, &child.name, child.size, true, &mut bucket_sizes, &mut buckets);
            }
        } else {
            add_to_buckets(&child.path, &child.name, child.size, true, &mut bucket_sizes, &mut buckets);
        }
    }

    let mut categories = Vec::new();
    let mut actionable_total: u64 = 0;
    let mut non_actionable_total: u64 = 0;
    let system_reserved_bytes = disk_used.saturating_sub(indexed.total_size());

    for meta in CATEGORY_META {
        let size = if meta.id == "system_reserved" {
            system_reserved_bytes
        } else {
            bucket_sizes.get(meta.id).copied().unwrap_or(0)
        };
        let items = buckets.remove(meta.id).unwrap_or_default();
        let percentage = if disk_used > 0 {
            (size as f64 / disk_used as f64) * 100.0
        } else {
            0.0
        };

        let top_items: Vec<BreakdownItem> = if meta.id == "system_reserved" && size > 0 {
            vec![BreakdownItem {
                path: disk_path.clone(),
                name: "卷级系统保留空间".to_string(),
                size,
                item_type: "directory".to_string(),
                can_migrate: false,
                can_delete: false,
                explanation: "这部分不属于普通目录树，通常来自 NTFS 元数据、MFT/USN 日志、卷影副本、还原点或簇分配差额。".to_string(),
            }]
        } else {
            items.iter().map(|(path, name, sz, is_dir)| {
                let expl = explain_path(path);
                BreakdownItem {
                    path: path.to_string(),
                    name: name.to_string(),
                    size: *sz,
                    item_type: if *is_dir { "directory".to_string() } else { "file".to_string() },
                    can_migrate: (meta.actionable == "full" || meta.actionable == "partial")
                        && !is_collection_root(path),
                    can_delete: expl.safe_to_delete,
                    explanation: expl.explanation,
                }
            }).collect()
        };
        let item_count = top_items.len();

        match meta.actionable {
            "full" => actionable_total += size,
            "partial" => actionable_total += size / 2,
            _ => non_actionable_total += size,
        }

        categories.push(BreakdownCategory {
            id: meta.id.to_string(),
            label: meta.label.to_string(),
            description: meta.description.to_string(),
            size,
            percentage,
            item_count,
            actionable: meta.actionable.to_string(),
            color: meta.color.to_string(),
            top_items,
        });
    }

    categories.sort_by_key(|c| std::cmp::Reverse(c.size));

    SpaceBreakdown {
        disk_path,
        disk_total,
        disk_used,
        disk_free: disk_total.saturating_sub(disk_used),
        system_reserved_bytes,
        categories,
        actionable_total,
        non_actionable_total,
        analysis_duration_ms: started.elapsed().as_millis() as u64,
    }
}

fn is_collection_root(path: &str) -> bool {
    let normalized = path
        .to_ascii_lowercase()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string();
    matches!(
        normalized.as_str(),
        "c:\\program files" | "c:\\program files (x86)" | "c:\\programdata" | "c:\\users"
    )
}

/// 扫描 Program Files 下的一级子目录，返回可搬家程序列表。
pub fn get_relocatable_programs(indexed: &IndexedScanResult) -> Vec<RelocatableProgram> {
    let mut programs = Vec::new();
    let program_dirs = ["C:\\Program Files", "C:\\Program Files (x86)"];

    for dir in &program_dirs {
        if let Some(children) = indexed.children_of(dir) {
            for child in children {
                let name_lower = child.name.to_ascii_lowercase();
                if name_lower == "windowsapps" || name_lower == "windows defender" {
                    continue;
                }

                let path = std::path::Path::new(&child.path);
                let safety = crate::safety::detector::analyze(
                    path,
                    crate::migration::LinkType::Junction,
                    None,
                    child.size,
                );

                let verdict_str = match &safety.verdict {
                    crate::safety::Verdict::Safe => "safe",
                    crate::safety::Verdict::SafeAfterAction => "safe_after_action",
                    crate::safety::Verdict::Blocked => "blocked",
                    crate::safety::Verdict::SystemCritical => "system_critical",
                };

                let verdict_reason = safety
                    .findings
                    .first()
                    .map(|f| f.message.clone())
                    .unwrap_or_else(|| "无已知风险".to_string());

                let can_migrate = safety.can_migrate;
                let app_type = identify_program_type(&child.name, verdict_str);

                programs.push(RelocatableProgram {
                    path: child.path.clone(),
                    name: child.name.clone(),
                    size: child.size,
                    file_count: child.file_count,
                    verdict: verdict_str.to_string(),
                    verdict_reason,
                    can_migrate,
                    app_type,
                });
            }
        }
    }

    programs.sort_by_key(|p| std::cmp::Reverse(p.size));
    programs
}

fn identify_program_type(name: &str, verdict: &str) -> String {
    let name_lower = name.to_ascii_lowercase();

    let ide_keywords = ["visual studio", "jetbrains", "code", "trae", "cursor", "intellij", "rider", "webstorm", "pycharm", "goland", "clion"];
    for kw in &ide_keywords {
        if name_lower.contains(kw) {
            return "ide".to_string();
        }
    }

    let game_keywords = ["steam", "epic", "xbox", "game", "riot", "battle.net", "origin", "ea app"];
    for kw in &game_keywords {
        if name_lower.contains(kw) {
            return "game".to_string();
        }
    }

    let office_keywords = ["microsoft office", "wps", "libreoffice"];
    for kw in &office_keywords {
        if name_lower.contains(kw) {
            return "office".to_string();
        }
    }

    if (name_lower.contains("windows") || name_lower.contains("microsoft")) && verdict == "blocked" {
        return "system".to_string();
    }

    "other".to_string()
}

/// 根据多磁盘信息和空间归类，生成平衡搬迁建议。
pub fn suggest_balance(disks: &[DiskInfo], breakdown: &SpaceBreakdown) -> BalanceSuggestion {
    let source_disk = breakdown.disk_path.clone();

    let target = disks
        .iter()
        .filter(|d| {
            let src_letter = source_disk.trim_end_matches('\\');
            !d.drive_letter.eq_ignore_ascii_case(src_letter)
        })
        .max_by_key(|d| d.free_space);

    let (target_disk, target_used, target_free) = match target {
        Some(d) => (format!("{}\\", d.drive_letter), d.used_space, d.free_space),
        None => ("".to_string(), 0u64, 0u64),
    };

    let mut suggested_items: Vec<BalanceItem> = Vec::new();

    for cat in &breakdown.categories {
        if cat.actionable != "full" && cat.actionable != "partial" {
            continue;
        }
        for item in &cat.top_items {
            if !item.can_migrate {
                continue;
            }

            let path = std::path::Path::new(&item.path);
            let safety = crate::safety::detector::analyze(
                path,
                crate::migration::LinkType::Junction,
                None,
                item.size,
            );
            match safety.verdict {
                crate::safety::Verdict::Blocked | crate::safety::Verdict::SystemCritical => continue,
                _ => {}
            }
            if !safety.can_migrate {
                continue;
            }

            let action = if cat.id == "temp" || cat.id == "app_cache" {
                "redirect"
            } else {
                "migrate"
            };
            let priority = match cat.id.as_str() {
                "app_cache" | "temp" => 1,
                "dev_tools" | "games" => 2,
                _ => 3,
            };
            suggested_items.push(BalanceItem {
                path: item.path.clone(),
                name: item.name.clone(),
                size: item.size,
                category: cat.id.clone(),
                action: action.to_string(),
                priority,
            });
        }
    }

    suggested_items.sort_by(|a, b| a.priority.cmp(&b.priority).then(b.size.cmp(&a.size)));

    let total_movable: u64 = suggested_items.iter().map(|i| i.size).sum();
    let capped_movable = total_movable.min(target_free);

    let current_source_used = breakdown.disk_used;
    let projected_source_used = current_source_used.saturating_sub(capped_movable);
    let projected_target_used = target_used.saturating_add(capped_movable);

    BalanceSuggestion {
        source_disk,
        target_disk,
        current_source_used,
        current_target_used: target_used,
        projected_source_used,
        projected_target_used,
        total_movable: capped_movable,
        suggested_items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: linear scan implementation (old behavior) for comparison.
    fn explain_path_linear(path: &str) -> FileExplanation {
        let path_lower = path.to_ascii_lowercase().replace('/', "\\");
        let all_rules: Vec<ExplainRule> =
            serde_json::from_str(RULES_JSON).expect("invalid explain_rules.json");

        for rule in all_rules.iter() {
            let matched = match rule.match_mode {
                MatchMode::Contains => path_lower.contains(rule.pattern.as_str()),
                MatchMode::EndsWith => path_lower.ends_with(rule.pattern.as_str()),
                MatchMode::StartsWith => path_lower.starts_with(rule.pattern.as_str()),
            };
            if matched {
                return FileExplanation {
                    path: path.to_string(),
                    explanation: rule.explanation.clone(),
                    app_name: rule.app_name.clone(),
                    safe_to_delete: rule.safe_to_delete,
                    will_regenerate: rule.will_regenerate,
                };
            }
        }

        FileExplanation {
            path: path.to_string(),
            explanation: "未识别的文件或目录".to_string(),
            app_name: None,
            safe_to_delete: false,
            will_regenerate: false,
        }
    }

    /// Verify that the AC engine produces the same results as old linear scan
    /// for 20 representative paths.
    #[test]
    fn ac_engine_matches_linear_scan() {
        let test_paths = [
            "C:\\Users\\Test\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache\\data_0",
            "C:\\Users\\Test\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Code Cache\\js\\abcdef",
            "C:\\Users\\Test\\AppData\\Local\\Microsoft\\Edge\\User Data\\Default\\Cache\\index",
            "C:\\Users\\Test\\AppData\\Roaming\\discord\\Cache\\data_1",
            "C:\\Users\\Test\\AppData\\Local\\Temp\\installer.tmp",
            "C:\\Windows\\Temp\\setup12345.log",
            "C:\\Users\\Test\\AppData\\Roaming\\Code\\Cache\\data_2",
            "C:\\Users\\Test\\AppData\\Local\\NVIDIA\\DXCache\\shader.bin",
            "C:\\Users\\Test\\AppData\\Roaming\\npm-cache\\_cacache\\content.txt",
            "C:\\Windows\\SoftwareDistribution\\Download\\abc123.cab",
            "C:\\Users\\Test\\AppData\\Local\\CrashDumps\\app.exe.1234.dmp",
            "C:\\Users\\Test\\AppData\\Local\\Microsoft\\Windows\\Explorer\\thumbcache_256.db",
            "C:\\Users\\Test\\AppData\\Roaming\\Slack\\Cache\\data_3",
            "C:\\Users\\Test\\AppData\\Roaming\\Telegram Desktop\\tdata\\user_data\\cache\\0\\file.dat",
            "C:\\Users\\Test\\AppData\\Local\\pip\\Cache\\wheels\\abc.whl",
            "C:\\Users\\Test\\AppData\\Local\\JetBrains\\IntelliJIdea2024.1\\caches\\index.dat",
            "C:\\Users\\Test\\AppData\\Local\\Steam\\htmlcache\\page.html",
            "C:\\Windows\\Prefetch\\CHROME.EXE-ABCDEF12.pf",
            "C:\\some\\completely\\unknown\\path\\file.xyz",
            "C:\\Users\\Test\\AppData\\Local\\D3DSCache\\shader.bin",
        ];

        for path in &test_paths {
            let ac_result = explain_path(path);
            let linear_result = explain_path_linear(path);
            assert_eq!(
                ac_result.explanation, linear_result.explanation,
                "mismatch for path: {}\n  AC: {}\n  Linear: {}",
                path, ac_result.explanation, linear_result.explanation
            );
            assert_eq!(
                ac_result.safe_to_delete, linear_result.safe_to_delete,
                "safe_to_delete mismatch for path: {}",
                path
            );
        }
    }

    #[test]
    fn explain_path_returns_known_chrome_cache() {
        let result = explain_path(
            "C:\\Users\\Test\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache\\data_0",
        );
        assert!(result.explanation.contains("Chrome"));
        assert!(result.safe_to_delete);
    }

    #[test]
    fn explain_path_returns_known_windows_system_paths() {
        let paths = [
            "C:\\Windows",
            "C:\\pagefile.sys",
            "C:\\hiberfil.sys",
            "C:\\System Volume Information",
            "C:\\$Recycle.Bin",
        ];

        for path in paths {
            let result = explain_path(path);
            assert_ne!(result.explanation, "未识别的文件或目录", "path: {path}");
            assert_eq!(result.app_name.as_deref(), Some("Windows"), "path: {path}");
            assert!(!result.safe_to_delete, "path: {path}");
        }
    }

    #[test]
    fn explain_path_returns_unknown_for_random_path() {
        let result = explain_path("D:\\SomeRandomFolder_XYZ123\\nothing_here.qzx");
        assert_eq!(result.explanation, "未识别的文件或目录");
        assert!(!result.safe_to_delete);
    }
}
