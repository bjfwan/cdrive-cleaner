use super::{JunkRule, JunkCategory, JunkRiskLevel};
use super::{env_path, collect_paths, push_rule_if_paths};

pub(super) fn extra_rules() -> Vec<JunkRule> {
    let mut rules: Vec<JunkRule> = Vec::new();

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "chrome_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Chrome 缓存（全部配置）",
            description: "Chrome 所有用户配置（含 Default/Profile N）的网页缓存，删除后首次访问会重新下载",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Google\\Chrome\\User Data\\*\\Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "chrome_code_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Chrome 代码缓存（全部配置）",
            description: "Chrome 所有配置编译后的 JavaScript/WebAssembly 缓存，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Google\\Chrome\\User Data\\*\\Code Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "chrome_gpu_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Chrome GPU 缓存（全部配置）",
            description: "Chrome 所有配置的 GPU 着色器缓存，删除后会被自动重建",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Google\\Chrome\\User Data\\*\\GPUCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "chrome_service_worker_cachestorage_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Chrome Service Worker CacheStorage（全部配置）",
            description: "Chrome 所有配置的 Service Worker CacheStorage，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Google\\Chrome\\User Data\\*\\Service Worker\\CacheStorage",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "chrome_service_worker_scriptcache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Chrome Service Worker ScriptCache（全部配置）",
            description: "Chrome 所有配置的 Service Worker 脚本缓存，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Google\\Chrome\\User Data\\*\\Service Worker\\ScriptCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "chrome_media_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Chrome 媒体缓存（全部配置）",
            description: "Chrome 所有配置的音视频媒体缓存，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Google\\Chrome\\User Data\\*\\Media Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "chrome_application_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Chrome Application Cache（全部配置）",
            description: "Chrome 所有配置的旧版 Application Cache 数据，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Google\\Chrome\\User Data\\*\\Application Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "chrome_shader_cache",
            category: JunkCategory::BrowserCache,
            name: "Chrome 全局 ShaderCache",
            description: "Chrome 跨配置共享的 GPU 着色器缓存，删除后会被自动重建",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Google\\Chrome\\User Data\\ShaderCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "chrome_crashpad_reports",
            category: JunkCategory::CrashDump,
            name: "Chrome 崩溃报告",
            description: "Chrome Crashpad 已生成的崩溃转储与元数据，普通用户无需保留",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Google\\Chrome\\User Data\\Crashpad\\reports",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "edge_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Edge 缓存（全部配置）",
            description: "Edge 所有用户配置的网页缓存，删除后首次访问会重新下载",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\Edge\\User Data\\*\\Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "edge_code_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Edge 代码缓存（全部配置）",
            description: "Edge 所有配置编译后的 JavaScript/WebAssembly 缓存，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\Edge\\User Data\\*\\Code Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "edge_gpu_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Edge GPU 缓存（全部配置）",
            description: "Edge 所有配置的 GPU 着色器缓存，删除后会被自动重建",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\Edge\\User Data\\*\\GPUCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "edge_service_worker_cachestorage_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Edge Service Worker CacheStorage（全部配置）",
            description: "Edge 所有配置的 Service Worker CacheStorage，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\Edge\\User Data\\*\\Service Worker\\CacheStorage",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "edge_media_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Edge 媒体缓存（全部配置）",
            description: "Edge 所有配置的音视频媒体缓存，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\Edge\\User Data\\*\\Media Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "edge_shader_cache",
            category: JunkCategory::BrowserCache,
            name: "Edge 全局 ShaderCache",
            description: "Edge 跨配置共享的 GPU 着色器缓存，删除后会被自动重建",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\Edge\\User Data\\ShaderCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "edge_crashpad_reports",
            category: JunkCategory::CrashDump,
            name: "Edge 崩溃报告",
            description: "Edge Crashpad 已生成的崩溃转储与元数据，普通用户无需保留",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\Edge\\User Data\\Crashpad\\reports",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "firefox_startup_cache",
            category: JunkCategory::BrowserCache,
            name: "Firefox 启动缓存",
            description: "Firefox 各配置的启动加速缓存，删除后下一次启动会重建",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Mozilla\\Firefox\\Profiles*\\startupCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "firefox_jumplist_cache",
            category: JunkCategory::BrowserCache,
            name: "Firefox 跳转列表缓存",
            description: "Firefox 任务栏跳转列表缩略图缓存，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Mozilla\\Firefox\\Profiles*\\jumpListCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "firefox_offline_cache",
            category: JunkCategory::BrowserCache,
            name: "Firefox 离线缓存",
            description: "Firefox 各配置的旧版离线 AppCache，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Mozilla\\Firefox\\Profiles*\\OfflineCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "firefox_thumbnails",
            category: JunkCategory::BrowserCache,
            name: "Firefox 新标签页缩略图",
            description: "Firefox 新标签页缩略图缓存，删除后会被自动重建",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Mozilla\\Firefox\\Profiles*\\thumbnails",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "firefox_crash_reports",
            category: JunkCategory::CrashDump,
            name: "Firefox 崩溃报告",
            description: "Firefox 崩溃报告器记录的崩溃转储与待提交元数据，可安全删除",
            paths: collect_paths([env_path(
                "APPDATA",
                "Mozilla\\Firefox\\Crash Reports",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: true,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "brave_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Brave 缓存（全部配置）",
            description: "Brave 所有用户配置的网页缓存，删除后首次访问会重新下载",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "BraveSoftware\\Brave-Browser\\User Data\\*\\Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "brave_code_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Brave 代码缓存（全部配置）",
            description: "Brave 所有配置编译后的 JavaScript/WebAssembly 缓存，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "BraveSoftware\\Brave-Browser\\User Data\\*\\Code Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "brave_gpu_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Brave GPU 缓存（全部配置）",
            description: "Brave 所有配置的 GPU 着色器缓存，删除后会被自动重建",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "BraveSoftware\\Brave-Browser\\User Data\\*\\GPUCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "opera_cache",
            category: JunkCategory::BrowserCache,
            name: "Opera 缓存",
            description: "Opera Stable 网页缓存，删除后首次访问会重新下载",
            paths: collect_paths([env_path(
                "APPDATA",
                "Opera Software\\Opera Stable\\Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "opera_code_cache",
            category: JunkCategory::BrowserCache,
            name: "Opera 代码缓存",
            description: "Opera Stable 编译后的 JavaScript/WebAssembly 缓存，可安全删除",
            paths: collect_paths([env_path(
                "APPDATA",
                "Opera Software\\Opera Stable\\Code Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "opera_gpu_cache",
            category: JunkCategory::BrowserCache,
            name: "Opera GPU 缓存",
            description: "Opera Stable 的 GPU 着色器缓存，删除后会被自动重建",
            paths: collect_paths([env_path(
                "APPDATA",
                "Opera Software\\Opera Stable\\GPUCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "vivaldi_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Vivaldi 缓存（全部配置）",
            description: "Vivaldi 所有用户配置的网页缓存，删除后首次访问会重新下载",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Vivaldi\\User Data\\*\\Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "vivaldi_code_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Vivaldi 代码缓存（全部配置）",
            description: "Vivaldi 所有配置编译后的 JavaScript/WebAssembly 缓存，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Vivaldi\\User Data\\*\\Code Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "vivaldi_gpu_cache_all_profiles",
            category: JunkCategory::BrowserCache,
            name: "Vivaldi GPU 缓存（全部配置）",
            description: "Vivaldi 所有配置的 GPU 着色器缓存，删除后会被自动重建",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Vivaldi\\User Data\\*\\GPUCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "qihoo_360_cache",
            category: JunkCategory::BrowserCache,
            name: "360 安全浏览器缓存",
            description: "360 安全浏览器（360se6）默认配置的网页缓存，可安全删除",
            paths: collect_paths([env_path(
                "APPDATA",
                "360se6\\User Data\\Default\\Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "tencent_qqbrowser_cache",
            category: JunkCategory::BrowserCache,
            name: "QQ 浏览器缓存",
            description: "QQ 浏览器默认配置的网页缓存，可安全删除",
            paths: collect_paths([env_path(
                "APPDATA",
                "Tencent\\QQBrowser\\User Data\\Default\\Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "ie_inetcache",
            category: JunkCategory::BrowserCache,
            name: "Internet Explorer 临时文件",
            description: "INetCache 中的 IE/旧版 WebView 网页缓存，删除后首次访问会重新下载",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\Windows\\INetCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: true,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "ie_webcache",
            category: JunkCategory::BrowserCache,
            name: "Internet Explorer WebCache",
            description: "WebCache 中含 IE/旧版 WebView 的访问历史，删除会清空浏览记录",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\Windows\\WebCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: true,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "ie_inetcookies",
            category: JunkCategory::BrowserCache,
            name: "Internet Explorer Cookies",
            description: "INetCookies 含 IE/旧版 WebView 的 Cookie，删除会导致相关网站登出",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\Windows\\INetCookies",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: true,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "teams_classic_cache",
            category: JunkCategory::BrowserCache,
            name: "Microsoft Teams（经典版）网页缓存",
            description: "Teams 经典版内嵌 Electron 的网页缓存，可安全删除",
            paths: collect_paths([env_path("APPDATA", "Microsoft\\Teams\\Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "teams_classic_code_cache",
            category: JunkCategory::BrowserCache,
            name: "Microsoft Teams（经典版）代码缓存",
            description: "Teams 经典版编译后的 JavaScript/WebAssembly 缓存，可安全删除",
            paths: collect_paths([env_path("APPDATA", "Microsoft\\Teams\\Code Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "teams_classic_gpu_cache",
            category: JunkCategory::BrowserCache,
            name: "Microsoft Teams（经典版）GPU 缓存",
            description: "Teams 经典版的 GPU 着色器缓存，删除后会被自动重建",
            paths: collect_paths([env_path("APPDATA", "Microsoft\\Teams\\GPUCache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "teams_classic_blob_storage",
            category: JunkCategory::BrowserCache,
            name: "Microsoft Teams（经典版）Blob 存储缓存",
            description: "Teams 经典版的 blob_storage 临时数据，可安全删除",
            paths: collect_paths([env_path("APPDATA", "Microsoft\\Teams\\blob_storage")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "teams_classic_service_worker",
            category: JunkCategory::BrowserCache,
            name: "Microsoft Teams（经典版）Service Worker 缓存",
            description: "Teams 经典版的 Service Worker CacheStorage，可安全删除",
            paths: collect_paths([env_path(
                "APPDATA",
                "Microsoft\\Teams\\Service Worker\\CacheStorage",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "teams_new_local_cache",
            category: JunkCategory::BrowserCache,
            name: "Microsoft Teams（新版）本地缓存",
            description: "新版 Teams（UWP）LocalCache 目录中的临时缓存，可安全删除",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Packages\\MSTeams_8wekyb3d8bbwe\\LocalCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: true,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "slack_cache",
            category: JunkCategory::BrowserCache,
            name: "Slack 网页缓存",
            description: "Slack 桌面端内嵌 Electron 的网页缓存，可安全删除",
            paths: collect_paths([env_path("APPDATA", "Slack\\Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "slack_code_cache",
            category: JunkCategory::BrowserCache,
            name: "Slack 代码缓存",
            description: "Slack 桌面端编译后的 JavaScript/WebAssembly 缓存，可安全删除",
            paths: collect_paths([env_path("APPDATA", "Slack\\Code Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "slack_gpu_cache",
            category: JunkCategory::BrowserCache,
            name: "Slack GPU 缓存",
            description: "Slack 桌面端的 GPU 着色器缓存，删除后会被自动重建",
            paths: collect_paths([env_path("APPDATA", "Slack\\GPUCache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "slack_service_worker",
            category: JunkCategory::BrowserCache,
            name: "Slack Service Worker 缓存",
            description: "Slack 桌面端的 Service Worker CacheStorage，可安全删除",
            paths: collect_paths([env_path(
                "APPDATA",
                "Slack\\Service Worker\\CacheStorage",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "discord_cache",
            category: JunkCategory::BrowserCache,
            name: "Discord 网页缓存",
            description: "Discord 桌面端内嵌 Electron 的网页缓存，可安全删除",
            paths: collect_paths([env_path("APPDATA", "discord\\Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "discord_code_cache",
            category: JunkCategory::BrowserCache,
            name: "Discord 代码缓存",
            description: "Discord 桌面端编译后的 JavaScript/WebAssembly 缓存，可安全删除",
            paths: collect_paths([env_path("APPDATA", "discord\\Code Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "discord_gpu_cache",
            category: JunkCategory::BrowserCache,
            name: "Discord GPU 缓存",
            description: "Discord 桌面端的 GPU 着色器缓存，删除后会被自动重建",
            paths: collect_paths([env_path("APPDATA", "discord\\GPUCache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "skype_cache",
            category: JunkCategory::BrowserCache,
            name: "Skype 网页缓存",
            description: "Skype 桌面端内嵌 Electron 的网页缓存，可安全删除",
            paths: collect_paths([env_path(
                "APPDATA",
                "Microsoft\\Skype for Desktop\\Cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "skype_gpu_cache",
            category: JunkCategory::BrowserCache,
            name: "Skype GPU 缓存",
            description: "Skype 桌面端的 GPU 着色器缓存，删除后会被自动重建",
            paths: collect_paths([env_path(
                "APPDATA",
                "Microsoft\\Skype for Desktop\\GPUCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "telegram_cache",
            category: JunkCategory::BrowserCache,
            name: "Telegram Desktop 缓存",
            description: "Telegram Desktop 在 tdata\\user_data\\cache 中保存的媒体缓存，删除后会按需重新下载",
            paths: collect_paths([env_path(
                "APPDATA",
                "Telegram Desktop\\tdata\\user_data\\cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "wechat_cache_only",
            category: JunkCategory::BrowserCache,
            name: "微信公共配置缓存",
            description: "WeChat Files\\All Users\\config\\cache 中的非用户聊天数据缓存，可安全删除",
            paths: collect_paths([env_path(
                "USERPROFILE",
                "Documents\\WeChat Files\\All Users\\config\\cache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "qq_websearch_cache",
            category: JunkCategory::BrowserCache,
            name: "QQ 内嵌网页搜索缓存",
            description: "QQ 客户端 WebSearchCache 中的内嵌网页搜索缓存，不包含聊天记录，可安全删除",
            paths: collect_paths([env_path(
                "APPDATA",
                "Tencent\\QQ\\WebSearchCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "dingtalk_cache",
            category: JunkCategory::AppLogs,
            name: "钉钉滚动日志",
            description: "DingTalk\\rolling_logs 客户端运行日志，仅排错保留，删除不影响登录",
            paths: collect_paths([env_path("APPDATA", "DingTalk\\rolling_logs")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
        },
    );

    rules
}
