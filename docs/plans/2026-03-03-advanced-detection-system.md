# 高级智能检测系统 - 多维度细粒度分析

**创建时间：** 2026-03-03  
**版本：** 2.0 - 智能增强版

---

## 🧠 核心理念

不是简单的"能不能迁移"，而是**多维度评估 + 智能推理**：

1. **应用类型识别**（12 种类型）
2. **依赖关系分析**（7 个维度）
3. **风险因素评估**（10+ 个因素）
4. **智能推荐引擎**（基于规则 + 启发式）

---

## 📊 多维度检测矩阵

### 维度 1: 应用类型分类（12 种）

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ApplicationType {
    // 系统级
    SystemCritical,        // 系统关键组件（绝对禁止）
    SystemUtility,         // 系统工具（不建议）
    
    // 应用级
    InstalledApplication,  // 已安装应用（有注册表）
    PortableApplication,   // 便携式应用（无依赖）
    GameSteam,            // Steam 游戏（可迁移）
    GameEpic,             // Epic 游戏（可迁移）
    GameOther,            // 其他游戏（可迁移）
    
    // 数据级
    UserData,             // 用户数据（推荐迁移）
    MediaFiles,           // 媒体文件（推荐迁移）
    DevelopmentProject,   // 开发项目（可迁移）
    
    // 其他
    TempFiles,            // 临时文件（推荐迁移）
    Unknown,              // 未知类型（需要更多检测）
}
```

### 维度 2: 依赖关系分析（7 个维度）

```rust
#[derive(Debug, Clone)]
pub struct DependencyAnalysis {
    // 1. 注册表依赖
    pub registry_entries: Vec<RegistryEntry>,
    pub registry_dependency_level: DependencyLevel,  // None/Low/Medium/High
    
    // 2. 文件系统依赖
    pub hardcoded_paths: Vec<String>,  // 硬编码路径
    pub relative_paths_only: bool,     // 只使用相对路径
    
    // 3. DLL 依赖
    pub system_dlls: Vec<String>,      // 系统 DLL
    pub local_dlls: Vec<String>,       // 本地 DLL
    pub missing_dlls: Vec<String>,     // 缺失的 DLL
    
    // 4. COM 组件依赖
    pub com_registrations: Vec<String>,
    
    // 5. Windows Installer 依赖
    pub msi_installed: bool,           // 是否通过 MSI 安装
    pub installer_guid: Option<String>,
    
    // 6. 服务依赖
    pub windows_services: Vec<String>,
    
    // 7. 启动项依赖
    pub startup_entries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DependencyLevel {
    None,      // 无依赖（完全便携）
    Low,       // 低依赖（只有配置）
    Medium,    // 中依赖（有注册表但可迁移）
    High,      // 高依赖（深度集成系统）
    Critical,  // 关键依赖（系统组件）
}
```

### 维度 3: 文件特征分析

```rust
#[derive(Debug, Clone)]
pub struct FileCharacteristics {
    // 基础信息
    pub total_size: u64,
    pub file_count: u32,
    pub folder_depth: u32,
    
    // 文件类型分布
    pub file_types: HashMap<String, u32>,  // .exe, .dll, .dat, etc.
    pub executable_count: u32,
    pub config_file_count: u32,
    
    // 特殊标记
    pub has_portable_marker: bool,         // portable.txt, portable.ini
    pub has_uninstaller: bool,             // uninstall.exe
    pub has_installer_artifacts: bool,     // .msi, setup.exe
    
    // 目录结构特征
    pub has_data_folder: bool,             // Data/ 目录
    pub has_config_folder: bool,           // Config/ 目录
    pub has_appinfo: bool,                 // PortableApps 格式
    
    // 访问模式
    pub last_accessed: SystemTime,
    pub last_modified: SystemTime,
    pub access_frequency: AccessFrequency,
}
```

### 维度 4: 运行时状态

```rust
#[derive(Debug, Clone)]
pub struct RuntimeStatus {
    // 进程状态
    pub running_processes: Vec<ProcessInfo>,
    pub process_count: u32,
    
    // 文件锁定
    pub locked_files: Vec<String>,
    pub lock_type: LockType,  // Shared/Exclusive
    
    // 网络连接
    pub active_connections: Vec<NetworkConnection>,
    
    // 系统集成
    pub shell_extensions: Vec<String>,
    pub context_menu_entries: Vec<String>,
}
```

### 维度 5: 路径模式识别

```rust
#[derive(Debug, Clone)]
pub struct PathPattern {
    // 路径分类
    pub path_category: PathCategory,
    
    // 父目录特征
    pub parent_is_program_files: bool,
    pub parent_is_user_folder: bool,
    pub parent_is_system: bool,
    
    // 路径深度
    pub depth_from_root: u32,
    
    // 命名模式
    pub contains_version: bool,        // v1.0, 2024, etc.
    pub contains_company: bool,        // Adobe, Microsoft, etc.
    pub is_generic_name: bool,         // Game, Data, Files, etc.
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathCategory {
    SystemCore,           // C:\Windows\System32
    SystemProgram,        // C:\Program Files\WindowsApps
    ProgramFiles,         // C:\Program Files\*
    ProgramFilesX86,      // C:\Program Files (x86)\*
    UserProfile,          // C:\Users\[user]\*
    UserDocuments,        // C:\Users\[user]\Documents
    UserDownloads,        // C:\Users\[user]\Downloads
    UserAppData,          // C:\Users\[user]\AppData
    CommonGames,          // C:\Games, C:\SteamLibrary
    RootLevel,            // C:\*
    Other,
}
```

### 维度 6: 智能启发式规则

```rust
#[derive(Debug, Clone)]
pub struct HeuristicRules {
    // 游戏检测启发式
    pub is_likely_game: bool,
    pub game_indicators: Vec<GameIndicator>,
    
    // 便携式应用启发式
    pub is_likely_portable: bool,
    pub portable_indicators: Vec<PortableIndicator>,
    
    // 开发工具启发式
    pub is_likely_dev_tool: bool,
    pub dev_indicators: Vec<DevIndicator>,
    
    // 媒体文件启发式
    pub is_likely_media: bool,
    pub media_indicators: Vec<MediaIndicator>,
}

// 游戏检测指标
pub enum GameIndicator {
    SteamAppId,              // steam_appid.txt
    UnrealEngine,            // Engine/ 目录
    UnityEngine,             // *_Data/ 目录
    LargeAssetFiles,         // .pak, .assets 文件
    SaveGamesFolder,         // SaveGames/ 目录
    ConfigIni,               // config.ini, settings.ini
    ExecutableInRoot,        // 根目录有 .exe
}

// 便携式应用指标
pub enum PortableIndicator {
    PortableMarker,          // portable.txt
    DataFolder,              // Data/ 目录
    NoRegistryWrites,        // 运行时不写注册表
    SelfContained,           // 所有文件在自己目录
    NoSystemDependencies,    // 不依赖系统 DLL
    ConfigInFolder,          // 配置文件在应用目录
}
```

---

## 🎯 智能评分算法 v2.0

### 多维度加权评分

```rust
pub fn calculate_advanced_score(analysis: &AdvancedAnalysis) -> MigrationScore {
    let mut score = 70.0;  // 基础分
    let mut confidence = 50.0;  // 置信度
    let mut factors = Vec::new();
    
    // === 第一层：应用类型（权重 40%）===
    match analysis.app_type {
        ApplicationType::SystemCritical => {
            score = 0.0;
            confidence = 100.0;
            factors.push("系统关键组件");
            return build_score(score, confidence, factors);
        }
        ApplicationType::SystemUtility => {
            score -= 30.0;
            confidence += 20.0;
            factors.push("系统工具");
        }
        ApplicationType::PortableApplication => {
            score += 25.0;
            confidence += 30.0;
            factors.push("便携式应用");
        }
        ApplicationType::GameSteam | ApplicationType::GameEpic => {
            score += 20.0;
            confidence += 25.0;
            factors.push("游戏（可迁移）");
        }
        ApplicationType::UserData | ApplicationType::MediaFiles => {
            score += 30.0;
            confidence += 30.0;
            factors.push("用户数据");
        }
        ApplicationType::InstalledApplication => {
            score -= 10.0;
            factors.push("已安装应用");
        }
        _ => {}
    }
    
    // === 第二层：依赖关系（权重 30%）===
    match analysis.dependencies.registry_dependency_level {
        DependencyLevel::None => {
            score += 15.0;
            confidence += 20.0;
            factors.push("无注册表依赖");
        }
        DependencyLevel::Low => {
            score += 5.0;
            factors.push("低注册表依赖");
        }
        DependencyLevel::Medium => {
            score -= 10.0;
            factors.push("中等注册表依赖");
        }
        DependencyLevel::High => {
            score -= 25.0;
            confidence += 15.0;
            factors.push("高注册表依赖");
        }
        DependencyLevel::Critical => {
            score -= 40.0;
            confidence += 25.0;
            factors.push("关键系统依赖");
        }
    }
    
    // 硬编码路径检测
    if !analysis.dependencies.hardcoded_paths.is_empty() {
        score -= 15.0;
        confidence += 10.0;
        factors.push("包含硬编码路径");
    }
    
    // Windows Installer
    if analysis.dependencies.msi_installed {
        score -= 12.0;
        factors.push("MSI 安装");
    }
    
    // Windows 服务
    if !analysis.dependencies.windows_services.is_empty() {
        score -= 30.0;
        confidence += 20.0;
        factors.push("包含 Windows 服务");
    }
    
    // === 第三层：运行时状态（权重 20%）===
    if analysis.runtime.process_count > 0 {
        score -= 35.0;
        confidence += 25.0;
        factors.push("进程正在运行");
    }
    
    if !analysis.runtime.locked_files.is_empty() {
        score -= 30.0;
        confidence += 20.0;
        factors.push("文件被占用");
    }
    
    if !analysis.runtime.shell_extensions.is_empty() {
        score -= 20.0;
        factors.push("Shell 扩展");
    }
    
    // === 第四层：文件特征（权重 10%）===
    if analysis.characteristics.total_size > 10 * 1024 * 1024 * 1024 {
        score += 8.0;  // 大文件夹通常是游戏/媒体
        factors.push("大型文件夹");
    }
    
    if analysis.characteristics.has_portable_marker {
        score += 20.0;
        confidence += 25.0;
        factors.push("便携式标记");
    }
    
    if analysis.characteristics.has_uninstaller {
        score -= 8.0;
        factors.push("包含卸载程序");
    }
    
    // === 第五层：启发式规则（加成）===
    if analysis.heuristics.is_likely_game {
        score += 15.0;
        confidence += 15.0;
        factors.push("游戏特征");
    }
    
    if analysis.heuristics.is_likely_portable {
        score += 18.0;
        confidence += 20.0;
        factors.push("便携式特征");
    }
    
    if analysis.heuristics.is_likely_media {
        score += 12.0;
        factors.push("媒体文件");
    }
    
    // === 第六层：路径模式（微调）===
    match analysis.path_pattern.path_category {
        PathCategory::SystemCore | PathCategory::SystemProgram => {
            score = 0.0;
            confidence = 100.0;
            factors.push("系统路径");
        }
        PathCategory::UserDocuments | PathCategory::UserDownloads => {
            score += 10.0;
            confidence += 15.0;
            factors.push("用户目录");
        }
        PathCategory::CommonGames => {
            score += 12.0;
            factors.push("游戏目录");
        }
        _ => {}
    }
    
    // 限制范围
    score = score.clamp(0.0, 100.0);
    confidence = confidence.clamp(0.0, 100.0);
    
    build_score(score, confidence, factors)
}
```

### 风险等级映射（动态阈值）

```rust
pub fn determine_risk_level(score: f32, confidence: f32) -> RiskLevel {
    // 高置信度时使用严格阈值
    // 低置信度时更保守
    
    let threshold_adjustment = (100.0 - confidence) / 100.0 * 10.0;
    
    let dangerous_threshold = 30.0 + threshold_adjustment;
    let risky_threshold = 50.0 + threshold_adjustment;
    let moderate_threshold = 75.0;
    
    if score < dangerous_threshold {
        RiskLevel::Dangerous
    } else if score < risky_threshold {
        RiskLevel::Risky
    } else if score < moderate_threshold {
        RiskLevel::Moderate
    } else {
        RiskLevel::Safe
    }
}
```

---

## 🔍 智能检测实现

### 1. 应用类型识别

```rust
pub fn identify_application_type(path: &Path, deps: &DependencyAnalysis) -> ApplicationType {
    // 系统关键检测
    if is_system_critical_path(path) {
        return ApplicationType::SystemCritical;
    }
    
    // Steam 游戏检测
    if path.join("steam_appid.txt").exists() 
        || path.to_string_lossy().contains("steamapps") {
        return ApplicationType::GameSteam;
    }
    
    // Epic 游戏检测
    if path.to_string_lossy().contains("Epic Games") 
        || path.join(".egstore").exists() {
        return ApplicationType::GameEpic;
    }
    
    // 便携式应用检测
    if is_portable_app(path) {
        return ApplicationType::PortableApplication;
    }
    
    // 用户数据检测
    if is_user_data_path(path) {
        return ApplicationType::UserData;
    }
    
    // 媒体文件检测
    if is_media_folder(path) {
        return ApplicationType::MediaFiles;
    }
    
    // 已安装应用检测
    if deps.registry_dependency_level != DependencyLevel::None 
        || deps.msi_installed {
        return ApplicationType::InstalledApplication;
    }
    
    ApplicationType::Unknown
}
```

### 2. 注册表依赖深度分析

```rust
pub fn analyze_registry_dependency(path: &Path) -> DependencyAnalysis {
    let mut entries = Vec::new();
    let mut level = DependencyLevel::None;
    
    // 检查卸载注册表
    let uninstall_entries = check_uninstall_registry(path);
    if !uninstall_entries.is_empty() {
        entries.extend(uninstall_entries);
        level = DependencyLevel::Medium;
    }
    
    // 检查 COM 注册
    let com_entries = check_com_registry(path);
    if !com_entries.is_empty() {
        entries.extend(com_entries);
        level = DependencyLevel::High;
    }
    
    // 检查服务注册
    let service_entries = check_service_registry(path);
    if !service_entries.is_empty() {
        entries.extend(service_entries);
        level = DependencyLevel::Critical;
    }
    
    // 检查启动项
    let startup_entries = check_startup_registry(path);
    if !startup_entries.is_empty() {
        entries.extend(startup_entries);
        if level < DependencyLevel::Medium {
            level = DependencyLevel::Low;
        }
    }
    
    DependencyAnalysis {
        registry_entries: entries,
        registry_dependency_level: level,
        // ... 其他字段
    }
}
```

### 3. 游戏检测启发式

```rust
pub fn detect_game_heuristics(path: &Path) -> Vec<GameIndicator> {
    let mut indicators = Vec::new();
    
    // Steam 特征
    if path.join("steam_appid.txt").exists() {
        indicators.push(GameIndicator::SteamAppId);
    }
    
    // Unreal Engine
    if path.join("Engine").exists() || path.join("Content").exists() {
        indicators.push(GameIndicator::UnrealEngine);
    }
    
    // Unity
    for entry in fs::read_dir(path).ok().into_iter().flatten() {
        if let Ok(name) = entry.file_name().into_string() {
            if name.ends_with("_Data") {
                indicators.push(GameIndicator::UnityEngine);
                break;
            }
        }
    }
    
    // 大型资产文件
    let asset_extensions = [".pak", ".assets", ".bundle", ".wad"];
    if has_files_with_extensions(path, &asset_extensions) {
        indicators.push(GameIndicator::LargeAssetFiles);
    }
    
    // 存档目录
    if path.join("SaveGames").exists() || path.join("Saves").exists() {
        indicators.push(GameIndicator::SaveGamesFolder);
    }
    
    // 配置文件
    if path.join("config.ini").exists() || path.join("settings.ini").exists() {
        indicators.push(GameIndicator::ConfigIni);
    }
    
    indicators
}
```

---

## 📈 预期效果

### 分类准确率

| 类型 | 准确率 | 说明 |
|------|--------|------|
| 系统关键 | 100% | 路径匹配 |
| 用户数据 | 98% | 路径 + 文件类型 |
| Steam 游戏 | 95% | steam_appid.txt |
| 便携式应用 | 90% | 多特征检测 |
| 已安装应用 | 85% | 注册表 + MSI |
| 未知类型 | 70% | 启发式推理 |

### 风险分布（预期）

- 🔴 危险：< 3%（只有系统目录）
- 🟠 高风险：~8-12%（运行中程序、系统工具）
- 🟡 中风险：~25-35%（已安装应用）
- 🟢 安全：~50-65%（用户数据、游戏、便携式应用）

---

## 🚀 实施优先级

### Phase 1: 核心检测（必须）
1. 应用类型识别
2. 注册表依赖分析
3. 运行时状态检测
4. 基础评分算法

### Phase 2: 智能增强（推荐）
5. 游戏检测启发式
6. 便携式应用检测
7. 文件特征分析
8. 高级评分算法

### Phase 3: 深度分析（可选）
9. DLL 依赖分析
10. COM 组件检测
11. 硬编码路径扫描
12. 机器学习分类（未来）

---

**最后更新：** 2026-03-03  
**版本：** 2.0 - 智能增强版
