# 开发指南

## 环境要求

- Node.js 18+ 与 npm 9+
- Rust stable（Windows 上需要 `x86_64-pc-windows-msvc` 工具链）
- Visual Studio 2022 Build Tools（含 C++ 工作负载与 Windows 11 SDK）
- WebView2 Runtime（Windows 10/11 通常已自带）

首次安装可参考 `docs/setup-windows.md`（待补）。

## 启动开发模式

```powershell
npm install
npm run tauri dev
```

`tauri dev` 会同时拉起 Vite（默认端口 1420）与 Tauri 调试窗口；改前端会热更新，改 Rust 代码会重新编译。

仅启动前端预览（不启 Tauri 窗口）：

```powershell
npm run dev
```

## 打包发布版

```powershell
npm run tauri build
```

产物位于：

- `target/release/bundle/msi/CSD_<version>_x64_zh-CN.msi`
- `target/release/bundle/nsis/CSD_<version>_x64-setup.exe`
- 裸可执行文件：`target/release/cdrive-cleaner.exe`

> 如果设置了环境变量 `CARGO_TARGET_DIR`，构建产物会输出到该路径而非 `target/`。

## 手动测试要点

1. 普通权限启动应用，选择 `C:`，点"开始扫描"。
2. 若提示"开启管理员模式"，确认 UAC 后再次扫描，确认日志含 `MFT + USN`。
3. 在结果中挑一个明确可迁移的大目录，迁移到 `D:`。
4. 记录 `C:`/`D:` 可用空间变化与目标盘文件结构。
5. 进入"迁移历史"，执行一次回滚，确认源目录被复原。

## 重点关注

- **扫描**：普通权限是否回退到原生枚举；管理员模式是否走 MFT + USN；扫描时长、文件数、目录数、漏算量是否合理。
- **安全检测**：迁移弹窗会先做安全分析，验证风险等级、原因、建议是否符合预期；危险目录应阻止迁移。
- **迁移**：复制成功 → 创建链接占位 → C 盘可用空间增加 → D 盘出现目标数据 → 回滚后源目录恢复原状。

## 终端日志关键字

- 深扫：`[scan-deep]`、`[deep-scan]`、`[阶段0]`、`[阶段1]`
- 迁移：`[migration]`、`[migration-core]`
- MFT 回退：`[mft-usn]`
- USN / 权限 / 漏算定位：`[winfs]`、`enabled privileges`、`captured USN checkpoint`、`metadata path fallback hits`

## 日志级别

后端用 [`tracing`](https://docs.rs/tracing/) 框架，可通过 `RUST_LOG` 环境变量调级别：

```powershell
# 默认 info：流程关键节点（启动、扫描完成、迁移完成、错误）
$env:RUST_LOG = 'info'
npm run tauri dev

# debug：含详细计时（[scan-timing] 等），调性能用
$env:RUST_LOG = 'debug'

# 仅本 crate 开 debug，其它依赖保持 warn 安静
$env:RUST_LOG = 'cdrive_cleaner_lib=debug,warn'

# 关掉日志
$env:RUST_LOG = 'off'
```
