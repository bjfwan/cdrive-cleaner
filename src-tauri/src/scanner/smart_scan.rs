use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use super::file_info::DirectoryNode;
use super::scan_index::IndexedScanResult;

const LARGE_FILE_THRESHOLD: u64 = 512 * 1024 * 1024;
const MIN_GROUP_ITEM_SIZE: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartCategory {
    AppCache,
    DevTools,
    TempFiles,
    LargeFiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartAction {
    Migrate,
    Delete,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartRisk {
    Safe,
    Caution,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct SmartItem {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub file_count: usize,
    pub category: SmartCategory,
    pub recommendation: SmartAction,
    pub risk: SmartRisk,
    pub rule: String,
    pub default_selected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SmartGroup {
    pub category: SmartCategory,
    pub recommendation: SmartAction,
    pub total_size: u64,
    pub item_count: usize,
    pub selected_size: u64,
    pub items: Vec<SmartItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SmartScanReport {
    pub root_path: String,
    pub generated_at_ms: u64,
    pub potential_savings: u64,
    pub default_savings: u64,
    pub groups: Vec<SmartGroup>,
    pub analysis_duration_ms: u64,
}

pub fn build_report(indexed: &IndexedScanResult) -> SmartScanReport {
    let started = std::time::Instant::now();
    let mut buckets: HashMap<SmartCategory, Vec<SmartItem>> = HashMap::new();
    let root_path = indexed.root_path();
    let mut classified_paths: Vec<String> = Vec::new();

    let mut nodes: Vec<&DirectoryNode> = indexed.iter_nodes().collect();
    nodes.sort_by(|a, b| b.size.cmp(&a.size));

    for node in nodes {
        if let Some(item) = classify_dir(node, root_path) {
            if is_inside_classified(&item.path, &classified_paths) {
                continue;
            }
            classified_paths.push(item.path.clone());
            buckets.entry(item.category).or_default().push(item);
        }
    }

    for f in indexed.large_files_iter() {
        if f.size < LARGE_FILE_THRESHOLD {
            continue;
        }
        if is_inside_classified(&f.path, &classified_paths) {
            continue;
        }
        let path_lower = f.path.to_ascii_lowercase();
        if is_in_global_blocklist(&path_lower) {
            continue;
        }
        let p = Path::new(&f.path);
        let name = p
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| f.name.clone());

        if is_volume_system_file(&path_lower) {
            buckets
                .entry(SmartCategory::LargeFiles)
                .or_default()
                .push(SmartItem {
                    path: f.path.clone(),
                    name,
                    size: f.size,
                    file_count: 1,
                    category: SmartCategory::LargeFiles,
                    recommendation: SmartAction::Review,
                    risk: SmartRisk::Blocked,
                    rule: "Windows 系统专用文件，不可迁移".into(),
                    default_selected: false,
                });
            continue;
        }

        if let Some((rule, risk, category)) = classify_large_file(&path_lower) {
            let recommendation = group_recommendation(category);
            let default_selected =
                matches!(risk, SmartRisk::Safe) && !matches!(recommendation, SmartAction::Review);
            buckets.entry(category).or_default().push(SmartItem {
                path: f.path.clone(),
                name,
                size: f.size,
                file_count: 1,
                category,
                recommendation,
                risk,
                rule: rule.to_string(),
                default_selected,
            });
            continue;
        }

        buckets
            .entry(SmartCategory::LargeFiles)
            .or_default()
            .push(SmartItem {
                path: f.path.clone(),
                name,
                size: f.size,
                file_count: 1,
                category: SmartCategory::LargeFiles,
                recommendation: SmartAction::Review,
                risk: SmartRisk::Unknown,
                rule: ">= 512MB 单文件".into(),
                default_selected: false,
            });
    }

    let mut groups: Vec<SmartGroup> = buckets
        .into_iter()
        .map(|(category, mut items)| {
            items.sort_by(|a, b| b.size.cmp(&a.size));
            if items.len() > 80 {
                items.truncate(80);
            }
            let total_size: u64 = items.iter().map(|i| i.size).sum();
            let selected_size: u64 = items
                .iter()
                .filter(|i| i.default_selected)
                .map(|i| i.size)
                .sum();
            let recommendation = group_recommendation(category);
            SmartGroup {
                category,
                recommendation,
                total_size,
                item_count: items.len(),
                selected_size,
                items,
            }
        })
        .collect();

    groups.sort_by(|a, b| b.selected_size.cmp(&a.selected_size));

    let potential_savings: u64 = groups.iter().map(|g| g.total_size).sum();
    let default_savings: u64 = groups.iter().map(|g| g.selected_size).sum();
    let generated_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    SmartScanReport {
        root_path: root_path.to_string(),
        generated_at_ms,
        potential_savings,
        default_savings,
        groups,
        analysis_duration_ms: started.elapsed().as_millis() as u64,
    }
}

fn group_recommendation(c: SmartCategory) -> SmartAction {
    match c {
        SmartCategory::AppCache => SmartAction::Migrate,
        SmartCategory::DevTools => SmartAction::Migrate,
        SmartCategory::TempFiles => SmartAction::Delete,
        SmartCategory::LargeFiles => SmartAction::Review,
    }
}

fn is_inside_classified(path: &str, classified: &[String]) -> bool {
    let lower = path.to_ascii_lowercase();
    classified.iter().any(|root| {
        let root_lower = root.to_ascii_lowercase();
        let mut prefix = root_lower.clone();
        if !prefix.ends_with('\\') {
            prefix.push('\\');
        }
        lower.starts_with(&prefix)
    })
}

fn is_in_global_blocklist(path_lower: &str) -> bool {
    let blocked_prefixes = [
        "c:\\windows\\",
        "c:\\$recycle.bin\\",
        "c:\\system volume information\\",
        "c:\\recovery\\",
        "c:\\$windows.~bt\\",
        "c:\\$windows.~ws\\",
        "c:\\onedrivetemp\\",
        "c:\\perflogs\\",
        "c:\\boot\\",
        "c:\\efi\\",
        "c:\\msocache\\",
    ];
    if blocked_prefixes.iter().any(|kw| path_lower.starts_with(kw)) {
        return true;
    }
    let blocked_anywhere = [
        "\\winsxs\\",
        "\\system32\\config\\",
        "\\appdata\\roaming\\microsoft\\windows\\",
        "\\appdata\\local\\microsoft\\windows\\",
        "\\appdata\\local\\packages\\",
        "\\driverstore\\filerepository\\",
    ];
    if blocked_anywhere.iter().any(|kw| path_lower.contains(kw)) {
        return true;
    }
    if path_lower.starts_with("c:\\program files\\")
        && !path_lower.contains("\\steam\\steamapps\\")
    {
        return true;
    }
    if path_lower.starts_with("c:\\program files (x86)\\")
        && !path_lower.contains("\\steam\\steamapps\\")
    {
        return true;
    }
    if path_lower.starts_with("c:\\programdata\\") {
        let safe_under_programdata = [
            "c:\\programdata\\nvidia corporation\\downloader",
            "c:\\programdata\\nvidia corporation\\nv_cache",
            "c:\\programdata\\nvidia corporation\\drs",
            "c:\\programdata\\amd\\dxcache",
        ];
        if !safe_under_programdata.iter().any(|p| path_lower.starts_with(p)) {
            return true;
        }
    }
    false
}

fn classify_dir(node: &DirectoryNode, root_path: &str) -> Option<SmartItem> {
    if node.size < MIN_GROUP_ITEM_SIZE {
        return None;
    }
    if node.is_symlink {
        return None;
    }
    if path_eq_root(&node.path, root_path) {
        return None;
    }

    let name_lower = node.name.to_ascii_lowercase();
    let path_lower = node.path.to_ascii_lowercase();

    if is_in_global_blocklist(&path_lower) {
        return None;
    }
    if is_blocked_user_dir(&path_lower) {
        return None;
    }

    if let Some((rule, risk)) = match_dev_tools(&name_lower, &path_lower) {
        return Some(make_item(node, SmartCategory::DevTools, SmartAction::Migrate, rule, risk));
    }

    if let Some((rule, risk)) = match_temp(&name_lower, &path_lower) {
        return Some(make_item(node, SmartCategory::TempFiles, SmartAction::Delete, rule, risk));
    }

    if let Some((rule, risk)) = match_app_cache(&name_lower, &path_lower) {
        return Some(make_item(node, SmartCategory::AppCache, SmartAction::Migrate, rule, risk));
    }

    None
}

fn is_blocked_user_dir(path_lower: &str) -> bool {
    let critical_user_paths = [
        "\\appdata\\roaming\\microsoft\\",
        "\\appdata\\local\\microsoft\\onedrive\\",
        "\\appdata\\local\\microsoft\\office\\",
        "\\appdata\\local\\microsoft\\windowsapps\\",
    ];
    if critical_user_paths.iter().any(|kw| path_lower.contains(kw)) {
        return true;
    }

    let user_home_subdirs = [
        "contacts",
        "desktop",
        "documents",
        "downloads",
        "favorites",
        "links",
        "music",
        "onedrive",
        "pictures",
        "saved games",
        "searches",
        "videos",
    ];
    if let Some(rest) = path_lower.strip_prefix("c:\\users\\") {
        if let Some(slash) = rest.find('\\') {
            let after_user = &rest[slash + 1..];
            for sub in user_home_subdirs {
                if after_user == sub || after_user.starts_with(&format!("{}\\", sub)) {
                    return true;
                }
            }
        }
    }
    false
}

fn match_dev_tools(name: &str, path: &str) -> Option<(&'static str, SmartRisk)> {
    if name == "node_modules" {
        return Some(("node_modules", SmartRisk::Safe));
    }
    if name == ".pnpm-store" {
        return Some(("pnpm store", SmartRisk::Safe));
    }
    if name == ".pnpm" && path.contains("\\.pnpm") {
        return Some(("pnpm 缓存", SmartRisk::Safe));
    }
    if path.ends_with("\\.gradle\\caches") || path.contains("\\.gradle\\caches\\") {
        return Some(("Gradle 缓存", SmartRisk::Safe));
    }
    if name == ".gradle" {
        return Some(("Gradle 主目录", SmartRisk::Caution));
    }
    if name == ".m2" {
        return Some(("Maven (.m2)", SmartRisk::Safe));
    }
    if path.contains("\\.cargo\\registry") || path.contains("\\.cargo\\git") {
        return Some(("Rust 包缓存", SmartRisk::Safe));
    }
    if name == ".cargo" || name == ".rustup" {
        return Some(("Rust toolchain", SmartRisk::Caution));
    }
    if name == ".npm" {
        return Some(("npm 缓存", SmartRisk::Safe));
    }
    if name == ".yarn" || name == ".yarn-cache" {
        return Some(("Yarn 缓存", SmartRisk::Safe));
    }
    if name == ".nuget" {
        return Some(("NuGet 包", SmartRisk::Safe));
    }
    if name == "__pycache__" {
        return Some(("Python __pycache__", SmartRisk::Safe));
    }
    if path.contains("\\pip\\cache") || path.ends_with("\\pip\\cache") {
        return Some(("pip 缓存", SmartRisk::Safe));
    }
    if (name == "venv" || name == ".venv") && path.matches('\\').count() >= 4 {
        return Some(("Python 虚拟环境", SmartRisk::Caution));
    }
    if path.contains("\\go\\pkg\\mod") || path.contains("\\go\\pkg\\cache") {
        return Some(("Go 模块缓存", SmartRisk::Safe));
    }
    None
}

fn match_temp(name: &str, path: &str) -> Option<(&'static str, SmartRisk)> {
    if path.contains("\\appdata\\local\\temp") {
        if path.ends_with("\\appdata\\local\\temp") {
            return Some(("用户 Temp 根", SmartRisk::Safe));
        }
        return Some(("用户 Temp 子目录", SmartRisk::Safe));
    }
    if path.starts_with("c:\\windows\\temp") {
        return Some(("系统 Temp", SmartRisk::Caution));
    }
    if path.ends_with("\\windows\\softwaredistribution\\download") {
        return Some(("Windows Update 下载", SmartRisk::Safe));
    }
    if path.ends_with("\\crashdumps") || name == "crashdumps" || name == "minidump" {
        return Some(("崩溃转储", SmartRisk::Safe));
    }
    if (name == "logs" || name == "log") && path.contains("\\appdata\\") {
        return Some(("日志目录", SmartRisk::Safe));
    }
    None
}

fn match_app_cache(name: &str, path: &str) -> Option<(&'static str, SmartRisk)> {
    let known_apps: &[(&str, &str, SmartRisk)] = &[
        ("\\google\\chrome\\user data\\default\\cache\\cache_data", "Chrome 缓存", SmartRisk::Safe),
        ("\\google\\chrome\\user data\\default\\code cache", "Chrome 代码缓存", SmartRisk::Safe),
        ("\\google\\chrome\\user data\\default\\gpucache", "Chrome GPU 缓存", SmartRisk::Safe),
        ("\\google\\chrome\\user data\\default\\service worker", "Chrome Service Worker", SmartRisk::Safe),
        ("\\google\\chrome\\user data\\default\\optguideondevicemodel", "Chrome 端侧模型", SmartRisk::Safe),
        ("\\google\\chrome\\user data\\optguideondevicemodel", "Chrome 端侧模型", SmartRisk::Safe),
        ("\\google\\chrome\\user data\\shadercache", "Chrome Shader 缓存", SmartRisk::Safe),
        ("\\microsoft\\edge\\user data\\default\\cache\\cache_data", "Edge 缓存", SmartRisk::Safe),
        ("\\microsoft\\edge\\user data\\default\\code cache", "Edge 代码缓存", SmartRisk::Safe),
        ("\\microsoft\\edge\\user data\\default\\gpucache", "Edge GPU 缓存", SmartRisk::Safe),
        ("\\microsoft\\edge\\user data\\default\\service worker", "Edge Service Worker", SmartRisk::Safe),
        ("\\microsoft\\edge\\user data\\shadercache", "Edge Shader 缓存", SmartRisk::Safe),
        ("\\mozilla\\firefox\\profiles\\", "Firefox 配置缓存", SmartRisk::Caution),
        ("\\code\\cache", "VSCode 缓存", SmartRisk::Safe),
        ("\\code\\code cache", "VSCode 代码缓存", SmartRisk::Safe),
        ("\\code\\gpucache", "VSCode GPU 缓存", SmartRisk::Safe),
        ("\\code\\cachedextensions", "VSCode 扩展缓存", SmartRisk::Caution),
        ("\\code\\service worker", "VSCode Service Worker", SmartRisk::Safe),
        ("\\jetbrains\\", "JetBrains 数据", SmartRisk::Caution),
        ("\\trae cn\\", "Trae CN 数据", SmartRisk::Caution),
        ("\\trae\\", "Trae 数据", SmartRisk::Caution),
        ("\\kiro\\", "Kiro 数据", SmartRisk::Caution),
        ("\\cursor\\", "Cursor 数据", SmartRisk::Caution),
        ("\\nvidia corporation\\downloader", "NVIDIA 下载器", SmartRisk::Safe),
        ("\\nvidia corporation\\nv_cache", "NVIDIA 着色器缓存", SmartRisk::Safe),
        ("\\nvidia corporation\\drs", "NVIDIA 配置", SmartRisk::Caution),
        ("\\amd\\dxcache", "AMD 着色器缓存", SmartRisk::Safe),
        ("\\amd\\glcache", "AMD GL 缓存", SmartRisk::Safe),
        ("\\.android\\avd", "Android AVD 镜像", SmartRisk::Caution),
        ("\\docker\\wsl", "Docker WSL 数据", SmartRisk::Caution),
        ("\\unity\\cache", "Unity 缓存", SmartRisk::Safe),
        ("\\unity\\asset store", "Unity Asset Store", SmartRisk::Caution),
        ("\\.gradle\\caches", "Gradle 缓存", SmartRisk::Safe),
    ];

    for (kw, label, risk) in known_apps {
        if path.contains(kw) {
            return Some((label, *risk));
        }
    }

    if (name == "cache" || name == "caches" || name == "code cache" || name == "gpucache")
        && (path.contains("\\appdata\\local\\") || path.contains("\\appdata\\roaming\\"))
    {
        return Some(("通用应用缓存", SmartRisk::Caution));
    }

    None
}

fn make_item(
    node: &DirectoryNode,
    category: SmartCategory,
    recommendation: SmartAction,
    rule: &str,
    risk: SmartRisk,
) -> SmartItem {
    let default_selected =
        matches!(risk, SmartRisk::Safe) && !matches!(recommendation, SmartAction::Review);

    SmartItem {
        path: node.path.clone(),
        name: node.name.clone(),
        size: node.size,
        file_count: node.file_count,
        category,
        recommendation,
        risk,
        rule: rule.to_string(),
        default_selected,
    }
}

fn path_eq_root(p: &str, root: &str) -> bool {
    let a = p.trim_end_matches('\\').to_ascii_lowercase();
    let b = root.trim_end_matches('\\').to_ascii_lowercase();
    a == b
}

fn is_volume_system_file(path_lower: &str) -> bool {
    let blocked_files = [
        "\\pagefile.sys",
        "\\swapfile.sys",
        "\\hiberfil.sys",
        "\\dumpstack.log.tmp",
        "\\dumpstack.log",
        "\\bootmgr",
        "\\bootnxt",
        "\\config.sys",
        "\\msdos.sys",
        "\\io.sys",
        "\\ntldr",
        "\\bootsect.bak",
    ];
    blocked_files.iter().any(|kw| path_lower.ends_with(kw))
}

fn classify_large_file(path_lower: &str) -> Option<(&'static str, SmartRisk, SmartCategory)> {
    let app_cache_files: &[(&str, &str, SmartRisk)] = &[
        ("\\google\\chrome\\user data\\optguideondevicemodel", "Chrome 端侧模型", SmartRisk::Safe),
        ("\\microsoft\\edge\\user data\\optguideondevicemodel", "Edge 端侧模型", SmartRisk::Safe),
        ("\\google\\chrome\\user data\\", "Chrome 用户数据", SmartRisk::Caution),
        ("\\microsoft\\edge\\user data\\", "Edge 用户数据", SmartRisk::Caution),
        ("\\nvidia corporation\\downloader", "NVIDIA 下载器", SmartRisk::Safe),
        ("\\nvidia corporation\\nv_cache", "NVIDIA 着色器缓存", SmartRisk::Safe),
        ("\\amd\\dxcache", "AMD 着色器缓存", SmartRisk::Safe),
    ];
    for (kw, label, risk) in app_cache_files {
        if path_lower.contains(kw) {
            return Some((label, *risk, SmartCategory::AppCache));
        }
    }

    let large_user_files: &[(&[&str], &str, SmartRisk)] = &[
        (&[".iso", ".img"], "光盘镜像", SmartRisk::Safe),
        (&[".vhd", ".vhdx", ".vmdk", ".qcow2"], "虚拟磁盘", SmartRisk::Caution),
        (&[".mp4", ".mkv", ".mov", ".avi", ".webm", ".flv"], "视频文件", SmartRisk::Safe),
        (&[".zip", ".7z", ".rar", ".tar", ".gz", ".xz"], "压缩包", SmartRisk::Safe),
        (&[".pst", ".ost"], "Outlook 邮件库", SmartRisk::Caution),
    ];
    for (exts, label, risk) in large_user_files {
        if exts.iter().any(|ext| path_lower.ends_with(ext)) {
            return Some((label, *risk, SmartCategory::LargeFiles));
        }
    }

    None
}
