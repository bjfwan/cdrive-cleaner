# 发布指南

记录怎么把 main 上的代码打成安装包、推到 GitHub Release。两种场景：

1. **新版本** —— 升 version、tag、出包、建 release
2. **覆盖现有 release 的资产** —— 不动 tag、不改 release notes，只换 .msi / .exe（比如修了图标、改了文案、打了热修但版本号不变）

## 0. 一次性准备

### Windows 工具

- Rust stable（msvc）+ VS 2022 Build Tools（C++ + Windows 11 SDK）
- Node.js 18+ / npm 9+
- [GitHub CLI](https://cli.github.com/)：`gh auth login` 登录一次

### 本机环境变量（如果你用了独立 Rust 安装目录）

如果 Rust 装在 `D:\DevTools\Rust\` 这种自定义路径而不是默认的 `~\.rustup`，每个新 PowerShell 会话都要先：

```powershell
$env:RUSTUP_HOME = 'D:\DevTools\Rust\rustup'
$env:CARGO_HOME  = 'D:\DevTools\Rust\cargo'
$env:PATH = 'D:\DevTools\Rust\cargo\bin;' + $env:PATH
```

否则 cargo 会报 `rustup could not choose a version of cargo to run`。

### 一致性检查

`npm run tauri build` 在开始之前会校验 `@tauri-apps/api` / `@tauri-apps/cli` 的 npm 版本必须跟 Rust crate `tauri` 在同一 minor。不一致会直接拒绝构建：

```
Found version mismatched Tauri packages.
tauri (v2.11.2) : @tauri-apps/api (v2.10.1)
```

修法：

```powershell
npm install @tauri-apps/api@^2.11 @tauri-apps/cli@^2.11 --save-exact
```

把 npm 端跟到 Rust 端的 minor。

## 1. 发新版本

### 1.1 升版本号（三处都要改）

```text
package.json                      "version": "0.1.x"
src-tauri/Cargo.toml              version = "0.1.x"
src-tauri/tauri.conf.json         "version": "0.1.x"
```

> ⚠️ `tauri.conf.json` 是 UTF-8（无 BOM）+ 含中文。**别用 PowerShell 5 的 `Set-Content` 改它**，PS5 会写 BOM 还会按 GBK 重编码，把中文搞乱，build script 直接 `unable to parse JSON Tauri config`。要改请直接编辑器手改、或用 PowerShell 7（`pwsh`）+ `-Encoding UTF8NoBOM`。

改完要让 Cargo.lock 跟上：

```powershell
cd src-tauri
cargo check --offline
cd ..
```

`--offline` 是为了不去 crates.io 下东西。如果本地缓存里已经有所有依赖，`cargo check` 会顺利走完，并把 Cargo.lock 里 `cdrive-cleaner` 的版本同步成新值。

### 1.2 写 changelog

```text
docs/changelog/v0.1.x.md
```

参考已有的 `v0.1.1.md` / `v0.1.2.md` 写法：标题 + 几个分类（新增功能 / 性能修复 / 工程化）+ 安装表格。这份内容会作为 release notes 用。

### 1.3 打包

```powershell
npm run tauri build
```

正常情况下：

1. 跑 `npm run build`（`vue-tsc --noEmit && vite build`），前端类型检查 + 打 `dist/`
2. `cargo build --release` 编 Rust 端，约 2~3 分钟
3. wix 打 MSI、makensis 打 NSIS

成功后产物在：

```text
src-tauri/target/release/bundle/msi/CSD_0.1.x_x64_zh-CN.msi
src-tauri/target/release/bundle/nsis/CSD_0.1.x_x64-setup.exe
```

### 1.4 提交 + 打 tag + 推 GitHub

```powershell
git add -A
git commit -m "release(v0.1.x): <简要描述>"
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

> 仓库里有一个 `.github/workflows/release.yml`，**push tag 会自动触发**它在 Actions 里也打包并发 release。如果你已经用本地产物自建了 release，那个 workflow 会一直转，浪费 runner 时间，可以取消：
>
> ```powershell
> gh run list --limit 3                       # 找 release workflow 的 run id
> gh run cancel <id>
> ```

## 2. 覆盖现有 release 的资产

适用：版本号不变（比如 v0.1.2 已发，但发现图标错了、小 bug 要顺手补、文案要调），不想多发一个号、不想改 release notes，只想把安装包换掉。

```powershell
# 1. 重新打包（前提：本地 main 已经是想要的状态）
npm run tauri build

# 2. 用 --clobber 覆盖现有 release 上的同名资产
gh release upload v0.1.x `
  src-tauri/target/release/bundle/msi/CSD_0.1.x_x64_zh-CN.msi `
  src-tauri/target/release/bundle/nsis/CSD_0.1.x_x64-setup.exe `
  --clobber
```

`--clobber` 是覆盖关键，没它会因为同名报错。tag、release id、release notes、固定的 release URL 都不动，普通用户从下载链接拿到的就是新文件。

验证：

```powershell
gh release view v0.1.x --json assets --jq '.assets[] | {name, size, updatedAt}'
```

`updatedAt` 应该是刚刚的时间。

## 3. 故障速查

| 现象 | 原因 | 处理 |
|---|---|---|
| `cargo check` 报 `Blocking waiting for file lock on package cache` | 之前的 cargo 进程没退干净 | `Stop-Process -Name cargo,rustc -Force; Remove-Item $env:CARGO_HOME\.package-cache -Force` |
| `unable to parse JSON Tauri config file` | `tauri.conf.json` 被 PS5 写坏（BOM/GBK） | `git checkout -- src-tauri/tauri.conf.json`，然后用编辑器手改版本号 |
| `Found version mismatched Tauri packages` | npm 端 `@tauri-apps/api` / `@tauri-apps/cli` 跟 Rust 端 `tauri` minor 不齐 | `npm install @tauri-apps/api@^2.x @tauri-apps/cli@^2.x --save-exact` |
| `failed to download windows v0.x.0 ... unexpected end of file` | crates.io 下载被打断，缓存里留了半个 .crate | 删 `$env:CARGO_HOME\registry\cache\<index>\<crate>-<ver>.crate` 和 `registry\src\<index>\<crate>-<ver>` 后重跑；网很差时可以先跑 `scripts\fetch-missing-crates.ps1` |
| `tauri-runtime-wry` 类型不匹配（`expected windows::Win32::Foundation::HWND, found Foundation::HWND`） | Rust crate `tauri` 与 npm 端版本不齐时，CLI 也可能直接编译，结果踩到 transitive `windows` 版本漂移 | 先看 npm vs Rust minor，对齐后通常自动消失 |
| Release workflow 在 push tag 后自动跑、跟手工 release 抢 | `.github/workflows/release.yml` 的 trigger | `gh run cancel <id>`；不想被自动触发可以临时把 workflow 文件改名或在 trigger 里加条件 |

## 4. 校验清单

发完版本前过一遍：

- `npm run build` 类型检查通过
- `cargo test --lib` 全过
- 装一次 `CSD_<v>_x64-setup.exe`，能启动、能扫盘、能搬一个测试目录
- Release 页面上 MSI / NSIS 大小、updatedAt 都是这次刚出的
