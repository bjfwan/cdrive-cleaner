use super::{JunkCategory, JunkRiskLevel, JunkRule};
use super::{collect_paths, env_path, push_rule_if_paths};

pub(super) fn extra_rules() -> Vec<JunkRule> {
    let mut rules: Vec<JunkRule> = Vec::new();

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "vscode_cache",
            category: JunkCategory::BrowserCache,
            name: "VSCode 缓存",
            description: "VSCode 网络与资源缓存，删除后会自动重建",
            paths: collect_paths([env_path("APPDATA", "Code\\Cache")]),
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
            id: "vscode_cached_data",
            category: JunkCategory::BrowserCache,
            name: "VSCode 启动缓存",
            description: "VSCode 预编译启动缓存（CachedData），删除后下次启动会重建",
            paths: collect_paths([env_path("APPDATA", "Code\\CachedData")]),
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
            id: "vscode_cached_extensions",
            category: JunkCategory::BrowserCache,
            name: "VSCode 扩展元数据缓存",
            description: "VSCode CachedExtensions，扩展元数据缓存，可安全删除",
            paths: collect_paths([env_path("APPDATA", "Code\\CachedExtensions")]),
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
            id: "vscode_cached_extension_vsixs",
            category: JunkCategory::BrowserCache,
            name: "VSCode 扩展安装包缓存",
            description: "VSCode 已下载的扩展 VSIX 安装包缓存，可安全删除",
            paths: collect_paths([env_path("APPDATA", "Code\\CachedExtensionVSIXs")]),
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
            id: "vscode_gpu_cache",
            category: JunkCategory::BrowserCache,
            name: "VSCode GPU 缓存",
            description: "VSCode 编辑器 GPU 着色器缓存，删除后会自动重建",
            paths: collect_paths([env_path("APPDATA", "Code\\GPUCache")]),
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
            id: "vscode_service_worker",
            category: JunkCategory::BrowserCache,
            name: "VSCode Service Worker 缓存",
            description: "VSCode 内嵌 Service Worker 缓存，可安全删除",
            paths: collect_paths([env_path("APPDATA", "Code\\Service Worker\\CacheStorage")]),
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
            id: "vscode_logs",
            category: JunkCategory::AppLogs,
            name: "VSCode 日志",
            description: "VSCode 各窗口与扩展日志，仅排障保留",
            paths: collect_paths([env_path("APPDATA", "Code\\logs")]),
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
            id: "vscode_crashpad",
            category: JunkCategory::CrashDump,
            name: "VSCode 崩溃转储",
            description: "VSCode Crashpad 崩溃转储数据，仅排障保留",
            paths: collect_paths([env_path("APPDATA", "Code\\Crashpad")]),
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
            id: "jetbrains_user_caches",
            category: JunkCategory::ThumbnailCache,
            name: "JetBrains IDE 缓存",
            description: "JetBrains 各 IDE（IntelliJ/PyCharm/WebStorm 等）缓存，重启 IDE 后会重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "JetBrains\\*\\caches")]),
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
            id: "jetbrains_user_log",
            category: JunkCategory::AppLogs,
            name: "JetBrains IDE 日志",
            description: "JetBrains 各 IDE 日志，仅排障保留",
            paths: collect_paths([env_path("LOCALAPPDATA", "JetBrains\\*\\log")]),
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
            id: "jetbrains_user_system_caches",
            category: JunkCategory::ThumbnailCache,
            name: "JetBrains 系统级缓存",
            description: "JetBrains 各 IDE system\\caches 子目录，重启 IDE 后会重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "JetBrains\\*\\system\\caches")]),
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
            id: "jetbrains_user_indexes",
            category: JunkCategory::ThumbnailCache,
            name: "JetBrains 索引",
            description: "JetBrains 项目索引数据，删除后重启 IDE 会重新索引（大型项目可能需要数分钟）",
            paths: collect_paths([env_path("LOCALAPPDATA", "JetBrains\\*\\index")]),
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
            id: "visualstudio_componentmodelcache",
            category: JunkCategory::ThumbnailCache,
            name: "Visual Studio MEF 组件缓存",
            description: "Visual Studio ComponentModelCache，启动异常时常清理，重启 VS 后会重建",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\VisualStudio\\*\\ComponentModelCache",
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
            id: "visualstudio_temp",
            category: JunkCategory::SystemTemp,
            name: "Visual Studio 临时目录",
            description: "Visual Studio 各版本 Temp 目录，编译/调试中转文件",
            paths: collect_paths([env_path("LOCALAPPDATA", "Microsoft\\VisualStudio\\*\\Temp")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: true,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "visualstudio_extensions_cache",
            category: JunkCategory::AppLogs,
            name: "Visual Studio 扩展日志",
            description: "Visual Studio 扩展安装与运行日志，仅排障保留",
            paths: collect_paths([env_path(
                "LOCALAPPDATA",
                "Microsoft\\VisualStudio\\*\\Extensions\\Logs",
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
            id: "npm_cache",
            category: JunkCategory::BrowserCache,
            name: "npm 全局缓存",
            description: "npm 全局下载缓存，删除后下次构建需重新下载（企业网络敏感，请按需勾选）",
            paths: collect_paths([env_path("APPDATA", "npm-cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "pnpm_cache",
            category: JunkCategory::BrowserCache,
            name: "pnpm 缓存",
            description: "pnpm 全局存储缓存，删除后下次安装需重新下载（企业网络敏感）",
            paths: collect_paths([env_path("LOCALAPPDATA", "pnpm-cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "yarn_cache",
            category: JunkCategory::BrowserCache,
            name: "Yarn 缓存",
            description: "Yarn 全局下载缓存，删除后下次构建需重新下载（企业网络敏感）",
            paths: collect_paths([env_path("LOCALAPPDATA", "Yarn\\Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "pip_cache",
            category: JunkCategory::BrowserCache,
            name: "pip wheel 缓存",
            description: "pip 已下载 wheel 缓存，删除后下次 pip install 需重新下载（企业网络敏感）",
            paths: collect_paths([env_path("LOCALAPPDATA", "pip\\Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "conda_pkgs_cache",
            category: JunkCategory::BrowserCache,
            name: "Conda 包缓存",
            description: "Conda 已下载包缓存，删除后再创建环境需重新下载（企业网络敏感）",
            paths: collect_paths([env_path("USERPROFILE", ".conda\\pkgs")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "cargo_registry_cache",
            category: JunkCategory::BrowserCache,
            name: "Cargo 注册表缓存",
            description: "Cargo 注册表 .crate 包缓存，删除后下次构建需重新下载（不动 src/index）",
            paths: collect_paths([env_path("USERPROFILE", ".cargo\\registry\\cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "gradle_daemon_logs",
            category: JunkCategory::AppLogs,
            name: "Gradle 守护进程日志",
            description: "Gradle daemon 输出日志，仅排障保留（不会动 Gradle caches）",
            paths: collect_paths([env_path("USERPROFILE", ".gradle\\daemon")]),
            patterns: vec!["*.out.log", "*.log"],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: true,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "nuget_v3_cache",
            category: JunkCategory::BrowserCache,
            name: "NuGet v3 元数据缓存",
            description: "NuGet v3-cache 元数据缓存，删除后会自动重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "NuGet\\v3-cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "nuget_http_cache",
            category: JunkCategory::BrowserCache,
            name: "NuGet HTTP 包缓存",
            description: "NuGet HTTP 包缓存，删除后下次还原需重新下载（企业网络敏感）",
            paths: collect_paths([env_path("LOCALAPPDATA", "NuGet\\Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "office_document_cache",
            category: JunkCategory::BrowserCache,
            name: "Office 文档缓存",
            description: "Office 文档缓存，极少数情况下含未上传修改，确认无遗漏后再删",
            paths: collect_paths([env_path("LOCALAPPDATA", "Microsoft\\Office\\*\\OfficeFileCache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "onedrive_setup_logs",
            category: JunkCategory::AppLogs,
            name: "OneDrive 安装日志",
            description: "OneDrive 安装与升级日志，仅排障保留（不动 OneDrive 同步文件）",
            paths: collect_paths([env_path("LOCALAPPDATA", "Microsoft\\OneDrive\\setup\\logs")]),
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
            id: "adobe_common_media_cache_files",
            category: JunkCategory::SystemTemp,
            name: "Adobe Premiere Media Cache",
            description: "Adobe Premiere/After Effects 媒体峰值与索引文件，重新打开素材会重建",
            paths: collect_paths([env_path("APPDATA", "Adobe\\Common\\Media Cache Files")]),
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
            id: "adobe_common_media_cache",
            category: JunkCategory::SystemTemp,
            name: "Adobe 媒体缓存数据库",
            description: "Adobe 媒体缓存数据库，重新打开项目会重建",
            paths: collect_paths([env_path("APPDATA", "Adobe\\Common\\Media Cache")]),
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
            id: "adobe_common_peak_files",
            category: JunkCategory::SystemTemp,
            name: "Adobe 音频峰值缓存",
            description: "Adobe Peak Files 音频峰值缓存，重新打开素材会重建",
            paths: collect_paths([env_path("APPDATA", "Adobe\\Common\\Peak Files")]),
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
            id: "adobe_bridge_cache",
            category: JunkCategory::ThumbnailCache,
            name: "Adobe Bridge 缩略图缓存",
            description: "Adobe Bridge 缩略图与预览缓存，删除后浏览文件夹会重建",
            paths: collect_paths([env_path("APPDATA", "Adobe\\Bridge*\\Cache")]),
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
            id: "adobe_camera_raw_cache",
            category: JunkCategory::ThumbnailCache,
            name: "Adobe Camera Raw 预览缓存",
            description: "Adobe Camera Raw 预览缓存，删除后会重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "Adobe\\Camera Raw\\Cache")]),
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
            id: "d3d_shader_cache",
            category: JunkCategory::ThumbnailCache,
            name: "DirectX 着色器缓存",
            description: "DirectX D3DSCache 着色器缓存，删除后游戏首次启动会卡几秒重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "D3DSCache")]),
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
            id: "nvidia_dx_cache",
            category: JunkCategory::ThumbnailCache,
            name: "NVIDIA DirectX 着色器缓存",
            description: "NVIDIA DXCache，DirectX 着色器缓存，删除后会自动重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "NVIDIA\\DXCache")]),
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
            id: "nvidia_gl_cache",
            category: JunkCategory::ThumbnailCache,
            name: "NVIDIA OpenGL 着色器缓存",
            description: "NVIDIA GLCache，OpenGL 着色器缓存，删除后会自动重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "NVIDIA\\GLCache")]),
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
            id: "nvidia_compute_cache",
            category: JunkCategory::ThumbnailCache,
            name: "NVIDIA 计算着色器缓存",
            description: "NVIDIA ComputeCache，CUDA/计算着色器缓存，删除后会自动重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "NVIDIA\\ComputeCache")]),
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
            id: "amd_dx_cache",
            category: JunkCategory::ThumbnailCache,
            name: "AMD DirectX 着色器缓存",
            description: "AMD DxCache，DirectX 着色器缓存，删除后会自动重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "AMD\\DxCache")]),
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
            id: "amd_gl_cache",
            category: JunkCategory::ThumbnailCache,
            name: "AMD OpenGL 着色器缓存",
            description: "AMD GLCache，OpenGL 着色器缓存，删除后会自动重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "AMD\\GLCache")]),
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
            id: "steam_html_cache",
            category: JunkCategory::BrowserCache,
            name: "Steam 商店 webview 缓存",
            description: "Steam 客户端商店 webview 缓存，可安全删除",
            paths: collect_paths([env_path("LOCALAPPDATA", "Steam\\htmlcache")]),
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
            id: "epic_webcache",
            category: JunkCategory::BrowserCache,
            name: "Epic Games Launcher webview 缓存",
            description: "Epic Games Launcher webview 缓存，可安全删除",
            paths: collect_paths([env_path("LOCALAPPDATA", "EpicGamesLauncher\\Saved\\webcache")]),
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
            id: "epic_logs",
            category: JunkCategory::AppLogs,
            name: "Epic Games Launcher 日志",
            description: "Epic Games Launcher 客户端日志，仅排障保留",
            paths: collect_paths([env_path("LOCALAPPDATA", "EpicGamesLauncher\\Saved\\Logs")]),
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
            id: "battlenet_cache",
            category: JunkCategory::BrowserCache,
            name: "战网客户端 webview 缓存",
            description: "Battle.net 客户端 webview 缓存，可安全删除",
            paths: collect_paths([env_path("APPDATA", "Battle.net\\Cache")]),
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
            id: "battlenet_logs",
            category: JunkCategory::AppLogs,
            name: "战网客户端日志",
            description: "Battle.net 客户端日志（位于 ProgramData），仅排障保留",
            paths: collect_paths([env_path("PROGRAMDATA", "Battle.net\\Logs")]),
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
            id: "riot_logs",
            category: JunkCategory::AppLogs,
            name: "Riot 客户端日志",
            description: "Riot Client 日志（League of Legends/VALORANT 等），仅排障保留",
            paths: collect_paths([env_path("LOCALAPPDATA", "Riot Games\\Riot Client\\Logs")]),
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
            id: "ea_app_cache",
            category: JunkCategory::BrowserCache,
            name: "EA 桌面客户端 webview 缓存",
            description: "EA Desktop 客户端 webview 缓存，可安全删除",
            paths: collect_paths([env_path("LOCALAPPDATA", "Electronic Arts\\EA Desktop\\Cache")]),
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
            id: "ea_app_logs",
            category: JunkCategory::AppLogs,
            name: "EA 桌面客户端日志",
            description: "EA Desktop 客户端日志，仅排障保留",
            paths: collect_paths([env_path("LOCALAPPDATA", "Electronic Arts\\EA Desktop\\Logs")]),
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
            id: "zoom_cache",
            category: JunkCategory::BrowserCache,
            name: "Zoom 客户端缓存",
            description: "Zoom 客户端 data\\cache 网络缓存，可安全删除",
            paths: collect_paths([env_path("APPDATA", "Zoom\\data\\cache")]),
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
            id: "zoom_logs",
            category: JunkCategory::AppLogs,
            name: "Zoom 客户端日志",
            description: "Zoom 客户端日志，仅排障保留",
            paths: collect_paths([env_path("APPDATA", "Zoom\\logs")]),
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
            id: "spotify_storage",
            category: JunkCategory::BrowserCache,
            name: "Spotify 离线缓存",
            description: "Spotify Storage 离线缓存（含已缓存歌曲），删除后再次播放需重新下载",
            paths: collect_paths([env_path("LOCALAPPDATA", "Spotify\\Storage")]),
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
            id: "spotify_data",
            category: JunkCategory::BrowserCache,
            name: "Spotify 客户端缓存",
            description: "Spotify 客户端 Data 缓存，删除后会重建",
            paths: collect_paths([env_path("LOCALAPPDATA", "Spotify\\Data")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
        },
    );

    rules
}
