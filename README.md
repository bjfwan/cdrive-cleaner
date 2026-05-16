# CSD · 磁盘空间管理

> Windows 桌面应用，基于 Tauri 2 + Vue 3 + Rust。
> 用于扫描磁盘空间分布、迁移大目录到其他盘、并保留可回滚的迁移历史。

## 功能

- **磁盘扫描**：NTFS 卷在管理员模式下使用 MFT + USN 增量扫描；普通权限自动回退到原生目录枚举。
- **空间分析**：树状图、列表、大文件三种视图，支持下钻浏览。
- **目录迁移**：把目录迁到其他盘，原位置创建 Junction / Symlink / Hardlink 占位（自动选择最合适的方式）。
- **安全检测**：迁移前自动判断风险等级，阻止误迁系统关键目录（Windows、Program Files、用户配置目录等）。
- **历史与回滚**：完整记录每次迁移，可一键回滚到原状态。

## 安装

到 [Releases 页面](https://github.com/bjfwan/cdrive-cleaner/releases) 下载最新版：

| 文件 | 类型 | 适用场景 |
|---|---|---|
| `CSD_x.y.z_x64-setup.exe` | NSIS 安装器 | 普通用户首选，体积更小 |
| `CSD_x.y.z_x64_zh-CN.msi`  | MSI 安装包 | 企业批量部署 |

系统要求：Windows 10 / 11 (x64) + WebView2 Runtime（缺失时安装器会自动拉取）。

## 使用建议

- 首次扫描建议**以管理员身份运行**：才能启用 MFT + USN 路径，扫描数十万文件可秒级完成。
- 标准权限也能用，会回退到原生枚举，速度慢一些但功能不变。
- 迁移大目录前先看安全检测的风险提示，红色等级的目录系统会拒绝迁移。

## 从源码构建

详见 [`docs/development.md`](docs/development.md)。

简略：

```powershell
npm install
npm run tauri dev      # 调试
npm run tauri build    # 出安装包
```

## 项目结构

```
.
├── src/                   Vue 3 前端
│   ├── components/
│   ├── composables/
│   ├── styles/
│   ├── types/
│   └── utils/
├── src-tauri/             Tauri / Rust 后端
│   ├── src/
│   │   ├── commands.rs    暴露给前端的 invoke 命令
│   │   ├── scanner/       MFT + USN 增量扫描
│   │   ├── migration/     文件迁移与链接管理
│   │   ├── safety/        风险等级判定
│   │   ├── database/      迁移历史 / 扫描缓存
│   │   ├── winfs.rs       Windows 文件系统底层封装
│   │   └── diagnostics.rs CLI 验证工具入口
│   ├── tests/             集成测试
│   ├── examples/          独立 CLI 工具
│   └── tauri.conf.json
├── docs/
│   ├── api.md
│   ├── development.md
│   ├── mft-usn-backend.md
│   └── changelog/
└── scripts/               PowerShell 验证脚本
```

## 许可

[MIT License](LICENSE) © 2026 bjfwan
