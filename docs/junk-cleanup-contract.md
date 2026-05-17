# 垃圾清理功能 · 五路并行开发契约

> **目标**：为 CSD 增加 Windows 一键垃圾清理（系统临时、浏览器缓存、Windows Update 残留、缩略图缓存、崩溃转储等）。
> **方式**：5 个 AI **同时并行**工作，不需要等待彼此。
> **保证**：所有路使用本契约中**完全相同**的类型签名，不能自创字段名。

---

## 0. 并行模型

```
┌─────────────────────────────────────────────────────────┐
│   5 路 AI 同时启动，互相不通信，各自独立完成            │
└─────────────────────────────────────────────────────────┘
                          │
        ┌─────────┬───────┴───────┬─────────┬─────────┐
        ▼         ▼               ▼         ▼         ▼
     路 1      路 2             路 3      路 4      路 5
   规则表    扫描器+mod        删除器    主界面    Tab接入
   (Rust)    (Rust)            +命令     (Vue)     (Vue)
                              (Rust)
```

**为什么能并行**：每路的文件触碰范围零重叠（见 §4 文件分配表）。所有跨路引用都按本契约第 2、3 节的类型签名 import，AI 无需读取其它路的产出代码。

**整合时机**：5 路全部完成后，做一次 `cargo check` + `npx vue-tsc --noEmit`，最多 5 分钟修对齐问题（基本就是 `mod junk;` 是否落到 lib.rs，命令是否注册）。

---

## 1. 任务全景

| 路 | 角色 | 负责文件 | 依赖 |
|---|---|---|---|
| **1** | Rust 规则库 | 1 个新文件 | 无（独立） |
| **2** | Rust 扫描器 | 2 个新文件 | 类型契约 §2.1 |
| **3** | Rust 清理器 + 命令 | 1 新文件 + 2 文件追加 | 类型契约 §2 全部 |
| **4** | Vue 主界面 | 2 个新文件 | 类型契约 §3 |
| **5** | App.vue Tab 接入 + 预设面板 | 1 新文件 + App.vue 改 4 处 | 类型契约 §3 |

---

## 2. Rust 类型契约（路 1/2/3 必须严格一致）

### 2.1 规则定义（路 1 实现，路 2/3 import）

文件：`src-tauri/src/junk/rules.rs`

```rust
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
pub enum JunkRiskLevel { Safe, Caution, Risky }

#[derive(Debug, Clone, Serialize)]
pub struct JunkRule {
    pub id: &'static str,            // 全局唯一英文 id（如 "chrome_cache"）
    pub category: JunkCategory,
    pub name: &'static str,          // 中文显示名
    pub description: &'static str,   // 中文一句话说明
    pub paths: Vec<String>,          // 已展开过环境变量的绝对路径列表
    pub patterns: Vec<&'static str>, // glob 简单模式，如 "*.tmp"。空表示整目录
    pub risk_level: JunkRiskLevel,
    pub requires_admin: bool,
    pub default_selected: bool,      // 扫描完是否默认勾选
    pub clean_subdirs_only: bool,    // true=保留目录本身，只删内容
}

pub fn all_rules() -> Vec<JunkRule>;
```

环境变量展开取自 `std::env::var`：`TEMP`、`LOCALAPPDATA`、`APPDATA`、`USERPROFILE`、`WINDIR`、`PROGRAMDATA`。取不到就跳过该规则的对应路径条目，不要 panic。

### 2.2 扫描结果（路 2 实现，路 3 import）

文件：`src-tauri/src/junk/scanner.rs`

```rust
use crate::junk::rules::{JunkCategory, JunkRiskLevel};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct JunkItem {
    pub rule_id: String,
    pub category: JunkCategory,
    pub category_name: String,   // 中文，如 "浏览器缓存"
    pub rule_name: String,       // 来自 rule.name
    pub path: String,            // 实际找到的文件或目录绝对路径
    pub size: u64,
    pub file_count: usize,
    pub is_directory: bool,
    pub risk_level: JunkRiskLevel,
    pub default_selected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct JunkScanResult {
    pub items: Vec<JunkItem>,
    pub total_size: u64,
    pub total_count: usize,
    pub scanned_rules: usize,
    pub skipped_rules: Vec<String>,   // rule_id 列表（无权限/路径不存在/缺环境变量）
    pub scan_duration_ms: u64,
}

pub fn scan_junk_blocking(
    categories: Option<Vec<JunkCategory>>, // None = 全部
    is_admin: bool,
) -> anyhow::Result<JunkScanResult>;
```

### 2.3 清理结果（路 3 实现）

文件：`src-tauri/src/junk/cleaner.rs`

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct JunkCleanError {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JunkCleanResult {
    pub success: bool,
    pub cleaned_size: u64,
    pub cleaned_count: usize,
    pub failed_count: usize,
    pub errors: Vec<JunkCleanError>,
    pub duration_ms: u64,
}

pub async fn clean_junk_paths(
    paths: Vec<String>,
    to_recycle_bin: bool, // true → DeleteMode::Recycle, false → DeleteMode::Permanent
) -> anyhow::Result<JunkCleanResult>;
```

### 2.4 Tauri 命令（路 3 在 commands.rs 末尾追加）

```rust
#[tauri::command]
pub async fn scan_junk_files(
    categories: Option<Vec<crate::junk::rules::JunkCategory>>,
) -> Result<crate::junk::scanner::JunkScanResult, String>;

#[tauri::command]
pub async fn clean_junk_files(
    paths: Vec<String>,
    to_recycle_bin: bool,
) -> Result<crate::junk::cleaner::JunkCleanResult, String>;
```

**前端 invoke 时参数命名采用 camelCase**（Tauri 自动转换）：`{ categories, toRecycleBin }`。

---

## 3. TypeScript 类型契约（路 4/5 必须严格一致）

文件：`src/types/junk.ts`（路 4 创建，路 5 仅 import）

```typescript
export type JunkCategory =
  | 'system_temp' | 'browser_cache' | 'windows_update'
  | 'thumbnail_cache' | 'recycle_bin' | 'crash_dump'
  | 'app_logs' | 'font_cache';

export type JunkRiskLevel = 'safe' | 'caution' | 'risky';

export interface JunkItem {
  rule_id: string;
  category: JunkCategory;
  category_name: string;
  rule_name: string;
  path: string;
  size: number;
  file_count: number;
  is_directory: boolean;
  risk_level: JunkRiskLevel;
  default_selected: boolean;
}

export interface JunkScanResult {
  items: JunkItem[];
  total_size: number;
  total_count: number;
  scanned_rules: number;
  skipped_rules: string[];
  scan_duration_ms: number;
}

export interface JunkCleanResult {
  success: boolean;
  cleaned_size: number;
  cleaned_count: number;
  failed_count: number;
  errors: { path: string; error: string }[];
  duration_ms: number;
}

export const CATEGORY_LABELS: Record<JunkCategory, string> = {
  system_temp: '系统临时文件',
  browser_cache: '浏览器缓存',
  windows_update: 'Windows 更新残留',
  thumbnail_cache: '缩略图缓存',
  recycle_bin: '回收站',
  crash_dump: '崩溃转储',
  app_logs: '应用日志',
  font_cache: '字体缓存',
};

// invoke 命令名（路 4/5 都从这里取，不要硬编码字符串）
export const CMD_SCAN_JUNK = 'scan_junk_files' as const;
export const CMD_CLEAN_JUNK = 'clean_junk_files' as const;
```

---

## 4. 文件分配表（零重叠保证）

| 路 | 新建 | 修改（追加） | **禁止改动** |
|---|---|---|---|
| 1 | `src-tauri/src/junk/rules.rs` | — | 其它一切 |
| 2 | `src-tauri/src/junk/mod.rs`（声明 3 个 sub mod）<br>`src-tauri/src/junk/scanner.rs` | — | rules.rs / cleaner.rs |
| 3 | `src-tauri/src/junk/cleaner.rs` | `src-tauri/src/commands.rs`（**仅文件末尾追加** 2 个 #[tauri::command]）<br>`src-tauri/src/lib.rs`（**仅追加** `pub mod junk;` 和 invoke_handler 列表里的 2 个命令名） | rules.rs / scanner.rs / mod.rs |
| 4 | `src/types/junk.ts`<br>`src/components/JunkCleanView.vue` | — | App.vue / 其它组件 |
| 5 | `src/components/JunkPresets.vue` | `src/App.vue`（**仅 4 处**：见 §6.5） | 其它一切 |

> **关键**：路 2 创建的 `mod.rs` 内容固定为：
> ```rust
> pub mod rules;
> pub mod scanner;
> pub mod cleaner;
> ```
> 路 3 不要再写 mod.rs，路 1 也不要写。

---

## 5. 已有可复用组件（不要重复造）

| 需要的能力 | 已有的位置 | 用法 |
|---|---|---|
| 删除文件（回收站 / 永久） | `src-tauri/src/migration/delete.rs::delete_path(path, DeleteMode, None)` | 路 3 直接调 |
| 检测当前进程是否管理员 | `crate::commands::is_elevated() -> bool` | 路 3 调用（**不在 winfs**） |
| 字节格式化 | `src/utils/format.ts::formatBytes(n: number): string` | 路 4/5 import |
| 日期格式化 | 同上 `formatDate` | 可选 |
| Toast 通知 | `src/components/Toast.vue` 模式（参考 DuplicatesView 怎么用） | 路 4 |
| 确认对话框 | `src/components/ConfirmDialog.vue` | 路 4 |
| 旋转图标 | `src/components/icons/scan/IconSpinner.vue` | 路 4 |
| 卡片视觉 | `src/components/DiskCard.vue` | 路 5 预设卡片照抄 |
| 列表视觉 + 勾选交互 | `src/components/DuplicatesView.vue` | 路 4 主参考 |

**已有 Cargo 依赖**：`walkdir 2.4`、`jwalk 0.8`、`rayon 1.8`、`anyhow 1.0`、`tracing 0.1`、`serde 1`、`trash 5`、`tokio 1.35`、`tauri 2`。**路 1/2/3 不要新增依赖**。

---

## 6. 各路详细任务

### 6.1 路 1｜Rust 规则库

**唯一职责**：写出 30+ 条 Windows 垃圾路径规则。

**输出文件**：`src-tauri/src/junk/rules.rs`（按 §2.1 类型签名）

**规则覆盖范围（必须包含）**：

| 分类 | 规则示例 | 风险 | 默认勾选 |
|---|---|---|---|
| SystemTemp | `%TEMP%`、`%LOCALAPPDATA%\Temp`、`%WINDIR%\Temp`、`%WINDIR%\Prefetch` | Caution | ✅ |
| BrowserCache | Chrome `%LOCALAPPDATA%\Google\Chrome\User Data\Default\Cache`、`Code Cache`、`GPUCache`、`Service Worker\CacheStorage`<br>Edge `%LOCALAPPDATA%\Microsoft\Edge\User Data\Default\Cache` 等同<br>Firefox `%LOCALAPPDATA%\Mozilla\Firefox\Profiles\*\cache2`（路径含 `*`，按 §6.1.b 处理） | Safe | ✅ |
| WindowsUpdate | `%WINDIR%\SoftwareDistribution\Download`、`%WINDIR%\Logs\WindowsUpdate` | Caution（requires_admin=true） | ✅ |
| ThumbnailCache | `%LOCALAPPDATA%\Microsoft\Windows\Explorer` patterns=`["thumbcache_*.db","iconcache_*.db"]` | Safe | ✅ |
| RecycleBin | `C:\$Recycle.Bin`（clean_subdirs_only=true） | Caution | ❌ |
| CrashDump | `%LOCALAPPDATA%\CrashDumps`、`%LOCALAPPDATA%\Microsoft\Windows\WER` | Safe | ✅ |
| AppLogs | 各类 `*.log`（保守） | Risky | ❌ |
| FontCache | `%WINDIR%\ServiceProfiles\LocalService\AppData\Local\FontCache`（requires_admin=true） | Safe | ❌ |

**6.1.b 关于 `*` 通配父目录**：本路只把含 `*` 的路径**原样写入** `paths` 字段；展开（即扫描时遇到 `*` 列出实际子目录）由路 2 负责。

**禁止**：写扫描逻辑、写 mod.rs、import Tauri、改其它文件。

**验证**：`cd src-tauri && cargo check` 通过（路 2 还没合也要通过，因为本路自包含）。

---

### 6.2 路 2｜Rust 扫描器 + mod 入口

**输出文件**：
- `src-tauri/src/junk/mod.rs`（内容 3 行，固定，见 §4）
- `src-tauri/src/junk/scanner.rs`（按 §2.2 类型签名）

**实现要点**：
- 用 rayon 并行处理不同 rule
- categories=Some 时只处理命中的 rule；is_admin=false 时跳过 requires_admin=true 的 rule（写入 skipped_rules）
- patterns 为空：整个 rule.path 视为一项 JunkItem（递归算总大小）
- patterns 非空：在 rule.path 下用 `walkdir` 遍历，文件名按简单 glob 匹配（只支持 `*` 和精确，不需要正则）
- rule.path 含 `*`：用 `std::fs::read_dir` 列父目录，名字匹配后再递归
- IO 错误：`tracing::debug!` 记录，不 panic、不终止
- 路径不存在：静默跳过，rule_id 写入 skipped_rules
- 不发 Tauri event（先简单实现，留 TODO 给后续）
- `category_name` 字段填中文，跟 §3 的 `CATEGORY_LABELS` 对齐

**禁止**：写 cleaner、写 command、改 lib.rs、改 rules.rs。

**验证**：`cargo check` 通过。

---

### 6.3 路 3｜Rust 清理器 + 命令注册

**输出**：
- 新建：`src-tauri/src/junk/cleaner.rs`（按 §2.3）
- 追加：`src-tauri/src/commands.rs` **末尾**追加 §2.4 的两个命令
- 追加：`src-tauri/src/lib.rs` **两处追加**：
  1. 顶部 mod 区域：`pub mod junk;`
  2. `tauri::generate_handler![...]` 列表末尾：`commands::scan_junk_files, commands::clean_junk_files,`

**清理实现**：
- 复用 `crate::migration::delete::delete_path(path, DeleteMode::Recycle | DeleteMode::Permanent, None).await`
- 串行处理 paths，单条失败不中断，写入 errors 继续
- 删除前 `Path::new(p).exists()` 跳过已不存在的（不计入 errors）
- `to_recycle_bin=true` → `DeleteMode::Recycle`，否则 `DeleteMode::Permanent`
- 聚合：`cleaned_size += result.deleted_size`、`cleaned_count += result.deleted_files`、`errors.extend(result.errors.map(...))`
- `success = errors.is_empty()`

**管理员检测**：在 `scan_junk_files` 命令体内调 `crate::commands::is_elevated()`（**不要** 写在 winfs，已确认该函数定义在 commands.rs 中）。

**禁止**：改 rules.rs、scanner.rs、mod.rs；改前端任何文件。

**验证**：`cargo check` 通过；`cargo build` 也通过。

---

### 6.4 路 4｜Vue 主界面 + 类型文件

**输出**：
- 新建：`src/types/junk.ts`（按 §3 完整复制）
- 新建：`src/components/JunkCleanView.vue`（`<script setup lang="ts">` 风格）

**JunkCleanView 行为**：
- 进入不自动扫，提供"开始扫描"按钮
- 扫描中：显示 `IconSpinner` + 文案"正在扫描垃圾文件…"
- 扫描完：按 category 用 `el-collapse` 折叠分组
  - 标题：`{CATEGORY_LABELS[cat]} · {n} 项 · {formatBytes(total)}`
  - default_selected 项最多的 3 组默认展开
- 每项一行：rule_name + path（中间截断 / hover 显示全路径） + size + 风险 badge（`safe`=绿色 `el-tag type="success"`、`caution`=黄 `el-tag type="warning"`、`risky`=红 `el-tag type="danger"`）
- 操作栏：[全选] [全不选] [仅选安全] 三按钮
- 顶部状态条：`已选 N 项 · 共 M GB` + [清理到回收站] [永久删除]
- 点删除按钮 → `ConfirmDialog`（永久删除文案标红）→ `invoke('clean_junk_files', { paths: Array.from(checked), toRecycleBin })`
- 清理完 → Toast → 自动重扫
- import 命令名用 `CMD_SCAN_JUNK` / `CMD_CLEAN_JUNK` 常量
- 默认勾选：扫描后把 `items.filter(i => i.default_selected).map(i => i.path)` 装入 `Set<string>`

**禁止**：改 App.vue、改其它组件。

**验证**：`npx vue-tsc --noEmit`（除新文件外不应报新错）。

---

### 6.5 路 5｜App.vue Tab 接入 + 预设面板

**输出 1**：新建 `src/components/JunkPresets.vue`

三张预设卡片，emit `apply` 事件给父组件：

```typescript
const emit = defineEmits<{ apply: [categories: JunkCategory[] | null] }>();
```

| 卡片 | categories | 默认高亮 |
|---|---|---|
| 快速清理 | `['browser_cache', 'thumbnail_cache', 'crash_dump']` | |
| 标准清理（推荐） | `['browser_cache','thumbnail_cache','crash_dump','system_temp','windows_update']` | ✅ |
| 深度清理（管理员） | `null` | |

样式参考 `src/components/DiskCard.vue`。本路**只创建组件文件，不在任何地方挂载它**（路 4 之后可以选择性接入，本路不负责）。

**输出 2**：修改 `src/App.vue`，**仅 4 处改动**：

```diff
// 1. import 区追加
+ import JunkCleanView from './components/JunkCleanView.vue';

// 2. activeTab 类型扩展（已有: 'workspace' | 'games'）
- const activeTab = ref<'workspace' | 'games'>('workspace');
+ const activeTab = ref<'workspace' | 'games' | 'junk'>('workspace');

// 3. workspace-tabs 容器内追加一个 button（紧跟"游戏库"按钮之后）
+ <button
+   class="workspace-tab"
+   :class="{ active: activeTab === 'junk' }"
+   @click="activeTab = 'junk'"
+ >垃圾清理</button>

// 4. 主内容区追加（紧跟现有 Workspace 组件之后，与 GamesView 同级）
+ <JunkCleanView v-if="activeTab === 'junk'" />
```

**禁止**：改其它组件、改 router、改 Rust。

**验证**：`npx vue-tsc --noEmit`；前端能成功 mount Tab（Rust 三路没合也能跑，命令调用会报错但 UI 出得来）。

---

## 7. 整合 Checklist（5 路全部完成后）

```powershell
# 1. Rust 编译
cd d:\Desktop\cdrive-cleaner\src-tauri
cargo check
cargo build --release

# 2. 前端类型 + 打包
cd d:\Desktop\cdrive-cleaner
npx vue-tsc --noEmit
npm run build

# 3. 联调
npm run tauri dev
# → 切到"垃圾清理"Tab → 开始扫描 → 看到 items → 勾选 → 清理到回收站 → 验证空间释放
```

**常见对齐问题**：
- 字段名 camelCase / snake_case 不齐：Tauri 默认 serde 用 snake_case，前端字段也用 snake_case（已在 §3 写明）
- `is_admin` 来源拿错：必须是 `crate::commands::is_elevated()`，不在 winfs
- 命令注册漏了：检查 `lib.rs` invoke_handler 列表
- `mod junk;` 漏了：检查 `lib.rs` 顶部

---

## 8. 给 5 个 AI 的提示词

每个 AI 看到的提示词都是「读取 `docs/junk-cleanup-contract.md`，按第 N 路完成任务」。

### 提示词模板

```
你的工作目录：d:\Desktop\cdrive-cleaner
你正在与其它 4 个 AI 同时并行开发同一个功能。请先完整阅读：
docs/junk-cleanup-contract.md

你只负责【第 N 路】，对应章节 §6.N。
严格遵守：
- 所有跨路引用的类型签名必须与契约 §2、§3 完全一致
- 只动 §4 文件分配表中本路允许的文件，禁止改动其它文件
- 不要假设其它路的产出代码内容，只按本契约的类型签名工作
- 完成后按 §6.N 末尾的"验证"步骤自检

完成后报告：
1. 创建/修改了哪些文件
2. 验证命令是否通过
3. 有无偏离契约（应该没有；如有，说明原因）
```

把上面 5 份提示词分别填入 N=1/2/3/4/5 发给 5 个 AI 即可。

---

## 9. 时间预估

| 阶段 | 时长 |
|---|---|
| 5 路并行执行 | ~45-60 分钟（取决于 AI 并发速度） |
| 整合 + 修对齐问题 | ~5-10 分钟 |
| 联调验证 | ~10 分钟 |
| **合计** | **~1-1.5 小时** |

对比单 AI 串行：3-4 小时。**节省 60%+**。
