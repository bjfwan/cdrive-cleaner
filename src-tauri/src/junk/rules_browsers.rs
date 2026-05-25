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
            max_depth: None,
            why_safe: Some("浏览器加载网页时存的图片、JS、CSS 副本，下次访问会按需重新下载。不会影响你的登录状态、书签、浏览历史、扩展、保存的密码——这些都在另外的文件里，本规则不会动。"),
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
            max_depth: None,
            why_safe: Some("V8 引擎把 JavaScript / WebAssembly 编译过一遍后存下来的字节码副本，下次访问网站会自动重新编译。最坏情况是首次打开网页慢 0.5-2 秒。不会影响账号、书签、历史。"),
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
            max_depth: None,
            why_safe: Some("GPU 着色器编译缓存。删除后浏览器首次加载复杂网页（如 3D、视频、Canvas 动画）可能多花一两百毫秒重新编译着色器，无其他影响。不会动账号、书签或扩展。"),
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
            max_depth: None,
            why_safe: Some("网页 PWA 离线模式保存的图片和脚本副本，下次访问网站会按需重新下载。不会让你登出网站、不会卸载任何已安装的 PWA、不会影响 IndexedDB 里的草稿/邮件等本地数据。"),
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
            max_depth: None,
            why_safe: Some("Service Worker 脚本本身的副本，下次访问注册了 SW 的网站会自动重新下载。不会取消任何 PWA 注册（注册信息在另外的 Database 子目录），不会影响登录态。"),
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
            max_depth: None,
            why_safe: Some("浏览器播放视频时缓存下来的媒体片段，下次播放同一视频会重新下载（视频流可能短暂卡顿一两秒）。不会影响登录、收藏、历史。"),
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
            max_depth: None,
            why_safe: Some("Chrome 早期版本的 AppCache 离线缓存机制，现代网站早就改用 Service Worker，这里通常是空目录或废弃数据。删除一般什么都感觉不到。"),
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
            max_depth: None,
            why_safe: Some("Chrome 多个用户配置共享的 GPU 着色器编译缓存。删除后首次渲染复杂页面会重建着色器，仅多几十到几百毫秒。不会影响登录、书签、历史或扩展。"),
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
            max_depth: None,
            why_safe: Some("Chrome 崩溃时生成的内存快照，主要给 Google 工程师排错。如果你当时没勾选上报，这些文件留在本地也没人会看。删除不影响 Chrome 任何浏览功能。"),
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
            max_depth: None,
            why_safe: Some("Edge 加载网页时缓存的图片、JS、CSS 副本，下次访问会按需重新下载。不会影响你的微软账号登录、收藏夹、浏览历史、保存的密码或已安装的扩展。"),
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
            max_depth: None,
            why_safe: Some("V8 引擎编译过 JavaScript / WebAssembly 后存下的字节码副本，下次访问网站会自动重新编译。最坏情况是首次打开网页慢 0.5-2 秒。不会影响登录、收藏、历史。"),
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
            max_depth: None,
            why_safe: Some("GPU 着色器编译缓存。删除后 Edge 首次加载复杂网页可能多花一两百毫秒重新编译，无其他影响。不会动账号、收藏夹、扩展。"),
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
            max_depth: None,
            why_safe: Some("网页 PWA 离线模式保存的图片和脚本副本，下次访问网站会按需重新下载。不会让你登出网站、不会卸载任何已安装的 PWA、不会影响 IndexedDB 里的本地数据。"),
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
            max_depth: None,
            why_safe: Some("Edge 播放视频时缓存的媒体片段，下次播放同一视频要重新下载（短暂卡顿一两秒）。不会影响登录、收藏、历史。"),
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
            max_depth: None,
            why_safe: Some("Edge 多个用户配置共享的 GPU 着色器编译缓存。删除后首次渲染复杂页面会重建，仅多几十到几百毫秒。不会影响微软账号、收藏夹或扩展。"),
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
            max_depth: None,
            why_safe: Some("Edge 崩溃时生成的内存快照，主要给微软工程师排错。已经过去的崩溃如果你没上报，留在本地也没人会看。删除不影响 Edge 任何浏览功能。"),
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
                "Mozilla\\Firefox\\Profiles\\*\\startupCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Firefox 启动时为加速 XPCOM 组件和插件加载而缓存的二进制索引。删除后首次启动 Firefox 会慢 1-2 秒，仅一次。不会影响账号登录、扩展、书签或浏览历史。"),
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
                "Mozilla\\Firefox\\Profiles\\*\\jumpListCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("任务栏右键 Firefox 时弹出的最近访问站点小图标缓存。删除后下次右键任务栏可能短暂没图标，Firefox 自动重建。不会影响书签、历史或扩展。"),
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
                "Mozilla\\Firefox\\Profiles\\*\\OfflineCache",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Firefox 旧版 AppCache 离线应用缓存机制（W3C 已废弃），现代网站基本不再使用。删除一般什么感觉都没有。不会影响登录、书签或扩展。"),
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
                "Mozilla\\Firefox\\Profiles\\*\\thumbnails",
            )]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Firefox 新标签页里常访问网站那些方块缩略图。删除后下次打开新标签页会先显示几秒空白占位符，Firefox 会重新截图生成。不会影响书签或历史本身。"),
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
            max_depth: None,
            why_safe: Some("Firefox 崩溃时生成的小型转储文件和待提交元数据，主要给 Mozilla 工程师排错。如果你没在反馈崩溃问题，这些文件就再无用处。删除不影响 Firefox 任何浏览功能。"),
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
            max_depth: None,
            why_safe: Some("Brave 加载网页时缓存的图片、JS、CSS 副本，下次访问会按需重新下载。不会影响你的 Brave 钱包、奖励 BAT、登录态、书签、历史或扩展。"),
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
            max_depth: None,
            why_safe: Some("V8 编译过的 JavaScript / WebAssembly 字节码副本，下次访问网站会自动重新编译。最坏首次打开网页慢 0.5-2 秒。不会触及 Brave 钱包或任何账号数据。"),
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
            max_depth: None,
            why_safe: Some("GPU 着色器编译缓存。删除后 Brave 首次渲染复杂页面会多花一两百毫秒重建着色器，无其他影响。不会动钱包、书签或扩展。"),
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
            max_depth: None,
            why_safe: Some("Opera 加载网页时存的图片、JS、CSS 副本，下次访问会按需重新下载。不会影响你的 Opera 账号、内置 VPN 设置、收藏夹、扩展或浏览历史。"),
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
            max_depth: None,
            why_safe: Some("V8 编译过的 JavaScript / WebAssembly 字节码副本，下次访问网站会自动重编译。最坏首次打开网页慢 0.5-2 秒。不会影响登录、收藏或扩展。"),
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
            max_depth: None,
            why_safe: Some("GPU 着色器编译缓存。删除后 Opera 首次渲染复杂页面会多花一两百毫秒，无其他影响。不会动账号或扩展。"),
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
            max_depth: None,
            why_safe: Some("Vivaldi 加载网页时存的图片、JS、CSS 副本，下次访问会按需重新下载。不会影响你的同步账号、标签会话、笔记、书签或扩展。"),
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
            max_depth: None,
            why_safe: Some("V8 编译过的 JavaScript / WebAssembly 字节码副本，下次访问网站会自动重编译。最坏首次打开网页慢 0.5-2 秒。不会影响 Vivaldi 的笔记或会话。"),
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
            max_depth: None,
            why_safe: Some("GPU 着色器编译缓存。删除后 Vivaldi 首次渲染复杂页面会多花一两百毫秒，无其他影响。不会触及笔记或扩展。"),
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
            max_depth: None,
            why_safe: Some("360 安全浏览器默认配置的网页缓存，下次访问网站会按需重新下载。不会影响 360 账号登录、收藏夹、浏览历史或已装扩展。"),
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
            max_depth: None,
            why_safe: Some("QQ 浏览器默认配置的网页缓存，下次访问网站会重新下载。不会影响 QQ 账号、书签、浏览历史，也不会清掉登录态。"),
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
            max_depth: None,
            why_safe: Some("IE 和系统旧版 WebView/MSHTML 控件的网页缓存。如今 Edge / Chrome 都不读这里，删除几乎无感。如果偶尔还有老旧网银插件依赖 IE，首次重新访问可能略慢。"),
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
            max_depth: None,
            why_safe: Some("IE 和旧版 WebView 的浏览历史数据库。新版 Chrome / Edge 不读这里。若你不再用 IE，删除无感；但若你还在用某些公司内网工具读这个库，删后这些工具读不到上次访问记录。日常不勾。"),
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
            max_depth: None,
            why_safe: Some("IE 和旧版 WebView 控件的 Cookie。如果你还在用 IE 或某些老旧网银控件，删了要重登。新版 Chrome / Edge 的登录态不在这里。日常不勾。"),
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
            max_depth: None,
            why_safe: Some("经典版 Teams（Electron 内核）的网页层缓存。下次启动 Teams 头像和图片会按需重下。不会让你登出 Teams、不会丢任何会议历史或聊天记录。"),
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
            max_depth: None,
            why_safe: Some("Teams 内嵌 V8 引擎编译过的字节码副本，下次启动会自动重新编译。最坏 Teams 启动慢 1-2 秒一次。不会让你登出，也不会丢聊天。"),
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
            max_depth: None,
            why_safe: Some("Teams 内嵌 Chromium 的 GPU 着色器缓存。删除后下次启动会重建，仅几十毫秒到几百毫秒影响。不会动账号或聊天数据。"),
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
            max_depth: None,
            why_safe: Some("Teams 内嵌 Chromium 临时存放 JS Blob 对象的目录（如发送中的图片分块）。Teams 关闭后这里通常已无用。不会影响登录或聊天记录。"),
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
            max_depth: None,
            why_safe: Some("Teams 网页 Service Worker 缓存的资源副本（脚本、图标等）。下次启动 Teams 会按需重新下载，不会让你登出、不会丢任何聊天或会议。"),
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
            max_depth: None,
            why_safe: Some("新版 Teams（应用商店版）UWP 沙箱中的临时缓存。下次启动 Teams 会按需重建，头像与缩略图可能要重新加载。不会让你登出，也不会丢聊天或组织数据。"),
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
            max_depth: None,
            why_safe: Some("Slack 桌面端（Electron 内核）的网页层缓存。下次启动会按需重新拉取头像、表情图等。不会让你登出工作区、不会丢任何历史消息或频道列表。"),
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
            max_depth: None,
            why_safe: Some("Slack 内嵌 V8 编译过的字节码副本，下次启动会自动重新编译。最坏 Slack 启动慢 1-2 秒一次。不会让你登出，也不会丢消息。"),
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
            max_depth: None,
            why_safe: Some("Slack 内嵌 Chromium 的 GPU 着色器缓存。删除后 Slack 首次渲染消息列表可能多几十毫秒，无其他影响。不会动账号或工作区。"),
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
            max_depth: None,
            why_safe: Some("Slack 网页 Service Worker 缓存的图片、表情、Bot 头像副本。下次启动会按需重下。不会让你登出、不会丢工作区或消息历史。"),
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
            max_depth: None,
            why_safe: Some("Discord 桌面端的网页层缓存（用户头像、表情、附件图片副本）。下次启动会按需重下。不会让你登出账号、不会丢服务器、不会丢消息记录。"),
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
            max_depth: None,
            why_safe: Some("Discord 内嵌 V8 编译过的字节码副本，下次启动会自动重新编译。最坏 Discord 启动慢 1-2 秒一次。不会让你登出或丢任何数据。"),
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
            max_depth: None,
            why_safe: Some("Discord 内嵌 Chromium 的 GPU 着色器缓存。删除后首次启动可能多几十毫秒。不会动账号、服务器列表或好友列表。"),
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
            max_depth: None,
            why_safe: Some("Skype 桌面端的网页层缓存。下次启动会按需重新拉取头像、表情等。不会让你登出 Skype，也不会丢任何聊天或通话记录。"),
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
            max_depth: None,
            why_safe: Some("Skype 内嵌 Chromium 的 GPU 着色器缓存。删除后首次启动可能多几十毫秒，无其他影响。不会动账号或聊天记录。"),
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
            max_depth: None,
            why_safe: Some("Telegram 缓存的图片、贴纸、视频缩略图副本。下次查看聊天时会按需重新下载。不会让你登出账号（登录态在 tdata 根目录，本规则不动）、不会丢任何消息或会话密钥。"),
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
            max_depth: None,
            why_safe: Some("微信公共配置目录下的程序缓存（小程序运行残留、UI 资源等），路径上不含具体用户的聊天数据库。不会让你重新扫码登录，不会触及聊天记录或收到的文件。"),
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
            max_depth: None,
            why_safe: Some("QQ 客户端内嵌网页（搜一搜、新闻、广告位等）的临时缓存。不在聊天记录路径上，不会让你重登 QQ，也不会丢任何接收过的文件、图片或表情。"),
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
            max_depth: None,
            why_safe: Some("钉钉客户端的滚动调试日志，只有在和钉钉客服或公司 IT 排查问题时才会用到。不会丢账号、聊天、会议或文件。日常不勾；如果当前正在排查钉钉问题，请保留这些日志。"),
        },
    );

    rules
}
