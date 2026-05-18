use serde::Serialize;
use std::sync::OnceLock;

use super::scan_index::IndexedScanResult;
use crate::commands::DiskInfo;

#[derive(Debug, Clone, Serialize)]
pub struct SpaceBreakdown {
    pub disk_path: String,
    pub disk_total: u64,
    pub disk_used: u64,
    pub disk_free: u64,
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

struct ExplainRule {
    pattern: &'static str,
    match_mode: MatchMode,
    explanation: &'static str,
    app_name: Option<&'static str>,
    safe_to_delete: bool,
    will_regenerate: bool,
}

#[derive(Clone, Copy)]
enum MatchMode {
    Contains,
    EndsWith,
}

fn explain_rules() -> &'static [ExplainRule] {
    static RULES: OnceLock<Vec<ExplainRule>> = OnceLock::new();
    RULES.get_or_init(|| vec![
        ExplainRule { pattern: "\\google\\chrome\\user data\\default\\cache", match_mode: MatchMode::Contains, explanation: "Chrome 浏览器缓存，删了会自动重建", app_name: Some("Google Chrome"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\google\\chrome\\user data\\default\\code cache", match_mode: MatchMode::Contains, explanation: "Chrome 编译后的 JS 缓存", app_name: Some("Google Chrome"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\google\\chrome\\user data\\default\\gpucache", match_mode: MatchMode::Contains, explanation: "Chrome GPU 着色器缓存", app_name: Some("Google Chrome"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\google\\chrome\\user data\\default\\service worker", match_mode: MatchMode::Contains, explanation: "Chrome Service Worker 缓存", app_name: Some("Google Chrome"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\google\\chrome\\user data\\default\\indexeddb", match_mode: MatchMode::Contains, explanation: "Chrome 网站本地数据库，删了可能丢网站离线数据", app_name: Some("Google Chrome"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\google\\chrome\\user data\\default\\local storage", match_mode: MatchMode::Contains, explanation: "Chrome 网站本地存储", app_name: Some("Google Chrome"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\google\\chrome\\user data\\crashpad", match_mode: MatchMode::Contains, explanation: "Chrome 崩溃转储报告", app_name: Some("Google Chrome"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\google\\chrome\\user data\\default\\blob_storage", match_mode: MatchMode::Contains, explanation: "Chrome Blob 临时存储", app_name: Some("Google Chrome"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\google\\chrome\\user data\\swreporter", match_mode: MatchMode::Contains, explanation: "Chrome 软件清理工具报告", app_name: Some("Google Chrome"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\microsoft\\edge\\user data\\default\\cache", match_mode: MatchMode::Contains, explanation: "Edge 浏览器缓存，删了会自动重建", app_name: Some("Microsoft Edge"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\microsoft\\edge\\user data\\default\\code cache", match_mode: MatchMode::Contains, explanation: "Edge 编译后的 JS 缓存", app_name: Some("Microsoft Edge"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\microsoft\\edge\\user data\\default\\gpucache", match_mode: MatchMode::Contains, explanation: "Edge GPU 着色器缓存", app_name: Some("Microsoft Edge"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\microsoft\\edge\\user data\\default\\service worker", match_mode: MatchMode::Contains, explanation: "Edge Service Worker 缓存", app_name: Some("Microsoft Edge"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\microsoft\\edge\\user data\\default\\indexeddb", match_mode: MatchMode::Contains, explanation: "Edge 网站本地数据库", app_name: Some("Microsoft Edge"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\microsoft\\edge\\user data\\crashpad", match_mode: MatchMode::Contains, explanation: "Edge 崩溃转储报告", app_name: Some("Microsoft Edge"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\mozilla\\firefox\\profiles", match_mode: MatchMode::Contains, explanation: "Firefox 用户配置文件", app_name: Some("Firefox"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\mozilla\\firefox\\crash reports", match_mode: MatchMode::Contains, explanation: "Firefox 崩溃转储报告", app_name: Some("Firefox"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\cache2\\entries", match_mode: MatchMode::Contains, explanation: "Firefox 浏览器磁盘缓存", app_name: Some("Firefox"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\code\\cachedextensionvsixs", match_mode: MatchMode::Contains, explanation: "VS Code 扩展安装包缓存，删了重装扩展时会重新下载", app_name: Some("VS Code"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\code\\user\\globalstorage", match_mode: MatchMode::Contains, explanation: "VS Code 扩展全局数据", app_name: Some("VS Code"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\code\\logs", match_mode: MatchMode::Contains, explanation: "VS Code 日志文件", app_name: Some("VS Code"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\code\\cacheddata", match_mode: MatchMode::Contains, explanation: "VS Code 编译缓存", app_name: Some("VS Code"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\code\\cache", match_mode: MatchMode::Contains, explanation: "VS Code 通用缓存", app_name: Some("VS Code"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\trae\\cachedextensionvsixs", match_mode: MatchMode::Contains, explanation: "Trae 扩展安装包缓存", app_name: Some("Trae"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\trae\\logs", match_mode: MatchMode::Contains, explanation: "Trae 日志文件", app_name: Some("Trae"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\trae\\cacheddata", match_mode: MatchMode::Contains, explanation: "Trae 编译缓存", app_name: Some("Trae"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\cursor\\cachedextensionvsixs", match_mode: MatchMode::Contains, explanation: "Cursor 扩展安装包缓存", app_name: Some("Cursor"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\cursor\\logs", match_mode: MatchMode::Contains, explanation: "Cursor 日志文件", app_name: Some("Cursor"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\cursor\\cacheddata", match_mode: MatchMode::Contains, explanation: "Cursor 编译缓存", app_name: Some("Cursor"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\cursor\\user\\globalstorage", match_mode: MatchMode::Contains, explanation: "Cursor 扩展全局数据", app_name: Some("Cursor"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "node_modules", match_mode: MatchMode::Contains, explanation: "Node.js 项目依赖，删了用 npm install 重建", app_name: Some("Node.js"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\.npm\\_cacache", match_mode: MatchMode::Contains, explanation: "npm 全局包缓存", app_name: Some("npm"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\.npm", match_mode: MatchMode::Contains, explanation: "npm 缓存目录", app_name: Some("npm"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\.yarn\\cache", match_mode: MatchMode::Contains, explanation: "Yarn 包缓存", app_name: Some("Yarn"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\.pnpm-store", match_mode: MatchMode::Contains, explanation: "pnpm 全局包存储，删了需要重新下载依赖", app_name: Some("pnpm"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "__pycache__", match_mode: MatchMode::Contains, explanation: "Python 字节码缓存，删了运行时会自动重建", app_name: Some("Python"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\pip\\cache", match_mode: MatchMode::Contains, explanation: "pip 下载缓存", app_name: Some("Python pip"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\.cargo\\registry", match_mode: MatchMode::Contains, explanation: "Rust crate 源码缓存，删了 cargo build 会重新下载", app_name: Some("Rust Cargo"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\target\\debug", match_mode: MatchMode::Contains, explanation: "Rust debug 编译产物", app_name: Some("Rust Cargo"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\target\\release", match_mode: MatchMode::Contains, explanation: "Rust release 编译产物", app_name: Some("Rust Cargo"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\.gradle\\caches", match_mode: MatchMode::Contains, explanation: "Gradle 构建缓存", app_name: Some("Gradle"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\.m2\\repository", match_mode: MatchMode::Contains, explanation: "Maven 本地仓库缓存", app_name: Some("Maven"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\docker\\overlay2", match_mode: MatchMode::Contains, explanation: "Docker 镜像层存储", app_name: Some("Docker"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\docker\\volumes", match_mode: MatchMode::Contains, explanation: "Docker 数据卷", app_name: Some("Docker"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\docker\\tmp", match_mode: MatchMode::Contains, explanation: "Docker 临时文件", app_name: Some("Docker"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "pagefile.sys", match_mode: MatchMode::EndsWith, explanation: "Windows 虚拟内存页面文件，系统管理不可删", app_name: Some("Windows"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "hiberfil.sys", match_mode: MatchMode::EndsWith, explanation: "Windows 休眠文件，可通过 powercfg /h off 关闭", app_name: Some("Windows"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "swapfile.sys", match_mode: MatchMode::EndsWith, explanation: "Windows UWP 应用交换文件，系统管理不可删", app_name: Some("Windows"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\windows\\winsxs", match_mode: MatchMode::Contains, explanation: "Windows 组件存储，系统关键目录不可删", app_name: Some("Windows"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\windows\\installer", match_mode: MatchMode::Contains, explanation: "Windows Installer 补丁缓存，可用 DISM 清理", app_name: Some("Windows"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\windows\\softwaredistribution", match_mode: MatchMode::Contains, explanation: "Windows Update 下载缓存，可安全清理", app_name: Some("Windows Update"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\windows\\temp", match_mode: MatchMode::Contains, explanation: "Windows 系统临时文件", app_name: Some("Windows"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\local\\temp", match_mode: MatchMode::Contains, explanation: "用户临时文件夹", app_name: None, safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\crashdumps", match_mode: MatchMode::Contains, explanation: "应用崩溃转储文件", app_name: None, safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\nvidia corporation\\nv_cache", match_mode: MatchMode::Contains, explanation: "NVIDIA 着色器缓存", app_name: Some("NVIDIA"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\nvidia corporation\\downloader", match_mode: MatchMode::Contains, explanation: "NVIDIA 驱动下载缓存", app_name: Some("NVIDIA"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\amd\\dxcache", match_mode: MatchMode::Contains, explanation: "AMD DirectX 着色器缓存", app_name: Some("AMD"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\steam\\steamapps\\common", match_mode: MatchMode::Contains, explanation: "Steam 游戏安装目录", app_name: Some("Steam"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\steam\\steamapps\\workshop", match_mode: MatchMode::Contains, explanation: "Steam 创意工坊内容", app_name: Some("Steam"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: "\\epic games", match_mode: MatchMode::Contains, explanation: "Epic Games 游戏安装目录", app_name: Some("Epic Games"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: ".vhdx", match_mode: MatchMode::EndsWith, explanation: "Hyper-V 虚拟硬盘文件", app_name: Some("Hyper-V"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: ".iso", match_mode: MatchMode::EndsWith, explanation: "光盘镜像文件", app_name: None, safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: ".vmdk", match_mode: MatchMode::EndsWith, explanation: "VMware 虚拟磁盘文件", app_name: Some("VMware"), safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: ".bak", match_mode: MatchMode::EndsWith, explanation: "备份文件，确认不需要后可删", app_name: None, safe_to_delete: false, will_regenerate: false },
        ExplainRule { pattern: ".tmp", match_mode: MatchMode::EndsWith, explanation: "临时文件", app_name: None, safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: ".dmp", match_mode: MatchMode::EndsWith, explanation: "内存转储文件，调试用", app_name: None, safe_to_delete: true, will_regenerate: false },
        ExplainRule { pattern: ".log", match_mode: MatchMode::EndsWith, explanation: "日志文件", app_name: None, safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\windows\\logs", match_mode: MatchMode::Contains, explanation: "Windows 系统日志", app_name: Some("Windows"), safe_to_delete: true, will_regenerate: true },
        ExplainRule { pattern: "\\windows\\prefetch", match_mode: MatchMode::Contains, explanation: "Windows 预读取缓存，加速应用启动", app_name: Some("Windows"), safe_to_delete: true, will_regenerate: true },
    ])
}

/// 给任意路径生成一句话解释。
pub fn explain_path(path: &str) -> FileExplanation {
    let path_lower = path.to_ascii_lowercase().replace('/', "\\");
    let rules = explain_rules();

    for rule in rules.iter() {
        let matched = match rule.match_mode {
            MatchMode::Contains => path_lower.contains(rule.pattern),
            MatchMode::EndsWith => path_lower.ends_with(rule.pattern),
        };
        if matched {
            return FileExplanation {
                path: path.to_string(),
                explanation: rule.explanation.to_string(),
                app_name: rule.app_name.map(|s| s.to_string()),
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
    let system_prefixes = [
        "c:\\windows\\",
        "c:\\$recycle.bin",
        "c:\\system volume information",
        "c:\\recovery",
        "c:\\boot",
        "c:\\efi",
    ];
    for prefix in &system_prefixes {
        if path_lower.starts_with(prefix) {
            return "system";
        }
    }

    if (path_lower.starts_with("c:\\program files\\") || path_lower.starts_with("c:\\program files (x86)\\"))
        && !path_lower.contains("\\windowsapps\\")
    {
        if path_lower.contains("\\steam\\steamapps\\")
            || path_lower.contains("\\epic games\\")
            || path_lower.contains("\\xbox games\\")
        {
            return "games";
        }
        return "programs";
    }

    let user_profile = get_user_profile_lower();
    let user_data_dirs = ["\\documents", "\\desktop", "\\downloads", "\\pictures", "\\videos", "\\music", "\\onedrive"];
    for dir in &user_data_dirs {
        let prefix = format!("{}{}", user_profile, dir);
        if path_lower.starts_with(&prefix) {
            return "user_data";
        }
    }

    if path_lower.contains("\\steam\\steamapps\\")
        || path_lower.contains("\\epic games\\")
        || path_lower.contains("\\xboxgames\\")
        || path_lower.contains("\\xbox games\\")
    {
        return "games";
    }

    let temp_indicators = [
        "\\local\\temp\\", "\\local\\temp",
        "\\windows\\temp\\", "\\windows\\temp",
        "\\crashdumps\\", "\\crashdumps",
    ];
    for indicator in &temp_indicators {
        if path_lower.contains(indicator) || path_lower.ends_with(indicator.trim_end_matches('\\')) {
            return "temp";
        }
    }

    let cache_indicators = [
        "\\cache\\", "\\cache2\\", "\\gpucache\\", "\\code cache\\",
        "\\cacheddata\\", "\\cachedextensionvsixs\\", "\\shader cache\\",
        "\\nv_cache\\", "\\dxcache\\",
    ];
    for indicator in &cache_indicators {
        if path_lower.contains(indicator) {
            return "app_cache";
        }
    }
    if path_lower.ends_with("\\cache") || path_lower.ends_with("\\gpucache") || path_lower.ends_with("\\code cache") {
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

    "other"
}

/// 把整棵目录树按"归属"分成 7~9 个大桶。
pub fn analyze_space_breakdown(indexed: &IndexedScanResult, disk_total: u64, disk_used: u64) -> SpaceBreakdown {
    let started = std::time::Instant::now();
    let disk_path = indexed.root_path().to_string();

    let mut buckets: std::collections::HashMap<&'static str, Vec<(&str, &str, u64, bool)>> =
        std::collections::HashMap::new();
    let mut bucket_sizes: std::collections::HashMap<&'static str, u64> =
        std::collections::HashMap::new();

    for node in indexed.iter_nodes() {
        let path_lower = node.path.to_ascii_lowercase().replace('/', "\\");
        let category = classify_path(&path_lower);
        *bucket_sizes.entry(category).or_insert(0) += node.size;
        let items = buckets.entry(category).or_default();
        if items.len() < 10 || node.size > items.last().map(|i| i.2).unwrap_or(0) {
            items.push((&node.path, &node.name, node.size, true));
            items.sort_by(|a, b| b.2.cmp(&a.2));
            if items.len() > 10 {
                items.truncate(10);
            }
        }
    }

    for file in indexed.large_files_iter() {
        let path_lower = file.path.to_ascii_lowercase().replace('/', "\\");
        let category = classify_path(&path_lower);
        let items = buckets.entry(category).or_default();
        if items.len() < 10 || file.size > items.last().map(|i| i.2).unwrap_or(0) {
            items.push((&file.path, &file.name, file.size, false));
            items.sort_by(|a, b| b.2.cmp(&a.2));
            if items.len() > 10 {
                items.truncate(10);
            }
        }
    }

    let mut categories = Vec::new();
    let mut actionable_total: u64 = 0;
    let mut non_actionable_total: u64 = 0;

    for meta in CATEGORY_META {
        let size = bucket_sizes.get(meta.id).copied().unwrap_or(0);
        let items = buckets.remove(meta.id).unwrap_or_default();
        let percentage = if disk_used > 0 {
            (size as f64 / disk_used as f64) * 100.0
        } else {
            0.0
        };

        let top_items: Vec<BreakdownItem> = items
            .iter()
            .map(|(path, name, sz, is_dir)| {
                let expl = explain_path(path);
                BreakdownItem {
                    path: path.to_string(),
                    name: name.to_string(),
                    size: *sz,
                    item_type: if *is_dir { "directory".to_string() } else { "file".to_string() },
                    can_migrate: meta.actionable == "full" || meta.actionable == "partial",
                    can_delete: expl.safe_to_delete,
                    explanation: expl.explanation,
                }
            })
            .collect();

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
            item_count: items.len(),
            actionable: meta.actionable.to_string(),
            color: meta.color.to_string(),
            top_items,
        });
    }

    categories.sort_by(|a, b| b.size.cmp(&a.size));

    SpaceBreakdown {
        disk_path,
        disk_total,
        disk_used,
        disk_free: disk_total.saturating_sub(disk_used),
        categories,
        actionable_total,
        non_actionable_total,
        analysis_duration_ms: started.elapsed().as_millis() as u64,
    }
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

    programs.sort_by(|a, b| b.size.cmp(&a.size));
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
