# 开发指南

CSD 是 Tauri 2 + Vue 3 + Rust 的 Windows 桌面应用，本文覆盖：本地搭环境、跑调试、跑测试、写日志、用诊断工具、做 benchmark。

> 找发版相关流程请看 [`release.md`](release.md)。
> 找前后端 IPC 契约请看 [`api.md`](api.md)。
> 找扫描后端架构细节请看 [`mft-usn-backend.md`](mft-usn-backend.md)。

---

## 1. 环境搭建

### 1.1 必备工具

| 组件 | 版本 | 用途 |
|---|---|---|
| **Rust stable** + `x86_64-pc-windows-msvc` | 1.78+ | 编 `cdrive-cleaner_lib` Rust 后端 |
| **Visual Studio 2022 Build Tools** | 17.x | C++ 工作负载 + Windows 11 SDK，提供 MSVC 链接器 |
| **Node.js** | 18 LTS / 20 LTS | 跑 `vue-tsc`、`vite` |
| **npm** | 9+ | `package-lock.json` 锁定 |
| **WebView2 Runtime** | 任意（Win11 自带） | Tauri 窗口宿主，相当于嵌入版 Edge |

### 1.2 自定义 Rust 路径

如果 Rust 不在默认 `~\.rustup`，每个新 PowerShell 会话先注入 PATH：

```powershell
$env:RUSTUP_HOME = 'D:\DevTools\Rust\rustup'
$env:CARGO_HOME  = 'D:\DevTools\Rust\cargo'
$env:PATH        = 'D:\DevTools\Rust\cargo\bin;' + $env:PATH
```

可以放进 `$PROFILE` 自动生效。

### 1.3 拉代码 + 装依赖

```powershell
git clone https://github.com/bjfwan/cdrive-cleaner.git
cd cdrive-cleaner
npm install
```

第一次 `npm install` 会下 `@tauri-apps/cli`，它会进一步下载 WiX Toolset 和 NSIS 到 `%LOCALAPPDATA%\tauri\` 下，下次构建直接用。

`@tauri-apps/api` 与 `@tauri-apps/cli` 必须跟 Rust crate `tauri` 的 minor 一致，详见 [`release.md` §0.3](release.md#03-tauri-包版本一致性)。

---

## 2. 启动开发模式

```powershell
npm run tauri dev
```

`tauri dev` 会同时干三件事：

1. 起 Vite dev server（默认 `http://localhost:1420`，`tauri.conf.json` 里写死）
2. `cargo build` 出 `cdrive-cleaner.exe`（debug profile）
3. 启动 Tauri 窗口加载 Vite 提供的 HTML

效果：

| 改动 | 行为 |
|---|---|
| 改 `src/` 下 `.vue` / `.ts` / `.css` | Vite HMR 热替换，毫秒级生效 |
| 改 `src-tauri/src/` 下 `.rs` | `cargo build` 重新编译，自动重启 Tauri 窗口 |
| 改 `src-tauri/Cargo.toml` 加依赖 | 同上，但首次编译会拉新 crate |
| 改 `tauri.conf.json` | 窗口配置 / 命令注册更新，自动重启 |

只想跑前端预览（不开 Tauri 窗口、所有 `invoke` 都报 `__TAURI_IPC__ is undefined`）：

```powershell
npm run dev
```

仅做样式 / 静态布局的快迭代时用得上。

---

## 3. 关键命令

### 前端

```powershell
npm run dev        # 仅 Vite，无 Tauri 窗口
npm run build      # vue-tsc 类型检查 + Vite 出 dist/
npm run preview    # 静态 serve dist/，验证生产产物
npm run tauri dev  # Tauri 调试（前端 HMR + Rust 重编）
npm run tauri build # 发布构建（详见 release.md）
```

### 后端

```powershell
cd src-tauri

# 类型检查（不出二进制，最快）
cargo check

# 单元 + 集成测试（21+ 个 in tests/）
cargo test --tests --no-fail-fast

# 单跑某个测试文件
cargo test --test scanner_test

# 单跑某个测试函数
cargo test test_safety_blocks_program_files

# 编 release exe（不打安装包，最小验证发布构建能过）
cargo build --release

# 跑诊断工具（见 §6）
cargo run --example admin_mft_validation -- C:\
cargo run --example deep_scan_benchmark   -- C:\Users\me\Desktop 50000

# 跑 benchmark suite（详见 docs/bench/README.md）
cargo test --features bench --test bench_runner -- --ignored --nocapture
```

---

## 4. 项目结构速览

详细版在 `README.md`。这里只列开发时高频要找的文件。

```
src-tauri/src/
├── lib.rs                 invoke 命令注册（要加新命令时改这里）
├── commands.rs            所有 Tauri 命令的实现入口
├── scanner/
│   ├── disk_scanner.rs    扫描总编排
│   ├── backend.rs         决定走 mft_usn 还是 native
│   ├── mft_usn.rs         NTFS MFT 全量 + USN 增量
│   ├── incremental.rs     缓存合并 / 增量重扫 / 三阶段合并
│   ├── duplicates.rs      重复文件检测
│   └── smart_scan.rs      智能分组（按 reparse / 应用安装 / 用户数据）
├── migration/
│   ├── file_migrator.rs   5 步迁移流程 (copy → verify → backup → link → cleanup)
│   ├── link_creator.rs    Junction / Symlink / Hardlink
│   └── delete.rs          回收站 / 永久删除
├── safety/detector.rs     并行 gate 串 + 30 秒缓存
├── games/                 Steam / Epic / GamePass 探测
├── database/              SQLite 持久化（migrations / scan_cache / space_history）
├── winfs.rs               Windows 文件系统底层 FFI（FSCTL_*、OpenFileById）
└── diagnostics.rs         CLI 验证工具入口

src/
├── App.vue                根组件（路由 / 全局 Toast）
├── components/Workspace.vue 主工作区
├── components/MigrateDialog.vue 迁移弹窗（含安全检测渲染）
├── composables/           Composition API 复用逻辑
└── types/index.ts         前端 TS 类型，与 Rust serde 字段一一对齐
```

---

## 5. 调试 + 日志

### 5.1 后端日志（tracing）

后端用 [`tracing`](https://docs.rs/tracing/) + `tracing-subscriber`，默认 `info` 级别打到 stdout（`npm run tauri dev` 的控制台）。

通过 `RUST_LOG` 环境变量调整：

```powershell
# 默认 info：只看流程关键节点
$env:RUST_LOG = 'info'
npm run tauri dev

# debug：含详细计时（[scan-timing]、[migration-core]、[winfs-usn] 等）
$env:RUST_LOG = 'debug'
npm run tauri dev

# 仅本 crate 开 debug，其它依赖保持 warn
$env:RUST_LOG = 'cdrive_cleaner_lib=debug,warn'

# 关掉日志
$env:RUST_LOG = 'off'
```

PowerShell 里 `$env:RUST_LOG` 只对当前会话生效；重开窗口要重新设。

### 5.2 关键日志前缀

各模块都用方括号前缀做 grep 友好型分组：

| 前缀 | 模块 | 何时出现 |
|---|---|---|
| `[scan-deep]` | `commands::scan_disk_deep` | 深度扫描入口决策 |
| `[scan-cache]` | `commands.rs` 持久化 | 后台保存缓存的成败 |
| `[scan-timing]` | 多模块 debug 计时 | 阶段耗时 / IO 计数（仅 debug 级别） |
| `[mft-usn]` | `scanner/mft_usn.rs` | MFT 枚举 / 回退原因 |
| `[winfs-usn]` | `winfs.rs` | USN journal 读取细节 |
| `[阶段0]` / `[阶段1]` / `[阶段2]` / `[阶段3]` | `scanner/incremental.rs` | 增量扫的四阶段切换点 |
| `[migration]` / `[migration-core]` | `commands::migrate_file` / `migration/file_migrator.rs` | 迁移命令 / 5 步执行 |
| `[delete]` | `commands::delete_path` | 删除请求与结果 |
| `[games]` / `[games-epic]` / `[games-migrate]` | `games/`、`commands::migrate_game` | 平台检测 / 游戏迁移 |
| `[space-history]` | `database/space_history.rs` | 磁盘空间快照写入失败 |

debug 级别会额外看到 `[scan-timing]`、阶段耗时 (`took xx.xx ms`)、漏算计数 (`metadata path fallback hits`) 等性能 profile 信息。

### 5.3 前端调试

Tauri 窗口在 dev 模式下默认右键 → "检查"打开 Chromium DevTools，跟普通网页一样调试 Vue。`console.log` / `Vue Devtools` 都能用。

`invoke` 调用 Rust 端命令时如果出错，错误会以字符串形式 reject，建议在 composables 里 try-catch 后通过 `useToast` 提示。

---

## 6. 诊断工具

`src-tauri/examples/` 下有两个独立 CLI，专门用来在不开 GUI 的情况下验证后端 / 跑性能。

### 6.1 `admin_mft_validation` —— MFT + USN 端到端验证

**做什么**：建一个临时目录、写几个文件、用 `mft_usn::scan_path` 扫一遍、再改一个文件、读 USN journal 验证变化捕获，结束自动清理。打印 JSON 报告。

**用什么权限**：必须管理员。

**两种用法**：

```powershell
# 方式 A：仓库根目录跑自提权脚本（推荐）
powershell -ExecutionPolicy Bypass -File scripts\run-admin-mft-validation.ps1 C:\

# 方式 B：已经在管理员 shell 里
cd src-tauri
cargo run --example admin_mft_validation -- C:\
```

退出码：

| Code | 含义 |
|---|---|
| `0` | 验证通过，报告 `passed: true` |
| `2` | 验证失败（环境问题或 USN 没捕获到变化），看报告 `notes` 字段定位 |

报告关键字段：

```json
{
  "passed": true,
  "scan_backend": "mft_usn",
  "total_files": 3,
  "total_dirs": 3,
  "usn_detected": true,
  "duration_ms": 412,
  "notes": [
    "MFT 枚举成功，统计结果与验证目录一致",
    "USN 变化集成功捕获到被修改的子目录",
    "验证临时目录已在退出时自动清理"
  ]
}
```

### 6.2 `deep_scan_benchmark` —— 深度扫描两轮基线

**做什么**：对指定目录跑两次 `scan_deep`，第一次冷扫（无缓存），第二次走增量 / 缓存命中。打印两轮的 `wall_duration_ms`、`reported_scan_duration_ms`、`scan_backend`、`strategy`。

**用什么权限**：标准 / 管理员都行（管理员能验证 mft_usn 路径，标准只能看 native）。

**用法**：

```powershell
cd src-tauri
# 默认 C:\，估算 80 万文件
cargo run --example deep_scan_benchmark -- C:\

# 指定子目录 + 估算文件数（影响进度计算精度）
cargo run --example deep_scan_benchmark -- C:\Users\me\Desktop 50000
```

**用途**：
- 验证某改动后增量路径还能走（`deep_second.strategy` 应该是 `incremental_cache`）
- 量化某优化对二扫加速效果（`deep_second.wall_duration_ms` 应明显小于 `deep_first.wall_duration_ms`）

### 6.3 `scripts/measure-reparse-depth.ps1`

测某路径下 reparse point 链路最深有多少层，用来调安全检测里 reparse 深度阈值。直接跑：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\measure-reparse-depth.ps1 C:\Users
```

### 6.4 `scripts/fetch-missing-crates.ps1`

`cargo check`/`build` 因网络中断报 `failed to download windows v0.x.0 ... unexpected end of file` 时用。重头预热 crates.io 下载，避免半个 `.crate` 文件。

---

## 7. 数据持久化

后端数据全部写在 **应用 exe 同级 `data/` 目录** 下（见 `utils.rs::get_app_data_dir()`），不是 AppData。

| 文件 | 作用 |
|---|---|
| `data/scan_cache.db` | 每次深扫的完整 `ScanResult` JSON + USN checkpoint，应用重开即用 |
| `data/migrations.db` | 迁移 / 删除历史，含 `migrations` 和 `delete_history` 表 |
| `data/space_history.db` | 每次扫完写一条磁盘空间快照，画 90 天走势图；自动清 90 天前的 |

dev 模式下 `cdrive-cleaner.exe` 在 `src-tauri/target/debug/`，所以 dev 数据库在 `src-tauri/target/debug/data/`。release 装出来后在安装目录下。

如果想清掉所有缓存重测一遍，直接删整个 `data/` 目录就行——下次启动会重建。

---

## 8. 手动测试要点

每个 PR 提之前过一遍：

1. **标准权限扫一次**：选 `C:`、点扫描，确认日志含 "回退到原生递归扫描" 或类似 fallback 提示
2. **管理员模式扫一次**：UI 上点"开启管理员模式" → UAC → 重启后再扫，确认日志含 `[mft-usn] MFT 枚举完成`
3. **下钻 / 大文件视图 / Treemap 视图**切换，确认每个视图都能渲染当前盘扫描结果
4. **挑一个明确可迁的目录**（比如 `C:\Users\<user>\AppData\Local\Temp` 不要选！选个测试创建的目录），迁到 `D:\`：
   - 复制完成 → C 盘可用空间增加
   - 原位置变成绿色 Junction 占位
   - D 盘出现目标数据
5. **进"历史" Tab 回滚**：源目录被还原、target 被删、状态变成 `rolled_back`
6. **安全检测红线**：试着迁 `C:\Windows\System32`，应被 `system_critical` 硬拒绝
7. **删除测试**：选一个临时目录，回收站删 + 永久删，验证两种模式都能正常完成
8. **游戏库**（如果机子装了 Steam）：进 GamesView Tab，确认 Steam 路径检测到、`appmanifest_*.acf` 解析的游戏列表显示出来

---

## 9. 重点关注（性能 / 健壮性）

- **扫描**：标准权限是否回退到原生枚举；管理员模式是否走 MFT + USN；扫描时长 / 文件数 / 目录数 / 漏算量是否合理（`metadata path fallback hits` 在 debug 日志里）
- **安全检测**：迁移弹窗 `analyze_migration_safety` 的耗时（理论 < 500ms，第二次起命中 30 秒缓存应该 < 1ms）；`Verdict::SystemCritical` 的目录必须真的拒绝
- **迁移**：复制成功 → 创建链接 → C 盘可用空间增加 → D 盘有数据 → 回滚后源目录恢复原状
- **增量扫**：第二次扫同一盘，`incremental.rs` 应该走 `[阶段1] 本次增量检测使用了 USN 日志`，不是 mtime 全量回退

性能基线数据见 [`bench/README.md`](bench/README.md)。

---

## 10. 常见坑

| 现象 | 原因 | 处理 |
|---|---|---|
| `npm run tauri dev` 卡在 `Compiling tauri-build` 不动 | crates.io 慢，不是真卡 | 等，或换 `[source.crates-io]` 镜像 |
| `dev` 启动后 IPC 全报 `__TAURI_IPC__ is undefined` | 你跑的是 `npm run dev` 而不是 `npm run tauri dev` | 用后者 |
| 改了 `tauri.conf.json` 没生效 | dev server 没重启 | Ctrl+C 重新 `npm run tauri dev` |
| 装了新 npm 包后 build 报 `Found version mismatched Tauri packages` | npm 端 `@tauri-apps/api` 版本被升到跟 `cli` 不齐 | `npm install @tauri-apps/api@<cli 同 minor> --save-exact` |
| `cargo test` 时 `[migration]` 测试失败 | 之前的测试残留 temp 目录 | `Remove-Item -Recurse -Force "$env:TEMP\cdrive-cleaner-*"` 然后重跑 |
| 安全检测一直报中文乱码 | 看日志的终端是 GBK，但 tracing 输出 UTF-8 | PowerShell 里跑 `$OutputEncoding = [Console]::OutputEncoding = [Text.Encoding]::UTF8` 一次 |
