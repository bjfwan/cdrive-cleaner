# 发布指南

把 `main` 上的代码打成安装包、推到 GitHub Release 的完整流程。

涵盖三种场景：

1. **首次准备环境**（§0）—— 一次性
2. **发新版本**（§1）—— 升 version、打 tag、出包、建 release
3. **覆盖现有 release 的资产**（§2）—— 不动 tag、不改 release notes，只换 MSI / EXE（修了图标、改了文案、打了热修但版本号不变）

末尾附 §3 故障速查 和 §4 校验清单。

---

## 0. 一次性准备

### 0.1 Windows 工具链

| 组件 | 版本 / 用途 |
|---|---|
| **Rust** stable, target `x86_64-pc-windows-msvc` | `cargo build --release` 编后端 |
| **Visual Studio 2022 Build Tools** | 选 "C++ 桌面开发" 工作负载 + Windows 11 SDK，链接 MSVC C++ 运行时 |
| **Node.js 18+** + **npm 9+** | `vue-tsc` + `vite build` 出前端 `dist/` |
| **WebView2 Runtime** | Tauri 运行时依赖；Win11 自带，Win10 NSIS 安装器会代为下载 |
| **WiX Toolset 3.x** | 打 MSI；Tauri CLI 首次运行时会自动下载到 `%LOCALAPPDATA%\tauri\WixTools` |
| **NSIS** | 打 EXE 安装器；同样首次构建时由 Tauri CLI 自动拉 |
| **GitHub CLI** | 建 / 上传 release。`gh auth login` 登一次就行 |

### 0.2 自定义 Rust 安装路径下的 PATH 注入

如果 Rust 不在默认 `~\.rustup`，而是装在自定义目录（比如 `D:\DevTools\Rust\`），**每个新 PowerShell 会话**都要先注入：

```powershell
$env:RUSTUP_HOME = 'D:\DevTools\Rust\rustup'
$env:CARGO_HOME  = 'D:\DevTools\Rust\cargo'
$env:PATH        = 'D:\DevTools\Rust\cargo\bin;' + $env:PATH
```

不注入会报：

```
rustup could not choose a version of cargo to run
```

或者直接 `cargo : 无法将 cargo 项识别为 cmdlet`。

可以把这三行写进个人 `$PROFILE` 一次到位。

### 0.3 Tauri 包版本一致性

`npm run tauri build` 在最开始做一次校验：npm 端 `@tauri-apps/api` / `@tauri-apps/cli` 的版本必须跟 Rust crate `tauri` 的 minor 一致，**不一致直接拒绝构建**：

```
Found version mismatched Tauri packages.
tauri (v2.11.2) : @tauri-apps/api (v2.10.1)
```

修法：

```powershell
npm install @tauri-apps/api@^2.11 @tauri-apps/cli@^2.11 --save-exact
```

把 npm 端跟到 Rust 端的 minor。提交一份 `package-lock.json` 一起。

---

## 1. 发新版本

新版本流程是一条直链：升号 → 写日志 → 打包 → 提交 → 打 tag → 建 release。

### 1.1 升版本号（三处必须同步）

| 文件 | 字段 |
|---|---|
| `package.json` | `"version": "0.1.x"` |
| `src-tauri/Cargo.toml` | `version = "0.1.x"` |
| `src-tauri/tauri.conf.json` | `"version": "0.1.x"` |

> ⚠️ **`tauri.conf.json` 是 UTF-8（无 BOM）+ 含中文字符**。绝对不要用 PowerShell 5 的 `Set-Content` 改它——PS5 会写 BOM 还会按 GBK 重编码，把中文字符串搞乱，下次 `cargo check` 直接报 `unable to parse JSON Tauri config`。
>
> 改这个文件请直接用编辑器手改，或者用 PowerShell 7 (`pwsh`) + `-Encoding UTF8NoBOM`。

改完让 `Cargo.lock` 跟上：

```powershell
cd src-tauri
cargo check --offline
cd ..
```

`--offline` 防止它去 crates.io 下东西。本地缓存里所有依赖都齐时，`cargo check` 会顺利完成，并把 `Cargo.lock` 里 `cdrive-cleaner` 的版本同步成新值。

### 1.2 写 changelog

```
docs/changelog/v0.1.x.md
```

参考已有的 `v0.1.0.md` / `v0.1.1.md` / `v0.1.2.md`：

```
## CSD v0.1.x

<一句话总览>

### 新增功能
- ...

### 性能修复
- ...（带数据，比如"扫描 30 文件 501ms → 21ms（24× 加速）"）

### 工程化
- ...

### 安装

| 文件 | 类型 | 说明 |
|---|---|---|
| `CSD_0.1.x_x64-setup.exe` | NSIS | 推荐 |
| `CSD_0.1.x_x64_zh-CN.msi`  | MSI  | 适合企业部署 |
```

这份内容会作为 GitHub Release 的 release notes 直接使用。

### 1.3 打包

```powershell
npm run tauri build
```

正常 pipeline：

| 步骤 | 命令 | 时长 |
|---|---|---|
| 1 | `npm run build` (= `vue-tsc --noEmit && vite build`) | ~3 秒（前端类型检查 + Vite 打 `dist/`） |
| 2 | `cargo build --release` | 冷编 2-3 分钟，热编 30-60 秒 |
| 3 | WiX 打 MSI | ~15 秒 |
| 4 | NSIS 打 EXE 安装器 | ~10 秒 |

成功后产物（路径写死，发版要原样上传）：

```
src-tauri/target/release/bundle/msi/CSD_0.1.x_x64_zh-CN.msi
src-tauri/target/release/bundle/nsis/CSD_0.1.x_x64-setup.exe
```

裸 exe 在 `src-tauri/target/release/cdrive-cleaner.exe`，不需要发版上传，但本地试运行可用。

### 1.4 提交 + 打 tag + 推 GitHub

```powershell
git add -A
git commit -m "release(v0.1.x): <一句话主题>"
git push origin main

git tag v0.1.x
git push origin v0.1.x
```

### 1.5 建 release（不走 CI）

```powershell
gh release create v0.1.x `
  src-tauri/target/release/bundle/msi/CSD_0.1.x_x64_zh-CN.msi `
  src-tauri/target/release/bundle/nsis/CSD_0.1.x_x64-setup.exe `
  --title "CSD v0.1.x - <简短主题>" `
  --notes-file docs/changelog/v0.1.x.md
```

> 仓库里有 `.github/workflows/release.yml`，**push tag 会自动触发**它在 GitHub Actions 上重新打包并发 release。如果你已经用本地产物自建了 release，那个 workflow 还是会跑（最后会因为 release 已存在而失败或重复上传）。两种做法：
>
> **A. 取消 workflow run**（推荐，如果你只想用本地产物）：
> ```powershell
> gh run list --limit 3              # 找 release workflow 的 run id
> gh run cancel <run-id>
> ```
>
> **B. 反过来，让 CI 全权打包**：跳过本地的 `npm run tauri build`、`gh release create`，只 push tag，等 Actions 完成。CI 用的是 `softprops/action-gh-release@v2`，会自动建 release 并上传 MSI/EXE。优点是产物可复现、有 build log；缺点是 ~10 分钟，且发版人不能手工挑产物。

---

## 2. 覆盖现有 release 的资产

适用场景：版本号不变（比如 v0.1.2 已发），但发现图标错了 / 文案要调 / 修了个小 bug，**不想多发一个版本号、不想改 release notes，只想把安装包换掉**。

```powershell
# 1. 重新打包（前提：本地 main 已经是想要的状态）
npm run tauri build

# 2. 用 --clobber 覆盖现有 release 上的同名资产
gh release upload v0.1.x `
  src-tauri/target/release/bundle/msi/CSD_0.1.x_x64_zh-CN.msi `
  src-tauri/target/release/bundle/nsis/CSD_0.1.x_x64-setup.exe `
  --clobber
```

`--clobber` 是覆盖关键，没它会因为同名报错。tag、release id、release notes、固定的 release URL、用户已收藏的下载链接都不动，下载到的就是新文件。

验证资产更新：

```powershell
gh release view v0.1.x --json assets --jq '.assets[] | {name, size, updatedAt}'
```

`updatedAt` 应该是刚刚的时间戳。

> 这种"只换资产"的场景下，**本地不要打 tag、不要 commit 改了版本号的文件**——版本号不能动。如果要把这次的 fix 留到 git 历史，建议另起一个 commit `fix(release-assets): replace v0.1.x bundles with rounded icon` 之类，提到 main 但不打新 tag。

---

## 3. 故障速查

| 现象 | 原因 | 处理 |
|---|---|---|
| `cargo : 无法将 cargo 项识别为 cmdlet` | 自定义路径下 PATH 没注入 | 跑 §0.2 三行 `$env:` |
| `cargo check` 报 `Blocking waiting for file lock on package cache` | 之前的 `cargo` 进程没退干净 | `Stop-Process -Name cargo,rustc -Force; Remove-Item $env:CARGO_HOME\.package-cache -Force` |
| `unable to parse JSON Tauri config file` | `tauri.conf.json` 被 PS5 写坏（BOM / GBK 中文乱码） | `git checkout -- src-tauri/tauri.conf.json`，然后用编辑器手改版本号 |
| `Found version mismatched Tauri packages` | npm 端跟 Rust 端 `tauri` minor 不齐 | `npm install @tauri-apps/api@^2.x @tauri-apps/cli@^2.x --save-exact`（x 对齐 Rust 端） |
| `failed to download windows v0.x.0 ... unexpected end of file` | crates.io 下载被打断，留了半个 `.crate` | 删 `$env:CARGO_HOME\registry\cache\<index>\<crate>-<ver>.crate` 和 `registry\src\<index>\<crate>-<ver>` 后重跑；网很差时用 `scripts\fetch-missing-crates.ps1` 预热缓存 |
| `tauri-runtime-wry` 类型不匹配（`expected windows::...HWND, found Foundation::HWND`） | npm vs Rust minor 不齐时，CLI 也可能蒙着头编译，结果踩到 transitive `windows` crate 的版本漂移 | 先看 §0.3 对齐版本，多数情况自动消失 |
| Release workflow 在 push tag 后自动跑、跟手工 release 抢 | `.github/workflows/release.yml` 的 trigger | `gh run cancel <id>`；不想自动触发可以临时把 workflow 文件改名或在 trigger 里加条件分支 |
| 桌面图标还是老版本 | Windows 图标缓存 (`%LocalAppData%\IconCache.db` 等) 没刷新 | 关 Explorer → 删 `IconCache.db` / `iconcache_*.db` / `thumbcache_*.db` → 起回 Explorer。具体命令见 §3.1 |
| MSI 安装时报 "1603 Fatal error during installation" | 通常是已装版本的卸载残留挡道 | 控制面板先彻底卸老版，再装新的；或 `msiexec /x "{old-product-code}" /qn` 强卸 |

### 3.1 清桌面图标缓存

替换图标后桌面 / 任务栏的图标可能仍显示老版本，因为 Windows 把图标位图缓存到 `IconCache.db` 之类。开个**普通**（非 Tauri）PowerShell：

```powershell
taskkill /f /im explorer.exe
Remove-Item -Force "$env:LOCALAPPDATA\IconCache.db" -ErrorAction SilentlyContinue
Remove-Item -Force "$env:LOCALAPPDATA\Microsoft\Windows\Explorer\iconcache_*.db" -ErrorAction SilentlyContinue
Remove-Item -Force "$env:LOCALAPPDATA\Microsoft\Windows\Explorer\thumbcache_*.db" -ErrorAction SilentlyContinue
start explorer.exe
```

Explorer 重启后图标缓存会从新的 PE 资源里重建。

### 3.2 重做圆角图标

仓库自带 `scripts\round-icons.ps1`，会给 `src-tauri\icons\` 下所有 PNG 加圆角并重打 `icon.ico`：

```powershell
# 默认 22% 圆角（与 Win11 / Edge / Office 视觉对齐）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\round-icons.ps1

# 自定义圆角强度，比如更圆
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\round-icons.ps1 -RadiusPercent 25
```

> 脚本会**叠加**圆角到现有 PNG 上。如果要换比例重做，先恢复原图标再跑：备份你自己的（圆角前的）原始 PNG，或者从一次干净的 commit checkout 这些 PNG，然后再跑脚本。

---

## 4. 校验清单

发完版本前过一遍：

- [ ] `npm run build` 类型检查通过（无 `vue-tsc` 错误）
- [ ] `cargo test --tests --no-fail-fast` 全过（21+ 个集成测试）
- [ ] `npm run tauri dev` 起得来，UI 没空白页
- [ ] 装一次 `CSD_<v>_x64-setup.exe`，能启动、能扫盘、能搬一个测试目录、能回滚
- [ ] 标准权限和管理员权限各扫一次 C 盘，确认日志含 `MFT + USN`（管理员）和回退到 `native`（标准）
- [ ] 桌面 / 开始菜单 / 任务栏 / 标题栏图标都对（清缓存后再看）
- [ ] Release 页面上 MSI / NSIS 大小、`updatedAt` 都是这次刚出的
- [ ] release notes 渲染没乱码、表格能正常显示
- [ ] 三处版本号一致：`package.json` / `Cargo.toml` / `tauri.conf.json` / `Cargo.lock`

---

## 附录：版本号映射表

| 来源 | 字段 | 例 |
|---|---|---|
| `package.json` | `version` | `0.1.2` |
| `src-tauri/Cargo.toml` | `[package] version` | `0.1.2` |
| `src-tauri/Cargo.lock` | `[[package]] name = "cdrive-cleaner"` 下的 `version` | `0.1.2` |
| `src-tauri/tauri.conf.json` | 顶层 `version` | `0.1.2` |
| MSI 文件名 | `CSD_<version>_x64_zh-CN.msi` | `CSD_0.1.2_x64_zh-CN.msi` |
| NSIS 文件名 | `CSD_<version>_x64-setup.exe` | `CSD_0.1.2_x64-setup.exe` |
| Git tag | `v<version>` | `v0.1.2` |
| GitHub Release | `name` | `CSD v0.1.2 - <主题>` |

任何一处不匹配，都会在某一步发版流程里报错或装出来的程序"显示版本对不上"。
