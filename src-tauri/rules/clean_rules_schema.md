# Clean Rules JSON Schema

本文档定义了 `clean_rules.json` 的数据结构，用于声明式配置 C 盘清理规则。

## 字段说明

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | string | ✅ | snake_case 唯一标识符 |
| `app_id` | string | ✅ | 应用分组标识，如 `discord`、`chrome`、`windows_system` |
| `detect` | string[] | ✅ | 检测路径数组，任一路径存在则规则激活。支持 `%APPDATA%` 等环境变量 |
| `category` | enum | ✅ | 分类枚举，见下表 |
| `name` | string | ✅ | 中文名称 |
| `description` | string | ✅ | 中文描述，说明是什么、删了有什么影响 |
| `targets` | Target[] | ✅ | 清理目标列表 |
| `risk` | enum | ✅ | `safe` / `caution` / `risky` |
| `requires_admin` | bool | ✅ | 是否需要管理员权限 |
| `default_selected` | bool | ✅ | 是否默认选中 |
| `clean_subdirs_only` | bool | ✅ | 是否仅清理子目录（保留目标目录本身） |

## Target 对象

```json
{
  "base": "%LOCALAPPDATA%\\Google\\Chrome\\User Data",
  "subdirs": ["Default\\Cache", "Default\\Code Cache"],
  "patterns": ["*.tmp", "*.log"]
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `base` | string | ✅ | 基础路径，支持环境变量 |
| `subdirs` | string[] | ❌ | 子目录列表，省略时扫描 base 本身 |
| `patterns` | string[] | ❌ | glob 文件匹配模式，省略时匹配全部文件 |

## Category 枚举

JSON 层面细分类别（映射到 Rust `JunkCategory` 枚举）：

| JSON category | 映射到 JunkCategory | 说明 |
|---------------|---------------------|------|
| `system_temp` | SystemTemp | 系统/用户临时文件 |
| `browser_cache` | BrowserCache | 浏览器缓存 |
| `windows_update` | WindowsUpdate | Windows 更新残留 |
| `thumbnail_cache` | ThumbnailCache | 缩略图/着色器缓存 |
| `recycle_bin` | RecycleBin | 回收站 |
| `crash_dump` | CrashDump | 崩溃转储 |
| `app_logs` | AppLogs | 应用日志 |
| `font_cache` | FontCache | 字体缓存 |
| `app_cache` | BrowserCache | 应用缓存（Electron 等） |
| `dev_cache` | BrowserCache | 开发工具缓存（npm/pip 等） |
| `installer_cache` | SystemTemp | 安装器缓存 |

## 环境变量

支持的环境变量占位符：

- `%APPDATA%` — 漫游应用数据
- `%LOCALAPPDATA%` — 本地应用数据
- `%USERPROFILE%` — 用户主目录
- `%WINDIR%` — Windows 系统目录
- `%PROGRAMDATA%` — 公共应用数据
- `%TEMP%` — 当前用户临时目录
- `%PROGRAMFILES%` — Program Files
- `%PROGRAMFILES(X86)%` — Program Files (x86)
