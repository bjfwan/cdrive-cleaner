# CSD · 磁盘空间管理

> **CSD (C-Drive Saver)** —— Windows 桌面端磁盘空间分析与目录迁移工具。
> 基于 **Tauri 2 + Vue 3 + Rust**，原生 NTFS MFT/USN 加速，支持迁移占位（Junction / Symlink / Hardlink）与全量回滚。

[![CI](https://github.com/bjfwan/cdrive-cleaner/actions/workflows/ci.yml/badge.svg)](https://github.com/bjfwan/cdrive-cleaner/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/bjfwan/cdrive-cleaner?include_prereleases)](https://github.com/bjfwan/cdrive-cleaner/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6)

---

## 这是什么

C 盘越用越满，但又分不清空间到底被谁吃掉、哪些能搬、哪些不能动。CSD 把这条路从扫描到迁移到回滚做成一个可视化、可逆、不踩坑的流程：

- **看清楚**：树状图 + 列表 + 大文件视图，下钻到任意一层目录看体积构成
- **搬得动**：把目录挪到另一块盘，原位置留 Junction，应用零感知地继续用
- **不会乱**：迁移前先做安全检测，系统关键目录直接拒绝；每一次迁移都有完整历史，一键回滚到原状
- **够快**：管理员模式下走 NTFS MFT + USN，几十万文件秒级出图；普通权限自动回退到原生枚举，功能不打折

---

## 核心功能

### 磁盘扫描

| 后端 | 触发条件 | 性能 |
|---|---|---|
| **MFT + USN** (`mft_usn`) | NTFS 卷 + 管理员权限 + 系统提供 MFT 访问 | 几十万文件 ≤ 1 秒，靠 USN journal 做增量 |
| **原生枚举** (`native`) | 非 NTFS / 标准权限 / MFT 不可用 | `FindFirstFileExW` 递归 + jwalk 并行 |

- 后端选择由 `scanner/backend.rs` 自动判断，前端通过 `get_scan_capabilities` 命令查询当前环境能用哪条路径
- 第二次扫同一个盘走增量：读缓存里的 root FRN + USN checkpoint，只重扫 USN journal 里变动过的子树
- 扫描结果落 SQLite 缓存，应用重开即取即用

### 空间分析

- **Treemap**：按目录大小堆面积，一眼看到 C 盘谁最胖
- **List**：表格视图，按大小 / 名称 / 修改时间排序
- **Large Files**：Top-N 大文件视图，跨目录直接挑出最沉的几个
- **Smart Groups**：识别"可清理 / 可迁移 / 系统关键"分组，给迁移决策做提示
- **Duplicates**：基于 size + xxhash 的重复文件检测，可批量删
- **Space Trend**：每次扫描记录磁盘快照，画 90 天空间走势图

### 目录迁移

每次迁移走 5 步：

```
copy → verify → backup → create-link → cleanup
```

- 链接类型默认 `auto`，规则：可跨盘 + 是目录 → Junction；不可跨盘 → Hardlink；其它 → Symlink。也可以在 `MigrateDialog` 里手动指定
- `CopyFile2` 走 unbuffered IO，超过 128 MiB 的大文件单独优化；目录树并行复制（Rayon）
- 整个过程发 `migration-progress` 事件，前端实时显示进度条 + 当前文件
- 迁移成功后写一条 `migrations` 记录，包含 source / target / link_type / size / created_at

### 安全检测

迁移前先跑 `analyze_migration_safety`：

| Verdict | 含义 | 行为 |
|---|---|---|
| `safe` | 普通用户数据 / 应用缓存 | 直接放行 |
| `safe_after_action` | 需要先关进程 / 拿权限 | UI 提示用户处理 |
| `blocked` | 高风险（活跃应用、reparse point 链路过深等） | 默认拒绝，可手动覆写 |
| `system_critical` | Windows 系统关键目录（`C:\Windows`、`C:\Program Files`、`C:\Users` 这种集合根） | 硬拒绝，不可覆写 |

检测模块用并行 gate 串：路径白名单 → 应用类型识别 → reparse 深度 → 进程占用 → 注册表关键键。结果带 30 秒缓存，避免在迁移弹窗反复弹关时重复 IO。

### 历史与回滚

- 全量记录每次迁移到 `migrations.db` (SQLite + WAL)
- 历史页可按状态 / 时间 / 大小过滤，查统计（总迁移数、节省空间）
- 回滚 = 复制 target → source、删除 target、删 link 占位，状态置 `rolled_back`
- 回滚也是 5 步事务，中途失败可重试

### 游戏库迁移（v0.1.2+）

独立 Tab，免去手工找路径：

- **Steam**：注册表读 Steam 路径 → 解析 `libraryfolders.vdf` → 扫每个库下的 `appmanifest_*.acf` 列已装游戏
- **Epic**：解析 `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item`
- **Game Pass / WindowsApps**：扫 `C:\XboxGames\` 标可迁移项；`WindowsApps` 下受 TrustedInstaller 保护的游戏标记为不可迁移，按钮跳转 `ms-settings:appsfeatures` 走系统原生迁移
- 迁移走 Junction，启动器可直接启动，迁移记录里 `link_type` 为 `game:Steam:<appid>` 形式

---

## 系统要求

- **OS**：Windows 10 (1809+) / Windows 11，仅 x64
- **WebView2 Runtime**：Windows 11 自带，Windows 10 缺失时 NSIS 安装器会自动拉取
- **管理员权限**（推荐）：开启后启用 MFT + USN，扫盘性能 10-100 倍提升

非 NTFS 卷 / 标准权限会自动回退到原生枚举，扫描慢一些但功能不变。

---

## 安装

到 [Releases 页面](https://github.com/bjfwan/cdrive-cleaner/releases) 下载最新版：

| 文件 | 类型 | 适用场景 |
|---|---|---|
| `CSD_<version>_x64-setup.exe` | NSIS 安装器 | 普通用户首选，体积小、自带 WebView2 拉取 |
| `CSD_<version>_x64_zh-CN.msi` | MSI 安装包 | 企业批量部署、Group Policy 管控 |

安装完会在开始菜单出现 `CSD - 磁盘空间管理`。首次启动若提示"开启管理员模式"，建议同意以启用 MFT + USN 加速。

---

## 上手使用

1. 启动 CSD，选择要扫描的盘（默认 `C:`）
2. 点 **开始扫描**。如果当前是标准权限且卷是 NTFS，扫描卡上会有"开启管理员模式"按钮，点一下走 UAC 后自动重启进入加速模式
3. 扫描完后默认进 Treemap 视图，鼠标悬停看大小、双击下钻
4. 看到想搬的目录，右键 → **迁移**，弹窗会先跑安全检测：
   - 绿色 `safe` → 选目标盘，确认
   - 黄色 `blocked` → 看 findings 提示，自己判断是否覆写
   - 红色 `system_critical` → 直接灰掉，不能搬
5. 迁移过程中可以做别的事，进度条会实时更新
6. 不满意？进 **历史** Tab，点 **回滚**，秒级恢复

> 第一次跑扫描会冷启动慢一些（NTFS MFT 初始化）；第二次起会走增量，`scan_incremental` 只重扫 USN 变更的子树。

---

## 从源码构建

完整开发指南见 [`docs/development.md`](docs/development.md)。简版：

```powershell
# 一次性环境
# 1. Rust stable + x86_64-pc-windows-msvc
# 2. Visual Studio 2022 Build Tools (C++ + Windows 11 SDK)
# 3. Node.js 18+ / npm 9+
# 4. WebView2 Runtime（Win 11 自带）

# 拉代码 + 装依赖
git clone https://github.com/bjfwan/cdrive-cleaner.git
cd cdrive-cleaner
npm install

# 调试模式（前端热更新 + Rust 重新编译）
npm run tauri dev

# 出安装包（产物在 src-tauri/target/release/bundle/）
npm run tauri build
```

发版流程见 [`docs/release.md`](docs/release.md)。

---

## 项目结构

```
.
├── src/                          Vue 3 前端（TypeScript + Composition API）
│   ├── App.vue                   根组件
│   ├── components/               业务组件（Workspace、ScanResults、MigrateDialog…）
│   │   └── icons/                自定义 SVG 图标库
│   ├── composables/              组合式逻辑（useCart、useToast、useCountUp）
│   ├── styles/                   全局样式
│   ├── types/                    TypeScript 类型定义（与 Rust 端 serde 字段对齐）
│   └── utils/                    格式化、设置存储
│
├── src-tauri/                    Tauri / Rust 后端
│   ├── src/
│   │   ├── lib.rs                Tauri 入口、命令注册、tracing 初始化
│   │   ├── commands.rs           暴露给前端的所有 invoke 命令
│   │   ├── scanner/              扫描子系统
│   │   │   ├── backend.rs        后端选择（mft_usn vs native）
│   │   │   ├── disk_scanner.rs   编排 + 进度发射
│   │   │   ├── mft_usn.rs        NTFS MFT 全量 + USN 增量
│   │   │   ├── incremental.rs    缓存合并 + 增量重扫
│   │   │   ├── duplicates.rs     重复文件检测（xxhash）
│   │   │   ├── smart_scan.rs     智能分组分析
│   │   │   └── progress.rs       进度数据结构（GUI 解耦）
│   │   ├── migration/            迁移子系统
│   │   │   ├── file_migrator.rs  5 步迁移流程 + CopyFile2
│   │   │   ├── link_creator.rs   Junction / Symlink / Hardlink 创建
│   │   │   └── delete.rs         回收站 / 永久删除
│   │   ├── safety/               安全检测
│   │   │   └── detector.rs       并行 gate 串 + 30 秒缓存
│   │   ├── games/                游戏库探测
│   │   │   ├── steam.rs          注册表 + libraryfolders.vdf
│   │   │   ├── epic.rs           manifests/*.item 解析
│   │   │   ├── gamepass.rs       XboxGames + WindowsApps 识别
│   │   │   └── vdf.rs            自带 KeyValues 解析
│   │   ├── database/             SQLite 持久化
│   │   │   ├── migrations.rs     迁移历史 + 删除记录
│   │   │   ├── scan_cache_db.rs  扫描结果缓存
│   │   │   └── space_history.rs  磁盘空间快照（90 天）
│   │   ├── winfs.rs              Windows 文件系统底层（FSCTL_*）
│   │   ├── diagnostics.rs        CLI 验证工具入口
│   │   └── bench.rs              性能基线采集（feature gated）
│   ├── tests/                    集成测试（21+ 个）
│   ├── examples/                 独立 CLI 工具（admin_mft_validation 等）
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── docs/
│   ├── api.md                    Tauri 命令 + 事件 + 类型契约
│   ├── development.md            开发环境 + 调试 + 日志
│   ├── release.md                打包 + 发版 + 故障速查
│   ├── mft-usn-backend.md        NTFS 后端架构深度文档
│   ├── bench/                    基准测试报告归档
│   └── changelog/                版本变更日志
│
├── scripts/                      PowerShell 验证 / 修复脚本
│   ├── run-admin-mft-validation.ps1
│   ├── measure-reparse-depth.ps1
│   └── fetch-missing-crates.ps1
│
└── .github/workflows/
    ├── ci.yml                    push/PR 自动 build + test
    └── release.yml               push tag 自动出 MSI/NSIS 包
```

---

## 关键技术栈

| 层 | 技术 | 版本 |
|---|---|---|
| 前端 | Vue 3 + Composition API | 3.5.13 |
|  | TypeScript | 5.6 |
|  | Vite | 6.0 |
|  | Element Plus | 2.13 |
|  | ECharts + vue-echarts | 6.0 / 8.0 |
| 桌面壳 | Tauri | 2.11 |
|  | WebView2 (Chromium) | 系统提供 |
| 后端 | Rust (edition 2021) | stable msvc |
|  | tokio | 1.35 |
|  | rayon | 1.8 |
|  | jwalk | 0.8 |
|  | rusqlite (bundled) | 0.31 |
|  | windows-rs | 0.52 |
|  | tracing | 0.1 |
|  | twox-hash | 2.x |
|  | trash | 5.x |
|  | junction | 1.0 |

---

## 性能基线

修复进度线程硬延迟之后的最新数据（详见 [`docs/bench/README.md`](docs/bench/README.md)）：

| 场景 | 平均耗时 | 吞吐 |
|---|---|---|
| 30 文件浅目录 | 21 ms | — |
| 1 k 文件平衡树 | 43 ms | ~23 k files/s |
| 10 k 文件真实分布 | 44 ms | ~227 k files/s |
| 15 k 真实 `node_modules` | 164 ms | ~91 k files/s |
| 端到端（扫描 → 迁移 → 回滚 30 文件） | 82 ms | — |

数据库写入 ≤ 0.1 ms / 条；查 1000 条迁移历史 ~1.3 ms。

---

## 开发与贡献

- 问题反馈 / 功能请求：开 [Issue](https://github.com/bjfwan/cdrive-cleaner/issues)
- PR 流程：fork → 新分支 → 跑 `npm run build` + `cargo test --tests` → 提 PR
- 代码风格：前端按 Vue 3 SFC + `<script setup lang="ts">`；Rust 按 `cargo fmt`；命令命名与现有保持一致（snake_case + 动词前缀）
- 不打算加跨平台支持（macOS / Linux），所有路径假设为 Windows 文件系统

---

## License

[MIT License](LICENSE) © 2026 bjfwan

