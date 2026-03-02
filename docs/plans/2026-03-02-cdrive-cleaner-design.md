# C 盘清理工具 - 设计文档

**项目名称：** CDrive Cleaner  
**创建日期：** 2026-03-02  
**状态：** 设计阶段  
**作者：** 巴建峰

---

## 1. 项目概述

### 1.1 项目目标

开发一款高性能的 C 盘清理工具，通过智能文件迁移和符号链接技术，在不影响程序正常运行的前提下释放 C 盘空间。

### 1.2 核心价值

- **智能迁移**：不是简单删除，而是迁移到其他盘并创建符号链接
- **性能优先**：30-60 秒扫描 C 盘，接近专业工具（WinDirStat/TreeSize）的速度
- **完整性保证**：获取 99%+ 的文件信息，和 Windows 文件管理器一致
- **用户友好**：CLI + GUI 双界面，适合不同使用场景

### 1.3 目标用户

- 主要用户：开发者本人及身边的技术人员
- 次要用户：普通用户（通过 GUI）
- 未来：开源到 GitHub，服务更广泛的用户群体

---

## 2. 功能设计

### 2.1 功能优先级

**第一阶段（MVP - 核心功能）：**
1. 磁盘空间扫描和可视化（树状图/饼图显示占用）
2. 大文件/大文件夹识别和排序
3. 文件/文件夹迁移 + 符号链接创建
4. 迁移记录保存

**第二阶段（增强功能）：**
5. 一键还原迁移
6. 智能推荐可迁移目录
7. 链接完整性检查
8. 临时文件清理

### 2.2 核心功能详细设计

#### 2.2.1 磁盘扫描

**功能描述：**
- 扫描指定磁盘（默认 C 盘）的所有文件和文件夹
- 计算实际磁盘占用（考虑簇大小、NTFS 压缩、稀疏文件）
- 识别符号链接、硬链接、Junction
- 处理权限受限的文件

**技术实现：**
- 使用 Windows API（FindFirstFileW/FindNextFileW）直接遍历文件系统
- 并发扫描多个目录（tokio 异步运行时）
- 以管理员权限运行，访问系统保护文件
- 实时推送扫描进度到前端

**性能目标：**
- 扫描速度：每秒 10,000-20,000 个文件
- C 盘（500GB，50 万文件）：30-60 秒完成
- 内存占用：< 200MB

#### 2.2.2 空间可视化

**功能描述：**
- 树状图：展示目录层级和占用比例
- 饼图：展示顶级目录占用分布
- 列表视图：按大小排序的文件/文件夹列表

**技术实现：**
- 前端使用 ECharts 渲染图表
- 支持交互式钻取（点击目录进入子目录）
- 颜色编码：不同类型文件用不同颜色

#### 2.2.3 文件迁移

**功能描述：**
- 用户选择要迁移的文件/文件夹
- 选择目标磁盘
- 自动创建符号链接
- 验证迁移成功

**符号链接策略：**
- **目录**：优先使用 Junction（不需要管理员权限）
- **文件**：使用 Symlink（需要管理员权限）
- **同分区**：使用 Hard Link（文件）

**迁移流程：**
1. 检查目标盘空间是否充足
2. 检查文件是否被占用
3. 复制文件到目标位置
4. 验证复制完整性（校验文件大小）
5. 创建符号链接
6. 验证链接有效性
7. 删除原文件
8. 记录迁移信息到数据库

**失败处理：**
- 任何步骤失败，自动回滚
- 保留原文件直到链接验证成功
- 记录失败原因和详细日志

#### 2.2.4 迁移记录

**功能描述：**
- 记录所有迁移操作
- 支持查询历史记录
- 支持一键还原（第二阶段）

**数据结构：**
```sql
CREATE TABLE migrations (
    id INTEGER PRIMARY KEY,
    source_path TEXT NOT NULL,
    target_path TEXT NOT NULL,
    link_type TEXT NOT NULL,  -- 'symlink', 'junction', 'hardlink'
    file_size INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    status TEXT NOT NULL  -- 'active', 'restored', 'broken'
);
```

---

## 3. 技术架构

### 3.1 技术栈

**前端：**
- Vue 3 + Vite
- TypeScript
- Element Plus（UI 组件库）
- ECharts（数据可视化）
- TailwindCSS（样式）

**后端：**
- Rust 1.70+
- Tauri 1.5+（桌面应用框架）
- tokio（异步运行时）
- windows-rs（Windows API）
- walkdir / jwalk（文件遍历）
- serde（序列化）
- rusqlite（SQLite 数据库）

**CLI：**
- clap（命令行解析）
- indicatif（进度条）
- colored（彩色输出）

### 3.2 架构设计

```
┌─────────────────────────────────────────────────────────┐
│                      GUI (Vue 3)                        │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐             │
│  │ 扫描视图 │  │ 可视化   │  │ 迁移管理 │             │
│  └──────────┘  └──────────┘  └──────────┘             │
└─────────────────────────────────────────────────────────┘
                         │
                    Tauri IPC
                         │
┌─────────────────────────────────────────────────────────┐
│                   Rust Backend                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ │
│  │ 扫描引擎     │  │ 迁移引擎     │  │ 数据库管理   │ │
│  │ - 并发扫描   │  │ - 文件复制   │  │ - SQLite     │ │
│  │ - 权限处理   │  │ - 链接创建   │  │ - 记录查询   │ │
│  │ - 进度推送   │  │ - 完整性验证 │  │              │ │
│  └──────────────┘  └──────────────┘  └──────────────┘ │
└─────────────────────────────────────────────────────────┘
                         │
                   Windows API
                         │
┌─────────────────────────────────────────────────────────┐
│                   File System                           │
└─────────────────────────────────────────────────────────┘
```

### 3.3 前后端通信接口

**Tauri Commands（前端调用后端）：**

```rust
// 扫描磁盘
#[tauri::command]
async fn scan_disk(path: String) -> Result<ScanResult, String>

// 获取目录详情
#[tauri::command]
async fn get_directory_info(path: String) -> Result<DirectoryInfo, String>

// 迁移文件
#[tauri::command]
async fn migrate_file(
    source: String,
    target_disk: String,
    link_type: LinkType
) -> Result<MigrationResult, String>

// 获取迁移历史
#[tauri::command]
async fn get_migration_history() -> Result<Vec<Migration>, String>

// 验证链接完整性
#[tauri::command]
async fn verify_links() -> Result<VerifyResult, String>
```

**Events（后端推送到前端）：**

```rust
// 扫描进度
emit("scan-progress", {
    scanned_files: 12345,
    total_size: 1024000000,
    current_path: "C:\\Users\\..."
})

// 迁移进度
emit("migration-progress", {
    current_file: "file.txt",
    progress: 0.75,
    status: "copying"
})
```

### 3.4 数据结构

**扫描结果：**
```typescript
interface ScanResult {
    root_path: string;
    total_size: number;
    total_files: number;
    total_dirs: number;
    scan_duration_ms: number;
    directories: DirectoryNode[];
}

interface DirectoryNode {
    path: string;
    name: string;
    size: number;
    file_count: number;
    children: DirectoryNode[];
    is_symlink: boolean;
}
```

**迁移记录：**
```typescript
interface Migration {
    id: number;
    source_path: string;
    target_path: string;
    link_type: 'symlink' | 'junction' | 'hardlink';
    file_size: number;
    created_at: string;
    status: 'active' | 'restored' | 'broken';
}
```

---

## 4. CLI 设计

### 4.1 命令结构

```bash
cdrive-cleaner [COMMAND] [OPTIONS]

Commands:
  scan      扫描磁盘
  migrate   迁移文件
  list      列出迁移记录
  verify    验证链接完整性
  restore   还原迁移（第二阶段）
  help      显示帮助信息
```

### 4.2 命令示例

**扫描磁盘：**
```bash
# 扫描 C 盘
cdrive-cleaner scan C:\

# 扫描并输出 JSON
cdrive-cleaner scan C:\ --format json > scan.json

# 只显示大于 1GB 的目录
cdrive-cleaner scan C:\ --min-size 1GB
```

**迁移文件：**
```bash
# 迁移目录到 D 盘
cdrive-cleaner migrate "C:\Users\用户\Videos" D:\

# 指定链接类型
cdrive-cleaner migrate "C:\Program Files\App" D:\ --link-type junction

# 批量迁移（从文件读取）
cdrive-cleaner migrate --batch migrate-list.txt
```

**查看记录：**
```bash
# 列出所有迁移记录
cdrive-cleaner list

# 输出 JSON 格式
cdrive-cleaner list --format json
```

---

## 5. 性能优化策略

### 5.1 扫描性能

1. **并发扫描**
   - 使用 tokio 异步运行时
   - 同时扫描多个顶级目录
   - 动态调整并发数（根据 CPU 核心数）

2. **零拷贝 IO**
   - 只读取文件元数据，不读取文件内容
   - 使用 Windows API 批量获取文件信息

3. **内存优化**
   - 流式处理，不一次性加载所有文件到内存
   - 大目录分批处理

### 5.2 迁移性能

1. **智能复制**
   - 使用 Windows CopyFileEx API（支持进度回调）
   - 大文件分块复制，实时反馈进度

2. **并发限制**
   - 同时迁移多个小文件
   - 大文件单独处理（避免 IO 竞争）

---

## 6. 安全性设计

### 6.1 权限管理

- 启动时请求管理员权限（UAC 提示）
- 明确告知用户为什么需要管理员权限
- 权限不足时，标注无法访问的文件

### 6.2 数据安全

- 迁移前验证目标盘空间
- 复制完成后验证文件完整性
- 失败自动回滚，不删除原文件
- 所有操作记录到日志文件

### 6.3 用户确认

- 迁移前显示详细信息（源路径、目标路径、大小）
- 危险操作（如迁移系统目录）需要二次确认
- 提供"模拟运行"模式（只显示会做什么，不实际执行）

---

## 7. 用户界面设计

### 7.1 GUI 主要页面

**1. 扫描页面**
- 选择要扫描的磁盘
- 显示扫描进度（进度条 + 当前路径）
- 扫描完成后跳转到可视化页面

**2. 可视化页面**
- 左侧：树状图（ECharts Treemap）
- 右侧：目录列表（按大小排序）
- 顶部：饼图（顶级目录占用）
- 支持点击钻取、搜索、过滤

**3. 迁移管理页面**
- 选择要迁移的文件/文件夹
- 选择目标磁盘
- 配置链接类型（自动/手动）
- 显示迁移进度
- 查看迁移历史

**4. 设置页面**
- 扫描选项（排除目录、最小文件大小）
- 迁移选项（默认目标盘、链接类型）
- 日志查看

### 7.2 CLI 输出示例

```
正在扫描 C:\ ...
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 75% (375,234/500,000 文件)
当前: C:\Users\用户\AppData\Local\...

扫描完成！
总大小: 245.6 GB
文件数: 487,234
目录数: 52,108
耗时: 42 秒

最大的 10 个目录:
  1. C:\Users\用户\AppData\Local\      85.2 GB
  2. C:\Program Files\                 52.3 GB
  3. C:\Windows\                       38.7 GB
  ...
```

---

## 8. 项目结构

### 8.1 目录结构

```
cdrive-cleaner/
├── src-tauri/              # Rust 后端
│   ├── src/
│   │   ├── main.rs         # 入口文件
│   │   ├── scanner/        # 扫描引擎模块
│   │   │   ├── mod.rs
│   │   │   ├── disk_scanner.rs
│   │   │   └── file_info.rs
│   │   ├── migration/      # 迁移引擎模块
│   │   │   ├── mod.rs
│   │   │   ├── file_migrator.rs
│   │   │   └── link_creator.rs
│   │   ├── database/       # 数据库模块
│   │   │   ├── mod.rs
│   │   │   └── migrations.rs
│   │   ├── commands.rs     # Tauri Commands
│   │   └── utils.rs        # 工具函数
│   ├── Cargo.toml          # Rust 依赖
│   └── tauri.conf.json     # Tauri 配置
│
├── src/                    # Vue 前端
│   ├── main.ts
│   ├── App.vue
│   ├── views/              # 页面组件
│   │   ├── ScanView.vue
│   │   ├── VisualizationView.vue
│   │   ├── MigrationView.vue
│   │   └── SettingsView.vue
│   ├── components/         # 通用组件
│   │   ├── DiskChart.vue
│   │   ├── FileTree.vue
│   │   └── ProgressBar.vue
│   ├── composables/        # Vue Composables
│   │   ├── useScan.ts
│   │   └── useMigration.ts
│   ├── types/              # TypeScript 类型
│   │   └── api.ts
│   └── utils/              # 工具函数
│       └── format.ts
│
├── cli/                    # CLI 工具
│   ├── src/
│   │   ├── main.rs
│   │   └── commands/
│   └── Cargo.toml
│
├── docs/                   # 文档
│   ├── plans/
│   │   └── 2026-03-02-cdrive-cleaner-design.md
│   ├── api.md
│   └── development.md      # 开发指南（待创建）
│
├── tests/                  # 测试
│   ├── unit/
│   └── integration/
│
├── .kiro/                  # Kiro 配置
│   └── skills/
│
├── package.json            # Node.js 依赖
├── vite.config.ts          # Vite 配置
├── tsconfig.json           # TypeScript 配置
├── tailwind.config.js      # TailwindCSS 配置
└── README.md               # 项目说明
```

### 8.2 模块职责

**扫描引擎（scanner）：**
- `disk_scanner.rs` - 磁盘扫描核心逻辑
- `file_info.rs` - 文件信息获取和处理

**迁移引擎（migration）：**
- `file_migrator.rs` - 文件复制和迁移
- `link_creator.rs` - 符号链接创建和验证

**数据库（database）：**
- `migrations.rs` - 迁移记录的 CRUD 操作

**前端（src）：**
- `views/` - 页面级组件
- `components/` - 可复用组件
- `composables/` - Vue 3 组合式 API 逻辑
- `types/` - TypeScript 类型定义

---

## 9. 开发环境配置

### 9.1 Windows 系统依赖（必需）

**步骤 1：安装 Microsoft C++ Build Tools**
1. 下载 [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
2. 运行安装程序
3. 在安装选项中勾选 "Desktop development with C++"
4. 完成安装

**步骤 2：安装 WebView2**
1. 访问 [WebView2 Runtime 下载页面](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)
2. 下载 "Evergreen Bootstrapper"
3. 运行安装（Windows 10/11 通常已预装）

**步骤 3：检查 VBSCRIPT（构建 MSI 需要）**
- 打开 设置 → 应用 → 可选功能 → 更多 Windows 功能
- 确保 VBSCRIPT 已勾选
- 如果未勾选，勾选后重启电脑

### 9.2 安装 Rust

**Windows 安装：**
1. 访问 https://rustup.rs/
2. 下载并运行 `rustup-init.exe`
3. 选择默认安装（MSVC toolchain）
4. 重启终端

**验证安装：**
```bash
rustc --version
# rustc 1.75.0 (或更高版本)

cargo --version
# cargo 1.75.0 (或更高版本)
```

### 9.3 安装 Node.js

**Windows 安装：**
1. 访问 https://nodejs.org/
2. 下载 LTS 版本（推荐 20.x）
3. 运行安装程序
4. 重启终端

**验证安装：**
```bash
node --version
# v20.10.0 (或更高版本)

npm --version
# 10.2.3 (或更高版本)
```

### 9.4 创建项目

**使用 create-tauri-app（推荐）：**

```powershell
# PowerShell 命令
irm https://create.tauri.app/ps | iex

# 或者使用 npm
npm create tauri-app@latest
```

**交互式配置：**
```
? Project name › cdrive-cleaner
? Identifier › com.cdrive.cleaner
? Choose which language to use for your frontend › TypeScript / JavaScript
? Choose your package manager › npm
? Choose your UI template › Vue
? Choose your UI flavor › TypeScript
```

**项目创建完成后：**
```bash
cd cdrive-cleaner
npm install

# 安装额外的前端依赖
npm install element-plus echarts
npm install -D tailwindcss postcss autoprefixer

# 初始化 TailwindCSS
npx tailwindcss init -p

# 开发模式运行
npm run tauri dev

# 构建生产版本
npm run tauri build
```

### 9.5 Rust 依赖配置

在 `src-tauri/Cargo.toml` 中添加依赖：

```toml
[dependencies]
tauri = { version = "2.0", features = ["shell-open"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.35", features = ["full"] }
windows = { version = "0.52", features = [
    "Win32_Storage_FileSystem",
    "Win32_Foundation",
    "Win32_System_IO"
]}
walkdir = "2.4"
rusqlite = { version = "0.31", features = ["bundled"] }
chrono = "0.4"
anyhow = "1.0"

# CLI 依赖（如果需要独立 CLI）
clap = { version = "4.4", features = ["derive"] }
indicatif = "0.17"
colored = "2.1"
```

### 9.6 常见问题

**问题 1：Rust 编译失败**
- 确保已安装 Microsoft C++ Build Tools
- 确保选择了 MSVC toolchain（不是 GNU）
- 运行 `rustup default stable-msvc`

**问题 2：WebView2 未找到**
- 手动下载安装 WebView2 Runtime
- 重启电脑

**问题 3：npm run tauri dev 失败**
- 检查 Node.js 和 Rust 是否正确安装
- 删除 `node_modules` 和 `target` 目录，重新安装
- 查看错误日志，搜索具体错误信息

---

## 10. 测试策略

### 10.1 单元测试

- 文件扫描逻辑
- 符号链接创建/验证
- 数据库操作
- 文件大小计算

### 10.2 集成测试

- 完整的扫描流程
- 完整的迁移流程
- 失败回滚机制
- 前后端通信

### 10.3 性能测试

- 扫描速度基准测试
- 不同文件数量下的性能
- 内存占用测试
- 并发扫描压力测试

### 10.4 兼容性测试

- Windows 10/11
- 不同文件系统（NTFS、ReFS）
- 不同权限场景
- 网络驱动器

---

## 9. 项目里程碑

### 9.1 第一阶段（MVP）- 4 周

**Week 1：基础架构**
- 搭建 Tauri + Vue 3 项目
- 实现基础的前后端通信
- 完成 CLI 框架

**Week 2：扫描功能**
- 实现磁盘扫描引擎
- 并发优化
- 前端进度显示

**Week 3：可视化**
- ECharts 集成
- 树状图、饼图实现
- 交互式钻取

**Week 4：迁移功能**
- 文件迁移引擎
- 符号链接创建
- 数据库记录

### 9.2 第二阶段（增强）- 2 周

**Week 5：高级功能**
- 一键还原
- 智能推荐
- 链接完整性检查

**Week 6：优化和测试**
- 性能优化
- 完整测试
- 文档完善

---

## 10. 风险和挑战

### 10.1 技术风险

**风险 1：权限问题**
- 某些系统文件即使管理员也无法访问
- 缓解：明确标注无法访问的文件，不影响其他功能

**风险 2：符号链接兼容性**
- 某些老旧程序可能不支持符号链接
- 缓解：提供"兼容模式"警告，让用户决定

**风险 3：性能达不到目标**
- 扫描速度可能慢于预期
- 缓解：持续优化，参考开源项目（dua-cli）

### 10.2 用户体验风险

**风险 1：误操作导致数据丢失**
- 缓解：多重确认、模拟运行、自动备份

**风险 2：界面复杂难用**
- 缓解：用户测试、迭代优化

---

## 11. 开源计划

### 11.1 开源准备

- 完善 README（功能介绍、安装指南、使用说明）
- 添加 LICENSE（MIT 或 Apache 2.0）
- 编写 CONTRIBUTING.md
- 设置 GitHub Actions（自动构建、测试）

### 11.2 社区建设

- 创建 Issue 模板
- 设置 Discussions
- 编写详细的开发文档
- 录制演示视频

---

## 12. 总结

这是一个技术上具有挑战性但非常实用的项目。通过使用 Rust + Tauri 的技术栈，我们可以实现：

✅ 接近原生工具的性能（30-60 秒扫描 C 盘）  
✅ 完整的文件信息获取（99%+ 准确度）  
✅ 安全的文件迁移（符号链接 + 完整性验证）  
✅ 友好的用户界面（CLI + GUI）  
✅ 小巧的打包体积（3-5MB）  

核心竞争力在于"智能迁移 + 符号链接"，这是 Windows 自带工具做不到的。
