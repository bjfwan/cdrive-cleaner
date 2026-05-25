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
            max_depth: None,
            why_safe: Some("VSCode 从扩展市场和远程仓库下载内容时存的网络副本。不会影响你的扩展、设置、打开过的文件、调试配置——这些都在别处。删了下次打开应用市场或加载远程内容时会重新拉。"),
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
            max_depth: None,
            why_safe: Some("VSCode 为加速启动预编译的 JavaScript 字节码缓存。删除后下一次启动会慢 1-2 秒重建，之后就和原来一样。不会影响你的工作区、扩展、设置、键位绑定。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "vscode_cached_extensions",
            category: JunkCategory::BrowserCache,
            name: "VSCode 扩展元数据缓存",
            description: "VSCode CachedExtensions，扩展元数据缓存",
            paths: collect_paths([env_path("APPDATA", "Code\\CachedExtensions")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("VSCode 给已安装扩展生成的元数据索引（不是扩展本体）。已安装的扩展位于 .vscode\\extensions，不会被动。删了下次启动 VSCode 会自动重建索引，过程不可感知。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "vscode_cached_extension_vsixs",
            category: JunkCategory::BrowserCache,
            name: "VSCode 扩展安装包缓存",
            description: "VSCode 已下载的扩展 VSIX 安装包缓存",
            paths: collect_paths([env_path("APPDATA", "Code\\CachedExtensionVSIXs")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("VSCode 安装扩展时下载的 .vsix 压缩包副本，扩展安装完成后这些包就没用了。不会影响已安装的扩展本体（在另一个目录）。删了下次要安装新扩展时正常下载，不变慢。"),
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
            max_depth: None,
            why_safe: Some("VSCode 内嵌 Chromium 给界面渲染存的着色器副本。删了下次启动时第一次绘制窗口会慢一瞬间，之后无感。不会影响你的代码、扩展、主题、字体设置。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "vscode_service_worker",
            category: JunkCategory::BrowserCache,
            name: "VSCode Service Worker 缓存",
            description: "VSCode 内嵌 Service Worker 缓存",
            paths: collect_paths([env_path("APPDATA", "Code\\Service Worker\\CacheStorage")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("VSCode 内嵌 Chromium Service Worker 离线缓存，主要给 Web 视图（Markdown 预览等）用。不会影响你的扩展数据、设置、工作区。删了下次打开 Web 视图重新拉资源，影响不可感知。"),
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
            max_depth: None,
            why_safe: Some("VSCode 主进程、扩展宿主、各窗口的运行日志。不会影响你的代码、设置、扩展。但如果你正在排查 VSCode 启动慢、扩展崩溃等问题，这里是开发者第一眼要看的——排障期间不要清。日常体积通常几十 MB。"),
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
            max_depth: None,
            why_safe: Some("VSCode 崩溃瞬间的内存快照，主要给微软排错用。已经发生过的崩溃用不上这些转储，留着只占空间。不会影响 VSCode 的任何功能。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "jetbrains_user_caches",
            category: JunkCategory::ThumbnailCache,
            name: "JetBrains IDE 缓存",
            description: "JetBrains 各 IDE（IntelliJ/PyCharm/WebStorm 等）缓存",
            paths: collect_paths([env_path("LOCALAPPDATA", "JetBrains\\*\\caches")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("JetBrains 各 IDE 给打开过的项目生成的解析缓存（类引用、外部库扫描结果）。不会动你的设置、插件、Local History、项目源码。删了下次打开大项目会有几分钟的索引重建，仅一次。"),
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
            max_depth: None,
            why_safe: Some("JetBrains IDE 主进程和插件的运行日志，不会影响代码、设置、插件。但如果你正在排查 IDE 卡死、内存暴涨、插件冲突，开发者支持会要你提供这里的日志——排障期间不要清。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "jetbrains_user_system_caches",
            category: JunkCategory::ThumbnailCache,
            name: "JetBrains 系统级缓存",
            description: "JetBrains 各 IDE system\\caches 子目录",
            paths: collect_paths([env_path("LOCALAPPDATA", "JetBrains\\*\\system\\caches")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("JetBrains IDE 内部解析缓存（项目结构、外部依赖摘要），是性能加速副本。不会影响你的项目源码、设置、Local History、插件、键位映射。删了下次打开大项目可能要等几分钟重新索引，之后照常。"),
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
            max_depth: None,
            why_safe: Some("JetBrains IDE 为代码补全和跳转生成的索引数据。删了下次打开大型项目（百万行级）可能要 3-10 分钟重新索引，期间补全和跳转会变慢。不会丢任何代码、设置、Local History。"),
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
            max_depth: None,
            why_safe: Some("Visual Studio 给已加载扩展生成的 MEF 组合缓存。VS 启动卡住时官方文档第一步就是删这个目录。删了下次启动 VS 会重建，多 5-10 秒启动时间。不会影响你的工程、扩展、设置。"),
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
            max_depth: None,
            why_safe: Some("Visual Studio 编译、设计器、调试过程的中转文件。不会影响你的项目源码、解决方案文件、调试设置。建议关闭 VS 后再清；如果当前 VS 正在跑构建或调试，先停下来再清，避免影响进行中的任务。"),
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
            max_depth: None,
            why_safe: Some("Visual Studio 各扩展的安装与运行日志（不是扩展本体）。不会影响已装好的扩展。但如果你正在排查某个扩展无法加载或频繁报错，删了就丢失了证据——排障期间不要勾。"),
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
            max_depth: None,
            why_safe: Some("npm 历史下载过的包的本地副本，给 `npm install` 加速用。不会影响 node_modules（在项目里）或全局安装的 CLI 工具。删了下次 npm install 会全部从源重新下载，公司网络受限或带宽差时单个大项目要等十几分钟，所以默认不勾。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "pnpm_cache",
            category: JunkCategory::BrowserCache,
            name: "pnpm 元数据缓存",
            description: "pnpm 的 HTTP 元数据缓存（非 store），删除后下次安装会重新拉元数据",
            paths: collect_paths([env_path("LOCALAPPDATA", "pnpm-cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("pnpm 向 registry 询问包元数据时的 HTTP 响应缓存（不是 pnpm 的 content-addressable store，那个在 pnpm\\store 别处）。删了下次 pnpm install 重新拉元数据，速度受网络影响——所以默认不勾。不会影响已安装项目的 node_modules 软链接。"),
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
            max_depth: None,
            why_safe: Some("Yarn 把下载过的 npm 包按版本保存的本地副本。不会影响 node_modules（在项目里）或 yarn.lock。删了下次 yarn install 要全部从源重新下，公司网络环境慢时大项目要等十几分钟，所以默认不勾。"),
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
            max_depth: None,
            why_safe: Some("pip 历史下载过的 wheel 文件副本。不会影响已经装好的 Python 包（在 site-packages）或虚拟环境。删了下次创建新环境或装新包时要从 PyPI 重新下载，包含大依赖（torch、numpy）时尤其慢——所以默认不勾。"),
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
            max_depth: None,
            why_safe: Some("Conda 历史下载过的包副本，给新建 env 时复用。不会影响已经创建好的 conda 环境（在 envs/ 下），那些还能正常用。删了下次创建新 env 要从 channels 重新下载，包含 cudatoolkit 等大依赖时单次几个 GB——所以默认不勾。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "cargo_registry_cache",
            category: JunkCategory::BrowserCache,
            name: "Cargo 注册表缓存",
            description: "Cargo 注册表 .crate 包缓存（不动 src/index）",
            paths: collect_paths([env_path("USERPROFILE", ".cargo\\registry\\cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Risky,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Cargo 历史从 crates.io 下载过的 .crate 包副本。不会动 registry\\src（解压源码，cargo build 会用）也不会动 registry\\index（包索引）。删了下次构建若需要重新解压，会从源重拉 .crate——网络受限时建议不勾。"),
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
            max_depth: None,
            why_safe: Some("Gradle daemon 后台构建进程的输出日志，仅匹配 *.log 不动 daemon 进程自身。不会动 ~/.gradle/caches（构建缓存）或 wrapper（版本管理）。但如果当前正在排查构建挂死、OOM，这里是关键证据，排障期间不要勾。"),
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
            max_depth: None,
            why_safe: Some("NuGet 向源服务器询问包元数据（版本号、依赖图）的 HTTP 响应缓存，不是包本体。不会影响项目里 packages 或全局 packages 文件夹。删了下次 nuget restore 时重新拉元数据，速度受网络影响——所以默认不勾。"),
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
            max_depth: None,
            why_safe: Some("NuGet 历史下载过的包文件副本。不会影响 .NET 项目里 packages 文件夹的已还原依赖。删了下次 nuget restore 要从源重新下，公司私有源带宽小或大型解决方案时较慢——所以默认不勾。"),
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
            max_depth: None,
            why_safe: Some("Office 上传中心（Upload Center）暂存的 OneDrive/SharePoint 文档副本。极少数情况下，如果你刚改完文档但还没传上云，本地修改就在这里——删了等于丢未上传的修改，所以默认不勾。建议确认上传中心无待传文件再勾。"),
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
            max_depth: None,
            why_safe: Some("OneDrive 客户端安装、升级、卸载过程的诊断日志。绝不会动 OneDrive 同步文件夹里的文档、照片。但如果你 OneDrive 正卡在升级、登录失败、同步异常，这里是微软支持要的第一手证据——排障期间不要勾。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "adobe_common_media_cache_files",
            category: JunkCategory::SystemTemp,
            name: "Adobe Premiere Media Cache",
            description: "Adobe Premiere/After Effects 媒体峰值与索引文件",
            paths: collect_paths([env_path("APPDATA", "Adobe\\Common\\Media Cache Files")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Premiere/After Effects 给视频、音频素材生成的音频峰值（波形）和索引副本，用于加速预览。不会动你的工程文件（.prproj/.aep）或原始素材。下次在 Pr/Ae 打开素材会重新生成索引——长素材重新生成要等几分钟，但仅一次。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "adobe_common_media_cache",
            category: JunkCategory::SystemTemp,
            name: "Adobe 媒体缓存数据库",
            description: "Adobe 媒体缓存数据库",
            paths: collect_paths([env_path("APPDATA", "Adobe\\Common\\Media Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Adobe 媒体缓存的数据库索引，记录哪些素材生成过缓存。不会动工程文件或原始素材。删了下次打开工程时 Adobe 会重新建索引——大型工程要等几分钟，但仅一次。常和 Media Cache Files 一起清，工程打开会快很多。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "adobe_common_peak_files",
            category: JunkCategory::SystemTemp,
            name: "Adobe 音频峰值缓存",
            description: "Adobe Peak Files 音频峰值缓存",
            paths: collect_paths([env_path("APPDATA", "Adobe\\Common\\Peak Files")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Adobe Audition / Premiere 给音频素材生成的波形可视化缓存。不会动工程文件或原始音频。删了下次在 Au/Pr 打开音频文件会重新算波形——长音频几秒到几十秒，但仅一次。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "adobe_bridge_cache",
            category: JunkCategory::ThumbnailCache,
            name: "Adobe Bridge 缩略图缓存",
            description: "Adobe Bridge 缩略图与预览缓存",
            paths: collect_paths([env_path("APPDATA", "Adobe\\Bridge*\\Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Adobe Bridge 浏览图片/视频时生成的缩略图和预览图。不会动你的原始素材或元数据（XMP）。删了下次在 Bridge 浏览文件夹时会重新生成——千张图的目录要等十几秒，但仅一次，之后照常。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "adobe_camera_raw_cache",
            category: JunkCategory::ThumbnailCache,
            name: "Adobe Camera Raw 预览缓存",
            description: "Adobe Camera Raw 预览缓存",
            paths: collect_paths([env_path("LOCALAPPDATA", "Adobe\\Camera Raw\\Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Camera Raw 给 RAW 文件生成的预览图缓存，加速翻阅。不会动 RAW 原片或 .xmp 调色侧文件（你的调色参数都在那里）。删了下次在 Camera Raw 打开 RAW 会重新解码生成预览，单张 1-2 秒，仅一次。"),
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
            max_depth: None,
            why_safe: Some("Windows 给 DirectX 程序（多数 3D 游戏、显卡加速应用）编译的着色器副本。不会影响游戏存档、设置、Mod。删了下次启动相关游戏时第一次进图会卡 5-10 秒重新编译着色器，仅一次。"),
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
            max_depth: None,
            why_safe: Some("NVIDIA 驱动给 DirectX 游戏编译的 GPU 着色器副本。不会影响游戏存档、设置或 GeForce Experience 配置。驱动升级后这里很容易堆积过时的缓存，删了下次进游戏时第一关卡几秒重建，之后照常。"),
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
            max_depth: None,
            why_safe: Some("NVIDIA 驱动给 OpenGL 程序（部分老游戏、Blender、3D 建模软件）编译的着色器副本。不会影响游戏存档、3D 建模工程文件。删了下次启动 OpenGL 程序时第一次渲染会卡几秒重建，仅一次。"),
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
            max_depth: None,
            why_safe: Some("NVIDIA 驱动给 CUDA 程序（深度学习、视频渲染、科学计算）编译的 PTX 二进制缓存。不会动你的模型权重、数据集、Python 包。删了下次跑 PyTorch/TensorFlow 等程序时第一次启动会多几秒 JIT 编译，仅一次。"),
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
            max_depth: None,
            why_safe: Some("AMD 显卡驱动给 DirectX 游戏编译的着色器副本。不会影响游戏存档、Radeon 软件配置、显示器配置。驱动升级后旧缓存就过期了，删了下次进游戏第一次场景加载卡几秒重建，仅一次。"),
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
            max_depth: None,
            why_safe: Some("AMD 显卡驱动给 OpenGL 程序编译的着色器副本。不会动 Blender、CAD 等 3D 软件的工程文件。删了下次启动 OpenGL 应用时第一次渲染卡几秒重建，仅一次。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "steam_html_cache",
            category: JunkCategory::BrowserCache,
            name: "Steam 商店 webview 缓存",
            description: "Steam 客户端商店 webview 缓存",
            paths: collect_paths([env_path("LOCALAPPDATA", "Steam\\htmlcache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Steam 客户端内嵌商店页（Chromium webview）的图片、JS、CSS 副本。不会动你的游戏库、存档、好友列表、登录态。删了下次打开商店页会重新加载，慢一两秒，之后照常。Steam 商店卡白屏时官方建议就是删这个目录。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "epic_webcache",
            category: JunkCategory::BrowserCache,
            name: "Epic Games Launcher webview 缓存",
            description: "Epic Games Launcher webview 缓存",
            paths: collect_paths([env_path("LOCALAPPDATA", "EpicGamesLauncher\\Saved\\webcache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Epic Games Launcher 内嵌商店、库页（Chromium webview）的资源副本。不会动你已下载的游戏、存档、登录态、好友列表。Epic 启动器卡白屏或商店打不开时，官方支持第一步就是删这个目录。删了下次打开重新加载，慢两秒。"),
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
            max_depth: None,
            why_safe: Some("Epic 启动器运行日志。不会动你的游戏、存档、登录态。但 Epic 启动器卡死、游戏装不上、下载断流时这里是官方支持要的日志——排障期间不要勾。日常体积通常几十 MB。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "battlenet_cache",
            category: JunkCategory::BrowserCache,
            name: "战网客户端 webview 缓存",
            description: "Battle.net 客户端 webview 缓存",
            paths: collect_paths([env_path("APPDATA", "Battle.net\\Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("战网客户端内嵌商店、新闻页（Chromium webview）的资源副本。不会动你已装的暴雪游戏、存档、登录态、好友列表。战网客户端卡白屏、新闻栏不刷新时官方建议删这个——下次打开重新拉，慢一两秒。"),
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
            max_depth: None,
            why_safe: Some("战网客户端运行日志，位于 ProgramData（系统级共享路径）。不会动游戏本体、存档、登录态。但暴雪客服在你报错时第一件事就是要这个日志——游戏装不上、下载断、登录循环失败的排障期间不要勾。"),
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
            max_depth: None,
            why_safe: Some("英雄联盟、VALORANT 等拳头游戏客户端的运行日志。不会动游戏本体、账号、好友、对战配置。但客户端打不开、自更新卡死、反作弊（Vanguard）报错时这里是技术支持要的——排障期间不要勾。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "ea_app_cache",
            category: JunkCategory::BrowserCache,
            name: "EA 桌面客户端 webview 缓存",
            description: "EA Desktop 客户端 webview 缓存",
            paths: collect_paths([env_path("LOCALAPPDATA", "Electronic Arts\\EA Desktop\\Cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("EA 桌面客户端商店与库页（Chromium webview）的资源副本。不会动 EA 游戏本体、存档、登录态、好友列表。EA 客户端商店卡白屏或库界面打不开时 EA 支持文档建议清这个——下次打开重新拉，慢一两秒。"),
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
            max_depth: None,
            why_safe: Some("EA 桌面客户端运行日志。不会动 EA 游戏本体或存档。但游戏装不上、客户端反复登录失败、下载卡住时这里是 EA Help 客服要的证据——排障期间不要勾。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "zoom_cache",
            category: JunkCategory::BrowserCache,
            name: "Zoom 客户端缓存",
            description: "Zoom 客户端 data\\cache 网络缓存",
            paths: collect_paths([env_path("APPDATA", "Zoom\\data\\cache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Zoom 客户端的网络资源缓存（联系人头像、聊天表情、入会页资源等）。不会动你的会议历史、会议录制、聊天记录（在 Documents\\Zoom 别处）、登录态。删了下次进会议时图片素材稍慢加载，无感。"),
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
            max_depth: None,
            why_safe: Some("Zoom 客户端运行日志，记录加会、音视频协商、网络状况。不会动会议录制文件或聊天记录。但会议进不去、麦克风/摄像头识别失败、音画不同步时 Zoom 支持要这里的日志——排障期间不要勾。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "spotify_storage",
            category: JunkCategory::BrowserCache,
            name: "Spotify 离线缓存",
            description: "Spotify Storage 离线缓存（含用户已下载的离线歌曲）",
            paths: collect_paths([env_path("LOCALAPPDATA", "Spotify\\Storage")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Caution,
            requires_admin: false,
            default_selected: false,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Spotify 已缓存或用户主动下载的离线歌曲（Premium 用户）。删了所有标记为可离线播放的歌单/播客会消失，下次播放需要联网重新流式或重新点下载。不会丢登录态、播放列表、收藏，但出门没网会尴尬——所以默认不勾。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "spotify_browser_cache",
            category: JunkCategory::BrowserCache,
            name: "Spotify 内嵌浏览器缓存",
            description: "Spotify 客户端内嵌 Chromium 的 Browser 子目录缓存",
            paths: collect_paths([env_path("LOCALAPPDATA", "Spotify\\Browser")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Spotify 客户端内嵌 Chromium webview 的资源缓存（封面、播客封图、商店页素材）。不会动你的离线歌曲、登录态、播放列表、收藏、播放记录。删了下次打开 Spotify 客户端时素材慢一两秒重新加载，之后无感。"),
        },
    );

    push_rule_if_paths(
        &mut rules,
        JunkRule {
            id: "spotify_gpu_cache",
            category: JunkCategory::BrowserCache,
            name: "Spotify GPU 缓存",
            description: "Spotify 客户端 GPU 着色器缓存",
            paths: collect_paths([env_path("LOCALAPPDATA", "Spotify\\GPUCache")]),
            patterns: vec![],
            risk_level: JunkRiskLevel::Safe,
            requires_admin: false,
            default_selected: true,
            clean_subdirs_only: false,
            max_depth: None,
            why_safe: Some("Spotify 客户端 GPU 着色器副本，给 UI 渲染加速用。不会动音乐、播放列表、登录态、离线下载。删了下次启动 Spotify 时第一次绘制界面慢一瞬，无感。"),
        },
    );

    rules
}
