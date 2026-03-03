# 迁移安全检测引擎 - 详细设计文档

**创建时间：** 2026-03-03  
**优先级：** 最高 ⭐⭐⭐⭐⭐  
**预计时间：** 8-12 小时

---

## 📋 目标

在绝对确保安全的前提下，尽可能允许用户迁移文件，防止：
- 系统崩溃
- 软件无法启动
- 数据丢失
- 注册表损坏

---

## 🎯 核心原则

1. **安全第一**：宁可误判为危险，也不能让用户迁移系统关键文件
2. **透明化**：明确告诉用户为什么有风险
3. **可控性**：用户有最终决策权（除了绝对危险的）
4. **可回滚**：所有迁移都可以撤销

---

## 🏗️ 架构设计

### 模块结构

```
src-tauri/src/
├── safety/
│   ├── mod.rs              # 模块导出
│   ├── detector.rs         # 核心检测逻辑
│   ├── rules.rs            # 规则引擎配置
│   ├── registry.rs         # 注册表检测
│   ├── file_lock.rs        # 文件占用检测
│   └── portable.rs         # 便携式应用检测
├── commands.rs             # 添加新命令
└── lib.rs                  # 注册模块
```

### 数据流

```
用户选择文件/目录
    ↓
调用 analyze_migration_safety()
    ↓
多层检测器并行执行
    ↓
规则引擎评分
    ↓
返回 MigrationSafety 结果
    ↓
前端显示风险等级 + 详情
    ↓
用户确认 → 执行迁移
```

---

## 🔍 检测层级

### Layer 1: 路径模式匹配（最快，100% 准确）

**黑名单（绝对禁止）：**
```rust
const ABSOLUTE_BLACKLIST: &[&str] = &[
    "C:\\Windows",
    "C:\\Program Files\\WindowsApps",
    "C:\\ProgramData\\Microsoft\\Windows",
    "C:\\System Volume Information",
    "C:\\$Recycle.Bin",
    "C:\\Recovery",
    "C:\\Boot",
    "C:\\bootmgr",
    "C:\\pagefile.sys",
    "C:\\hiberfil.sys",
    "C:\\swapfile.sys",
];
```

**白名单（绝对安全）：**
```rust
const SAFE_WHITELIST: &[&str] = &[
    "C:\\Users\\*\\Downloads",
    "C:\\Users\\*\\Documents",
    "C:\\Users\\*\\Videos",
    "C:\\Users\\*\\Pictures",
    "C:\\Users\\*\\Music",
    "C:\\Users\\*\\Desktop",
    "C:\\Temp",
    "C:\\tmp",
];
```

### Layer 2: 文件占用检测（快速，100% 准确）

```rust
fn is_file_locked(path: &Path) -> Result<bool> {
    use std::fs::OpenOptions;
    
    // 尝试以独占写入模式打开
    match OpenOptions::new()
        .write(true)
        .create(false)
        .open(path)
    {
        Ok(_) => Ok(false),  // 未被占用
        Err(e) if e.kind() == ErrorKind::PermissionDenied => Ok(true),  // 被占用
        Err(e) => Err(e.into()),
    }
}
```

### Layer 3: 注册表依赖检测（中速，95% 准确）

```rust
use winreg::enums::*;
use winreg::RegKey;

fn check_registry_dependency(path: &Path) -> Result<bool> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    
    // 检查卸载注册表
    let uninstall_paths = [
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        "SOFTWARE\\Wow6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    ];
    
    for reg_path in &uninstall_paths {
        if let Ok(key) = hklm.open_subkey(reg_path) {
            for subkey_name in key.enum_keys().filter_map(Result::ok) {
                if let Ok(subkey) = key.open_subkey(&subkey_name) {
                    // 检查 InstallLocation
                    if let Ok(install_loc) = subkey.get_value::<String, _>("InstallLocation") {
                        if path.starts_with(&install_loc) {
                            return Ok(true);  // 有注册表依赖
                        }
                    }
                }
            }
        }
    }
    
    Ok(false)
}
```

### Layer 4: 便携式应用特征检测（快速，80% 准确）

```rust
fn is_portable_app(path: &Path) -> bool {
    // 检查便携式应用的常见特征
    let indicators = [
        "portable.txt",
        "portable.ini",
        "App\\AppInfo\\appinfo.ini",  // PortableApps.com 格式
        "Data",  // 便携式应用通常有 Data 目录
    ];
    
    for indicator in &indicators {
        if path.join(indicator).exists() {
            return true;
        }
    }
    
    // 检查是否所有配置都在自己目录
    let has_external_config = path.join("config.ini").exists() 
        || path.join("settings.json").exists();
    
    has_external_config
}
```

### Layer 5: 进程检测（中速，100% 准确）

```rust
use sysinfo::{System, SystemExt, ProcessExt};

fn is_process_running(path: &Path) -> bool {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    for (_pid, process) in sys.processes() {
        if let Some(exe) = process.exe() {
            if exe.starts_with(path) {
                return true;  // 有进程正在运行
            }
        }
    }
    
    false
}
```

---

## 📊 评分系统

### 评分规则

```rust
fn calculate_safety_score(checks: &DetectionResults) -> u8 {
    let mut score = 50;  // 基础分 50
    
    // 黑名单：-50（直接归零）
    if checks.in_blacklist {
        return 0;
    }
    
    // 白名单：+40
    if checks.in_whitelist {
        score += 40;
    }
    
    // 注册表依赖：-20
    if checks.has_registry_entry {
        score -= 20;
    }
    
    // 文件被占用：-30
    if checks.is_locked {
        score -= 30;
    }
    
    // 进程运行中：-25
    if checks.process_running {
        score -= 25;
    }
    
    // 便携式应用：+30
    if checks.is_portable {
        score += 30;
    }
    
    // Program Files 目录：-15
    if checks.in_program_files {
        score -= 15;
    }
    
    score.clamp(0, 100)
}
```

### 风险等级映射

```rust
fn score_to_risk_level(score: u8) -> RiskLevel {
    match score {
        0..=20 => RiskLevel::Dangerous,   // 🔴
        21..=40 => RiskLevel::Risky,      // 🟠
        41..=70 => RiskLevel::Moderate,   // 🟡
        71..=100 => RiskLevel::Safe,      // 🟢
    }
}
```

---

## 🎨 前端集成

### 1. 列表视图显示

```vue
<template>
  <tr :class="getRiskClass(item.safety)">
    <td>
      <span :class="getRiskIcon(item.safety)"></span>
      {{ item.name }}
    </td>
    <td>{{ formatSize(item.size) }}</td>
    <td>
      <button 
        @click="showSafetyDetails(item)"
        :disabled="!item.safety.can_migrate"
      >
        迁移
      </button>
    </td>
  </tr>
</template>

<style scoped>
.risk-safe { background: rgba(34, 197, 94, 0.1); }
.risk-moderate { background: rgba(234, 179, 8, 0.1); }
.risk-risky { background: rgba(249, 115, 22, 0.1); }
.risk-dangerous { 
  background: rgba(239, 68, 68, 0.1);
  opacity: 0.6;
}
</style>
```

### 2. 风险详情弹窗

```vue
<template>
  <div class="safety-dialog">
    <h3>⚠️ 迁移风险评估</h3>
    
    <div class="path">
      📁 {{ item.path }}
    </div>
    
    <div class="risk-badge" :class="getRiskClass(safety.risk_level)">
      {{ getRiskLabel(safety.risk_level) }}
    </div>
    
    <div class="score">
      安全评分：{{ safety.safety_score }}/100
    </div>
    
    <div class="checks">
      <h4>检测结果：</h4>
      <ul>
        <li v-for="reason in safety.reasons" :key="reason">
          {{ getCheckIcon(reason) }} {{ reason }}
        </li>
      </ul>
    </div>
    
    <div class="impact">
      <h4>可能的影响：</h4>
      <ul>
        <li v-for="impact in getImpacts(safety)" :key="impact">
          {{ impact }}
        </li>
      </ul>
    </div>
    
    <div class="recommendations">
      <h4>建议：</h4>
      <ul>
        <li v-for="rec in safety.recommendations" :key="rec">
          {{ rec }}
        </li>
      </ul>
    </div>
    
    <div v-if="safety.risk_level === 'Risky'" class="confirm">
      <label>
        <input type="checkbox" v-model="userConfirmed" />
        我了解风险，仍要继续
      </label>
    </div>
    
    <div class="actions">
      <button @click="close">取消</button>
      <button 
        @click="migrate"
        :disabled="!canMigrate"
        class="primary"
      >
        继续迁移
      </button>
    </div>
  </div>
</template>
```

---

## 🧪 测试计划

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_blacklist_detection() {
        let path = Path::new("C:\\Windows\\System32");
        let result = analyze_migration_safety(path).unwrap();
        assert_eq!(result.risk_level, RiskLevel::Dangerous);
        assert!(!result.can_migrate);
    }
    
    #[test]
    fn test_whitelist_detection() {
        let path = Path::new("C:\\Users\\Test\\Downloads");
        let result = analyze_migration_safety(path).unwrap();
        assert_eq!(result.risk_level, RiskLevel::Safe);
        assert!(result.can_migrate);
    }
    
    #[test]
    fn test_registry_dependency() {
        // 测试已安装程序的检测
    }
    
    #[test]
    fn test_file_lock() {
        // 测试文件占用检测
    }
}
```

### 集成测试

1. **系统目录测试**
   - `C:\Windows\` → 应该被禁止
   - `C:\Program Files\WindowsApps\` → 应该被禁止

2. **用户目录测试**
   - `C:\Users\[用户]\Downloads\` → 应该是安全的
   - `C:\Users\[用户]\Documents\` → 应该是安全的

3. **已安装程序测试**
   - Adobe、Office 等 → 应该有风险提示
   - Steam 游戏 → 应该是中风险

4. **便携式应用测试**
   - 7-Zip Portable → 应该是低风险
   - VSCode Portable → 应该是低风险

5. **运行中程序测试**
   - 正在运行的程序 → 应该被禁止或高风险

---

## 📦 依赖库

### Cargo.toml

```toml
[dependencies]
# 规则引擎
rusty-rules = "0.2"

# Windows API
winreg = "0.52"
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_System_Registry",
] }

# 系统信息
sysinfo = "0.31"

# 序列化
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# 错误处理
thiserror = "2.0"
anyhow = "1.0"
```

---

## 🚀 实施步骤

### Step 1: 基础架构（2 小时）

- [ ] 创建 `safety` 模块
- [ ] 定义数据结构（`MigrationSafety`, `RiskLevel`）
- [ ] 实现路径黑/白名单检测
- [ ] 添加 Tauri 命令

### Step 2: 核心检测（3 小时）

- [ ] 实现文件占用检测
- [ ] 实现注册表依赖检测
- [ ] 实现进程检测
- [ ] 实现便携式应用检测

### Step 3: 评分系统（2 小时）

- [ ] 实现评分算法
- [ ] 实现风险等级映射
- [ ] 生成建议和说明

### Step 4: 前端集成（3 小时）

- [ ] 修改扫描结果数据结构
- [ ] 添加风险等级显示
- [ ] 创建风险详情弹窗
- [ ] 添加确认机制

### Step 5: 测试优化（2 小时）

- [ ] 编写单元测试
- [ ] 进行集成测试
- [ ] 性能优化
- [ ] 文档完善

---

## 📝 配置文件

### migration-rules.json

```json
{
  "version": "1.0.0",
  "updated": "2026-03-03",
  "rules": {
    "blacklist": {
      "paths": [
        "C:\\Windows",
        "C:\\Program Files\\WindowsApps",
        "C:\\ProgramData\\Microsoft\\Windows"
      ],
      "description": "系统关键目录，绝对禁止迁移"
    },
    "whitelist": {
      "paths": [
        "C:\\Users\\*\\Downloads",
        "C:\\Users\\*\\Documents",
        "C:\\Users\\*\\Videos"
      ],
      "description": "用户数据目录，安全迁移"
    },
    "scoring": {
      "base_score": 50,
      "blacklist": -50,
      "whitelist": 40,
      "registry_dependency": -20,
      "file_locked": -30,
      "process_running": -25,
      "portable_app": 30,
      "program_files": -15
    },
    "thresholds": {
      "dangerous": 20,
      "risky": 40,
      "moderate": 70,
      "safe": 100
    }
  }
}
```

---

## 🎯 成功标准

1. ✅ 系统关键目录 100% 被拦截
2. ✅ 用户数据目录 100% 标记为安全
3. ✅ 已安装程序正确识别并提示风险
4. ✅ 便携式应用正确识别
5. ✅ 运行中程序被拦截或高风险提示
6. ✅ 用户界面清晰易懂
7. ✅ 性能影响 < 100ms

---

**最后更新：** 2026-03-03  
**状态：** 待实施
