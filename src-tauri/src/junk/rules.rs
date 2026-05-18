use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JunkCategory {
    SystemTemp,
    BrowserCache,
    WindowsUpdate,
    ThumbnailCache,
    RecycleBin,
    CrashDump,
    AppLogs,
    FontCache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JunkRiskLevel {
    Safe,
    Caution,
    Risky,
}

#[derive(Debug, Clone, Serialize)]
pub struct JunkRule {
    pub id: &'static str,
    pub category: JunkCategory,
    pub name: &'static str,
    pub description: &'static str,
    pub paths: Vec<String>,
    pub patterns: Vec<&'static str>,
    pub risk_level: JunkRiskLevel,
    pub requires_admin: bool,
    pub default_selected: bool,
    pub clean_subdirs_only: bool,
    pub max_depth: Option<u32>,
}

pub(super) fn env_path(var: &str, suffix: &str) -> Option<String> {
    let base = std::env::var(var).ok()?;
    let base = base.trim().trim_end_matches(['\\', '/']);
    if base.is_empty() {
        return None;
    }
    if suffix.is_empty() {
        Some(base.to_string())
    } else {
        let suffix = suffix.trim_start_matches(['\\', '/']);
        Some(format!("{}\\{}", base, suffix))
    }
}

pub(super) fn collect_paths<I: IntoIterator<Item = Option<String>>>(iter: I) -> Vec<String> {
    iter.into_iter().flatten().collect()
}

pub(super) fn push_rule_if_paths(rules: &mut Vec<JunkRule>, rule: JunkRule) {
    if !rule.paths.is_empty() {
        rules.push(rule);
    }
}

#[path = "rules_browsers.rs"]
mod rules_browsers;

#[path = "rules_apps.rs"]
mod rules_apps;

#[path = "rule_loader.rs"]
pub mod rule_loader;

fn os_layer_rules() -> Vec<JunkRule> {
    let mut rules: Vec<JunkRule> = Vec::new();

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "user_temp",
            category: JunkCategory::SystemTemp,
            name: "用户临时文件夹",
            description: "当前用户 %TEMP% 目录下的临时文件，应用退出后大多不再使用",
            paths: collect_paths([env_path("TEMP", "")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "local_appdata_temp",
            category: JunkCategory::SystemTemp,
            name: "本地应用临时文件夹",
            description: "%LOCALAPPDATA%\\Temp 中的临时文件，安装包/解压残留常驻于此",
            paths: collect_paths([env_path("LOCALAPPDATA", "Temp")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "windows_temp",
            category: JunkCategory::SystemTemp,
            name: "Windows 临时文件夹",
            description: "系统 %WINDIR%\\Temp 目录，安装/更新过程中产生的中转文件",
            paths: collect_paths([env_path("WINDIR", "Temp")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "windows_prefetch",
            category: JunkCategory::SystemTemp,
            name: "Windows Prefetch",
            description: "预读取缓存（*.pf），删除后下一次启动会自动重建，仅短暂影响启动速度",
            paths: collect_paths([env_path("WINDIR", "Prefetch")]),
            patterns: vec!["*.pf"],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "windows_update_download",
            category: JunkCategory::WindowsUpdate,
            name: "Windows 更新下载缓存",
            description: "%WINDIR%\\SoftwareDistribution\\Download，已安装的更新包残留，可释放数 GB",
            paths: collect_paths([env_path("WINDIR", "SoftwareDistribution\\Download")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: true,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "windows_update_logs",
            category: JunkCategory::WindowsUpdate,
            name: "Windows 更新日志",
            description: "%WINDIR%\\Logs\\WindowsUpdate，仅排错使用，可安全删除",
            paths: collect_paths([env_path("WINDIR", "Logs\\WindowsUpdate")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: true,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "delivery_optimization_cache",
            category: JunkCategory::WindowsUpdate,
            name: "Windows 传递优化缓存",
            description: "%WINDIR%\\SoftwareDistribution\\DeliveryOptimization\\Cache，P2P 更新分发缓存，企业网常达数 GB",
            paths: collect_paths([env_path(
                "WINDIR",
                "SoftwareDistribution\\DeliveryOptimization\\Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: true,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "windows_setup_panther",
            category: JunkCategory::WindowsUpdate,
            name: "Windows 安装/升级日志（Panther）",
            description: "%WINDIR%\\Panther，安装与功能更新的部署日志，体积可达数百 MB",
            paths: collect_paths([env_path("WINDIR", "Panther")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: true,
            default_selected: false,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "windows_setupapi_logs",
            category: JunkCategory::AppLogs,
            name: "驱动安装日志（setupapi）",
            description: "%WINDIR%\\inf\\setupapi.*.log，驱动与 INF 安装日志，体积可达数百 MB，仅排障保留",
            paths: collect_paths([env_path("WINDIR", "inf")]),
            patterns: vec!["setupapi.app.log", "setupapi.dev.log", "setupapi.*.log"],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: true,
            default_selected: false,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "thumbnail_cache",
            category: JunkCategory::ThumbnailCache,
            name: "资源管理器缩略图缓存",
            description: "thumbcache_*.db / iconcache_*.db，删除后首次浏览文件夹会重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "Microsoft\\Windows\\Explorer")]),
            patterns: vec!["thumbcache_*.db", "iconcache_*.db"],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
        },
    );

    rules.push(JunkRule {
        id: "recycle_bin_c",
        category: JunkCategory::RecycleBin,
        name: "回收站 (C:)",
        description: "C: 盘回收站，删除后无法从回收站还原，请确认其中无需要的文件",
        paths: vec!["C:\\$Recycle.Bin".to_string()],
        patterns: vec![],
        risk_level: JunkRiskLevel::Caution,
        requires_admin: false,
        default_selected: false,
        clean_subdirs_only: true,
        max_depth: None,
    });

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "crash_dumps",
            category: JunkCategory::CrashDump,
            name: "应用崩溃转储",
            description: "%LOCALAPPDATA%\\CrashDumps，应用崩溃时生成的内存快照，体积常达数百 MB",
            paths: collect_paths([env_path("LOCALAPPDATA", "CrashDumps")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "windows_error_reporting",
            category: JunkCategory::CrashDump,
            name: "Windows 错误报告 (用户级 WER)",
            description: "%LOCALAPPDATA%\\Microsoft\\Windows\\WER，已上报或排队中的用户级错误报告",
            paths: collect_paths([env_path("LOCALAPPDATA", "Microsoft\\Windows\\WER")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "wer_report_archive",
            category: JunkCategory::CrashDump,
            name: "Windows 错误报告归档（系统级）",
            description: "%PROGRAMDATA%\\Microsoft\\Windows\\WER\\ReportArchive，已归档的系统级错误报告",
            paths: collect_paths([env_path("PROGRAMDATA", "Microsoft\\Windows\\WER\\ReportArchive")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: true,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "wer_report_queue",
            category: JunkCategory::CrashDump,
            name: "Windows 错误报告队列（系统级）",
            description: "%PROGRAMDATA%\\Microsoft\\Windows\\WER\\ReportQueue，等待上报的系统级错误报告",
            paths: collect_paths([env_path("PROGRAMDATA", "Microsoft\\Windows\\WER\\ReportQueue")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: true,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "wer_temp",
            category: JunkCategory::CrashDump,
            name: "Windows 错误报告临时数据",
            description: "%PROGRAMDATA%\\Microsoft\\Windows\\WER\\Temp，错误报告生成过程中的临时文件",
            paths: collect_paths([env_path("PROGRAMDATA", "Microsoft\\Windows\\WER\\Temp")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: true,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "windows_minidump",
            category: JunkCategory::CrashDump,
            name: "Windows 小型转储",
            description: "%WINDIR%\\Minidump，蓝屏时生成的小型内存转储，仅排障保留",
            paths: collect_paths([env_path("WINDIR", "Minidump")]),
            patterns: vec!["*.dmp"],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: true,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "live_kernel_reports",
            category: JunkCategory::CrashDump,
            name: "Live Kernel 报告",
            description: "%WINDIR%\\LiveKernelReports，内核挂起诊断转储，体积常达数百 MB",
            paths: collect_paths([env_path("WINDIR", "LiveKernelReports")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: true,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "windows_update_log_legacy",
            category: JunkCategory::AppLogs,
            name: "Windows 旧版更新日志",
            description: "WindowsUpdate.log，旧版本系统遗留的纯文本更新日志，删除前请确认不再排错",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\Windows\\WindowsUpdate.log",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "cbs_logs",
            category: JunkCategory::AppLogs,
            name: "CBS 组件存储日志",
            description: "%WINDIR%\\Logs\\CBS，组件服务（SFC/DISM）日志，长期累积可达数百 MB",
            paths: collect_paths([env_path("WINDIR", "Logs\\CBS")]),
            patterns: vec!["*.log", "*.cab"],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: true,
            default_selected: false,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "dism_logs",
            category: JunkCategory::AppLogs,
            name: "DISM 部署日志",
            description: "%WINDIR%\\Logs\\DISM，部署映像服务和管理工具日志，仅排障保留",
            paths: collect_paths([env_path("WINDIR", "Logs\\DISM")]),
            patterns: vec!["*.log"],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: true,
            default_selected: false,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "system_font_cache",
            category: JunkCategory::FontCache,
            name: "系统字体缓存",
            description: "%WINDIR%\\ServiceProfiles\\LocalService\\AppData\\Local\\FontCache，删除后系统会重建",
            paths: collect_paths([env_path(
                "WINDIR",
                "ServiceProfiles\\LocalService\\AppData\\Local\\FontCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: true,
            default_selected: false,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "user_font_cache",
            category: JunkCategory::FontCache,
            name: "用户字体缓存",
            description: "%LOCALAPPDATA%\\FontCache，每用户字体缓存，删除后系统会重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "FontCache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: true,
            max_depth: None,
        },
    );

    rules
}

pub fn all_rules() -> Vec<JunkRule> {
    // Load JSON-based rules first (new system)
    let mut rules = rule_loader::load_json_rules();

    // Append legacy hardcoded rules, deduplicating by id
    let json_ids: std::collections::HashSet<&str> = rules.iter().map(|r| r.id).collect();

    let mut legacy = os_layer_rules();
    legacy.extend(rules_browsers::extra_rules());
    legacy.extend(rules_apps::extra_rules());

    for rule in legacy {
        if !json_ids.contains(rule.id) {
            rules.push(rule);
        }
    }

    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn rules_have_unique_ids() {
        let rules = all_rules();
        let mut seen: HashSet<&'static str> = HashSet::new();
        for r in &rules {
            assert!(seen.insert(r.id), "duplicate rule id: {}", r.id);
        }
    }

    #[test]
    fn rule_ids_are_snake_case_ascii() {
        let rules = all_rules();
        for r in &rules {
            assert!(!r.id.is_empty(), "empty id");
            assert!(
                r.id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "rule id is not snake_case ascii: {}",
                r.id
            );
        }
    }

    #[test]
    fn every_rule_has_paths_and_text() {
        let rules = all_rules();
        for r in &rules {
            assert!(!r.paths.is_empty(), "rule {} has empty paths", r.id);
            assert!(!r.name.is_empty(), "rule {} has empty name", r.id);
            assert!(!r.description.is_empty(), "rule {} has empty description", r.id);
        }
    }

    #[test]
    fn recycle_bin_is_present_with_drive_letter() {
        let rules = all_rules();
        let rb = rules
            .iter()
            .find(|r| r.category == JunkCategory::RecycleBin)
            .expect("recycle bin rule missing");
        assert!(rb.paths.iter().any(|p| p.contains("$Recycle.Bin")));
    }

    #[test]
    fn env_path_strips_trailing_slash() {
        std::env::set_var("__JUNK_RULES_TEST_VAR__", "C:\\Some\\Dir\\");
        let p = env_path("__JUNK_RULES_TEST_VAR__", "Sub").unwrap();
        assert_eq!(p, "C:\\Some\\Dir\\Sub");
        std::env::remove_var("__JUNK_RULES_TEST_VAR__");
    }

    #[test]
    fn env_path_returns_none_for_missing_var() {
        std::env::remove_var("__JUNK_RULES_TEST_MISSING__");
        assert!(env_path("__JUNK_RULES_TEST_MISSING__", "x").is_none());
    }

    #[test]
    fn rules_count_meets_enterprise_floor() {
        let rules = all_rules();
        assert!(rules.len() >= 80, "expected >=80 rules, got {}", rules.len());
    }

    #[test]
    fn print_rules_summary() {
        use std::collections::BTreeMap;
        let rules = all_rules();
        let mut by_cat: BTreeMap<String, usize> = BTreeMap::new();
        for r in &rules {
            *by_cat.entry(format!("{:?}", r.category)).or_insert(0) += 1;
        }
        println!("[summary] total={}", rules.len());
        for (k, v) in &by_cat {
            println!("[summary] {}: {}", k, v);
        }
    }
}
