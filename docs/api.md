# API 参考

CSD 前后端 IPC 契约。所有 Tauri 命令通过 `invoke('<name>', { ...args })` 调用，事件通过 `listen('<event>', cb)` 订阅。

> 所有命令的参数与返回值均来自 `src-tauri/src/commands.rs`，序列化通过 `serde_json`。
> Rust 字段名是 `snake_case`，前端 TS 直接使用相同字段名。

**目录**

- [§1 命令列表](#1-命令列表)
  - [§1.1 扫描](#11-扫描)
  - [§1.2 迁移与回滚](#12-迁移与回滚)
  - [§1.3 删除](#13-删除)
  - [§1.4 安全检测](#14-安全检测)
  - [§1.5 历史与统计](#15-历史与统计)
  - [§1.6 缓存管理](#16-缓存管理)
  - [§1.7 系统 / 权限](#17-系统--权限)
  - [§1.8 重复文件](#18-重复文件)
  - [§1.9 游戏库](#19-游戏库)
  - [§1.10 垃圾清理](#110-垃圾清理)
- [§2 事件](#2-事件)
- [§3 类型定义](#3-类型定义)
- [§4 数据库 Schema](#4-数据库-schema)

---

## 1. 命令列表

共 30 个命令，按业务域分组列出。每个命令注明：参数、返回值、是否会 emit 事件。

### 1.1 扫描

#### `scan_disk_deep`

深度扫描某个盘 / 路径，自动选择 `mft_usn` 或 `native` 后端，命中缓存时走增量。

| 参数 | 类型 | 说明 |
|---|---|---|
| `path` | `string` | 磁盘路径，如 `"C:\\"` 或子目录 |
| `estimatedFiles` | `number?` | 估算文件数，默认 800000，影响进度百分比计算 |

**返回**：[`ScanResult`](#scanresult)（仅根目录的快照，子节点用 `get_directory_snapshot` 拉）

**Emits**：[`deep-scan-progress`](#deep-scan-progress)

#### `get_directory_snapshot`

读取扫描结果中某子目录的快照（不重扫，从内存索引取）。

| 参数 | 类型 | 说明 |
|---|---|---|
| `rootPath` | `string` | 当前活跃的扫描根，必须先跑过 `scan_disk_deep` |
| `path` | `string` | 想看的子目录路径 |

**返回**：[`ScanResult`](#scanresult)，错误 `"目录快照不存在，请重新执行深度扫描"` 表示根没扫过或路径不在树里

#### `analyze_smart_groups`

对当前扫描结果做"智能分组"分析（应用缓存 / 开发工具 / 大文件等），用于 SmartGroupCard 视图。

| 参数 | 类型 | 说明 |
|---|---|---|
| `rootPath` | `string` | 扫描根路径 |

**返回**：[`SmartScanReport`](#smartscanreport)

#### `scan_directory_files`

平铺读某个目录下的文件列表（不递归子目录），用于 ListView。

| 参数 | 类型 | 说明 |
|---|---|---|
| `path` | `string` | 目录路径 |

**返回**：`FileInfo[]`（按 `size` 降序）

#### `cancel_scan`

取消正在进行的深度扫描。

**返回**：`void`

#### `get_scan_capabilities`

查询当前路径上能用的扫描后端 / 是否需要管理员。

| 参数 | 类型 | 说明 |
|---|---|---|
| `path` | `string` | 目标盘 / 路径 |

**返回**：[`ScanCapabilities`](#scancapabilities)

#### `reveal_in_explorer`

在 Explorer 里选中并显示一个路径（等同 `explorer.exe /select,<path>`）。

| 参数 | 类型 | 说明 |
|---|---|---|
| `path` | `string` | 文件 / 目录路径 |

**返回**：`void`

---

### 1.2 迁移与回滚

#### `migrate_file`

把一个目录 / 文件迁到另一个盘，原位置创建链接占位。

| 参数 | 类型 | 说明 |
|---|---|---|
| `source` | `string` | 源路径 |
| `targetDisk` | `string` | 目标盘根，如 `"D:\\"` |
| `linkType` | [`LinkType`](#linktype)? | 默认 `"auto"` |
| `knownSize` | `number?` | 已知总字节数（跳过预扫） |
| `knownFiles` | `number?` | 已知文件数（同上） |

**返回**：[`MigrationResult`](#migrationresult)，失败时 `Promise.reject(error_message)`

**Emits**：[`migration-progress`](#migration-progress)

> 迁移成功后会自动写一条 `migrations` 记录并清空安全检测缓存。

#### `rollback_migration`

按历史 ID 回滚一次迁移：把 target 复制回 source、删 link 占位、删 target、状态置 `rolled_back`。

| 参数 | 类型 | 说明 |
|---|---|---|
| `migrationId` | `number` | `MigrationRecord.id` |

**返回**：[`RollbackResult`](#rollbackresult)

> 只有 `status == "active"` 的记录才能回滚。

---

### 1.3 删除

#### `delete_path`

删除一个目录 / 文件，可选回收站或永久删。

| 参数 | 类型 | 说明 |
|---|---|---|
| `path` | `string` | 目标路径 |
| `mode` | [`DeleteMode`](#deletemode) | `"recycle"` 或 `"permanent"` |

**返回**：[`DeleteResult`](#deleteresult)

**Emits**：[`delete-progress`](#delete-progress)

> 删除前会先跑一次安全检测，`Verdict::SystemCritical` 的路径会被硬拒绝。

---

### 1.4 安全检测

#### `analyze_migration_safety`

对路径做迁移前的风险分析。结果带 30 秒缓存。

| 参数 | 类型 | 说明 |
|---|---|---|
| `path` | `string` | 待分析路径 |
| `size` | `number?` | 源大小（字节），影响某些 gate |
| `linkType` | `string?` | `"none"` / `"symlink"` / `"junction"` / 默认 `"auto"` |
| `targetDisk` | `string?` | 目标盘根，跨盘 / 同盘判断会用到 |

**返回**：[`MigrationSafety`](#migrationsafety)

---

### 1.5 历史与统计

#### `get_migration_history`

返回所有迁移记录（按 `created_at` 倒序）。

**返回**：`MigrationRecord[]`

#### `get_migration_stats`

返回汇总统计。

**返回**：[`MigrationStats`](#migrationstats)

#### `get_disk_info`

枚举所有逻辑盘符的容量信息。

**返回**：`DiskInfo[]`

#### `get_space_history`

读某个盘最近 N 天的空间快照（用于趋势图）。

| 参数 | 类型 | 说明 |
|---|---|---|
| `drive` | `string` | 盘符，如 `"C:\\"` |
| `days` | `number` | 天数（≥ 1） |

**返回**：`DiskSnapshot[]`（按时间升序）


---

### 1.6 缓存管理

#### `save_scan_cache`

手动保存一个扫描结果到缓存（一般 `scan_disk_deep` 会自动调，前端不直接用）。

| 参数 | 类型 | 说明 |
|---|---|---|
| `diskPath` | `string` | 缓存键（盘根） |
| `scanType` | `string` | 必须是 `"deep"`，其它会被拒绝 |
| `result` | [`ScanResult`](#scanresult) | 完整结果 |

**返回**：`void`

#### `get_scan_cache`

读缓存。

| 参数 | 类型 | 说明 |
|---|---|---|
| `diskPath` | `string` | 缓存键 |
| `scanType` | `string` | `"deep"` |

**返回**：`ScanResult | null`

#### `clear_scan_cache`

清空所有扫描缓存（不止当前盘）+ vacuum SQLite。

**返回**：`void`

#### `get_cache_info`

获取缓存数据库的总览（占用、每条记录元信息）。

**返回**：[`CacheInfo`](#cacheinfo)

#### `delete_cache_entry`

删某一条缓存记录。

| 参数 | 类型 | 说明 |
|---|---|---|
| `diskPath` | `string` | 缓存键 |
| `scanType` | `string` | `"deep"` |

**返回**：`void`

---

### 1.7 系统 / 权限

#### `is_elevated`

当前进程是否管理员权限。

**返回**：`boolean`

#### `restart_as_admin`

通过 `ShellExecuteW(verb="runas")` 触发 UAC，重启当前进程为管理员。**当前进程会立即退出**。

**返回**：`void`，失败时 `"Failed to restart as administrator"`

#### `exit_app`

正常退出应用。

**返回**：`void`

---

### 1.8 重复文件

#### `find_duplicates`

在已扫描结果的"大文件"集合里找重复（≥ 100 MB，按 size + xxhash 双重哈希）。

| 参数 | 类型 | 说明 |
|---|---|---|
| `rootPath` | `string` | 扫描根，必须先 `scan_disk_deep` |

**返回**：`DuplicateGroup[]`，按 `size` 降序

#### `delete_duplicate_files`

批量删一组路径。

| 参数 | 类型 | 说明 |
|---|---|---|
| `paths` | `string[]` | 要删的路径 |
| `mode` | `string` | `"recycle"` 或 `"permanent"` |

**返回**：[`DeleteResult`](#deleteresult)（聚合统计）

**Emits**：[`delete-progress`](#delete-progress)

---

### 1.9 游戏库

#### `detect_game_libraries`

探测所有支持的启动器（Steam / Epic / Game Pass），返回各家库与游戏列表。每家平台都会出现在结果里，未装的平台 `installed=false` 且 `games=[]`。

**返回**：`GameLibraryInfo[]`

#### `migrate_game`

迁移一个游戏（仅 Steam / Epic / Game Pass，**不支持 MicrosoftStore**），强制走 Junction。

| 参数 | 类型 | 说明 |
|---|---|---|
| `platform` | [`GamePlatform`](#gameplatform) | 不能传 `"microsoft_store"` |
| `appId` | `string` | 启动器内的应用 ID |
| `sourcePath` | `string` | 当前安装路径 |
| `targetDisk` | `string` | 目标盘根 |
| `knownSize` / `knownFiles` | `number?` | 同 `migrate_file` |

**返回**：[`MigrationResult`](#migrationresult)（`link_type` 字段始终为 `"junction"`，但历史表里写成 `game:<Platform>:<appId>`）

**Emits**：[`migration-progress`](#migration-progress)

#### `open_native_migration_ui`

打开 Windows 系统设置的"应用"页（`ms-settings:appsfeatures`），让用户走系统原生迁移流程（用于 WindowsApps / Game Pass 受 TrustedInstaller 保护的游戏）。

| 参数 | 类型 | 说明 |
|---|---|---|
| `platform` | [`GamePlatform`](#gameplatform) | 任意，命令内部都跳到同一处 |

**返回**：`void`

---

### 1.10 垃圾清理

#### `scan_junk_files`

按规则库扫 Windows 垃圾（系统 Temp / 浏览器缓存 / Windows Update / 缩略图缓存 / 回收站 / 崩溃转储 / 应用日志 / 字体缓存）。

| 参数 | 类型 | 说明 |
|---|---|---|
| `categories` | [`JunkCategory[]`](#junkcategory)? | 仅扫指定分类，`null` / 不传 = 全扫 |

**返回**：[`JunkScanResult`](#junkscanresult)

> `requires_admin` 的规则在标准权限会被跳过，写入 `skippedRules` 字段。

#### `clean_junk_files`

清理给定路径列表（串行、不会并行——回收站 API 在并发下不稳）。

| 参数 | 类型 | 说明 |
|---|---|---|
| `paths` | `string[]` | 待清理路径 |
| `toRecycleBin` | `boolean` | `true` 进回收站，`false` 永久删 |

**返回**：[`JunkCleanResult`](#junkcleanresult)



---

## 2. 事件

所有事件都通过 `app.emit("<name>", payload)` 在主进程发，前端用 `@tauri-apps/api/event` 的 `listen` 订阅。

| 事件名 | Payload | 触发时机 |
|---|---|---|
| `deep-scan-progress` | [`ScanProgress`](#scanprogress) | 深度扫描进度 |
| `migration-progress` | [`MigrationProgress`](#migrationprogress) | 迁移进度（含游戏迁移） |
| `delete-progress` | [`DeleteProgress`](#deleteprogress) | 删除进度（含 `delete_path` / `delete_duplicate_files`） |

> 进度线程内部用可中断的 20ms 切片轮询取消旗，最坏延迟 < 20ms 响应取消。

### `deep-scan-progress`

```typescript
interface ScanProgress {
    scanned_files: number;       // 已扫文件数
    scanned_dirs: number;        // 已扫目录数
    total_size: number;          // 累计字节
    current_path: string;        // 当前正在处理的路径
    elapsed_ms: number;          // 自扫描开始已耗时
    files_per_second: number;    // 实时吞吐
    progress_percent: number;    // 0-100，依赖 estimatedFiles 估算
}
```

### `migration-progress`

```typescript
interface MigrationProgress {
    status: string;              // "copying" | "verifying" | "creating_link" | "cleaning_up"
    copied_bytes: number;
    total_bytes: number;
    copied_files: number;
    total_files: number;
    current_file: string;
    progress_percent: number;    // 0-100
}
```

### `delete-progress`

```typescript
interface DeleteProgress {
    current_file: string;
    deleted_files: number;
    total_files: number;
    deleted_size: number;
    total_size: number;
    progress_percent: number;
    error_count: number;
}
```

---

## 3. 类型定义

### `ScanResult`

```typescript
interface ScanResult {
    root_path: string;
    total_size: number;
    total_files: number;
    total_dirs: number;
    scan_duration_ms: number;
    directories: DirectoryNode[];
    large_files: FileInfo[];
    inaccessible_count: number;
    scan_backend?: string;       // "mft_usn" | "incremental_usn" | "native"
    root_file_id?: number;       // NTFS 文件引用号（用于增量定位）
    usn_journal_id?: number;     // 当前 USN journal ID
    usn_next_usn?: number;       // 当前 USN 检查点
}
```

### `DirectoryNode`

```typescript
interface DirectoryNode {
    path: string;
    name: string;
    size: number;
    file_count: number;
    dir_count: number;
    children: DirectoryNode[];
    has_children: boolean;       // 即使 children 被裁剪，也能从这判断
    is_symlink: boolean;
    link_target: string | null;
    safety?: MigrationSafety;    // 仅在某些场景预填
    modified_time?: number;      // Unix 秒
    file_id?: number;            // NTFS 文件引用号
}
```

### `FileInfo`

```typescript
interface FileInfo {
    path: string;
    name: string;
    size: number;
    extension: string;
    modified_at: string;         // "YYYY-MM-DD HH:MM:SS"
    is_readonly: boolean;
    is_symlink: boolean;
    link_target?: string;
}
```

### `ScanCapabilities`

```typescript
interface ScanCapabilities {
    is_elevated: boolean;
    file_system: string;         // "NTFS" | "exFAT" | "FAT32" | "Unknown" | ...
    mft_available: boolean;
    preferred_backend: string;   // "mft_usn" | "native"
    admin_recommended: boolean;  // true 时 UI 显示"开启管理员模式"按钮
    reason: string;              // 给用户解释当前选择原因的中文文案
}
```

### `MigrationResult`

```typescript
interface MigrationResult {
    success: boolean;
    source_path: string;
    target_path: string;
    link_type: LinkType;         // "auto" | "symlink" | "junction" | "hardlink" | "none"
    file_size: number;
    duration_ms: number;
    migration_id: number;        // SQLite migrations.id；失败时为 0
    error?: string;
    warnings: string[];
}
```

### `RollbackResult`

```typescript
interface RollbackResult {
    success: boolean;
    source_path: string;
    target_path: string;
    duration_ms: number;
    error?: string;
}
```

### `LinkType`

```typescript
type LinkType = "auto" | "symlink" | "junction" | "hardlink" | "none";
```

`"auto"` 的解析规则（`link_creator.rs::determine_link_type`）：

- 跨盘 + 是目录 → `junction`
- 同盘 + 是文件 → `hardlink`
- 其它 → `symlink`

### `DeleteMode`

```typescript
type DeleteMode = "recycle" | "permanent";
```

### `DeleteResult`

```typescript
interface DeleteResult {
    success: boolean;
    source_path: string;
    mode: DeleteMode;
    deleted_size: number;
    deleted_files: number;
    errors: { path: string; error: string }[];
    duration_ms: number;
}
```

### `MigrationSafety`

```typescript
interface MigrationSafety {
    verdict: Verdict;
    can_migrate: boolean;
    findings: Finding[];
    required_actions: string[];
    app_type: string;
    analysis_duration_ms: number;
    gate_durations_ms: Record<string, number>;  // 每个 gate 单独耗时（并行跑，总和会大于 analysis_duration_ms）
}

type Verdict = "safe" | "safe_after_action" | "blocked" | "system_critical";

interface Finding {
    gate: string;                // 触发 gate 的 id，比如 "system_critical"、"reparse_depth"
    severity: "info" | "warning" | "blocker";
    message: string;
    detail?: string;
}
```

**Verdict 语义**：

| 值 | 行为 |
|---|---|
| `safe` | 直接放行 |
| `safe_after_action` | UI 提示用户先做某动作（比如关进程）才能迁 |
| `blocked` | 默认拒绝，用户可手动覆写继续 |
| `system_critical` | 硬拒绝，UI 灰按钮，无法覆写 |

### `DiskInfo`

```typescript
interface DiskInfo {
    drive_letter: string;        // "C:"
    label: string;               // 卷标，空时返回 "Local Disk"
    file_system: string;         // "NTFS" | "exFAT" | "FAT32" | "Unknown"
    total_space: number;         // 字节
    free_space: number;
    used_space: number;
    usage_percent: number;       // 0.0-100.0
}
```

### `MigrationRecord`

```typescript
interface MigrationRecord {
    id: number;
    source_path: string;
    target_path: string;
    link_type: string;           // "Symlink" | "Junction" | "Hardlink" | "Auto" | "None" | "Delete" | "game:Steam:<appid>" | ...
    file_size: number;
    created_at: string;          // SQLite 默认时间戳格式
    status: string;              // "active" | "rolled_back" | "broken" | "deleted"
}
```

### `MigrationStats`

```typescript
interface MigrationStats {
    total_count: number;
    total_size: number;
    active_count: number;
    rolled_back_count: number;
}
```

### `DiskSnapshot`

```typescript
interface DiskSnapshot {
    id: number;
    drive_letter: string;
    total_size: number;
    used_size: number;
    scanned_size: number;
    captured_at: string;
}
```

### `CacheInfo`

```typescript
interface CacheInfo {
    cache_path: string;          // scan_cache.db 的绝对路径
    total_size: number;          // 字节
    caches: CacheEntry[];
}

interface CacheEntry {
    disk_path: string;
    scan_type: string;           // "deep"
    file_count: number;
    total_size: number;
    created_at: string;
    cache_size: number;          // result_json 体积
}
```

### `DuplicateGroup`

```typescript
interface DuplicateGroup {
    size: number;                // 单个文件的大小
    files: { path: string; modified_at: string }[];
    wasted_bytes: number;        // = size * (files.length - 1)
}
```

> 仅检测 ≥ 100 MB 的文件，且跳过 symlink。

### `SmartScanReport`

```typescript
interface SmartScanReport {
    root_path: string;
    generated_at_ms: number;
    potential_savings: number;   // 所有 group 总和
    default_savings: number;     // default_selected=true 的项总和
    groups: SmartGroup[];
    analysis_duration_ms: number;
}

interface SmartGroup {
    category: SmartCategory;
    recommendation: SmartAction;
    total_size: number;
    item_count: number;
    selected_size: number;
    items: SmartItem[];
}

interface SmartItem {
    path: string;
    name: string;
    size: number;
    file_count: number;
    category: SmartCategory;
    recommendation: SmartAction;
    risk: SmartRisk;
    rule: string;
    default_selected: boolean;
}

type SmartCategory = "app_cache" | "dev_tools" | "temp_files" | "large_files";
type SmartAction   = "migrate" | "delete" | "review";
type SmartRisk     = "safe" | "caution" | "blocked" | "unknown";
```

### `GameLibraryInfo`

```typescript
interface GameLibraryInfo {
    platform: GamePlatform;
    library_paths: string[];
    games: GameInfo[];
    installed: boolean;
}

interface GameInfo {
    platform: GamePlatform;
    app_id: string;
    name: string;
    install_path: string;
    install_size: number;
    drive_letter: string;
    last_played?: string;
    can_migrate: boolean;
    migration_hint: string;      // 给用户解释能 / 不能迁的中文文案
}
```

### `GamePlatform`

```typescript
type GamePlatform = "steam" | "epic" | "game_pass" | "microsoft_store";
```

### `JunkScanResult`

```typescript
interface JunkScanResult {
    items: JunkItem[];
    total_size: number;
    total_count: number;
    scanned_rules: number;
    skipped_rules: string[];     // rule_id 列表（被管理员要求或环境变量缺失而跳过的）
    scan_duration_ms: number;
}

interface JunkItem {
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

type JunkRiskLevel = "safe" | "caution" | "risky";
```

### `JunkCategory`

```typescript
type JunkCategory =
    | "system_temp"
    | "browser_cache"
    | "windows_update"
    | "thumbnail_cache"
    | "recycle_bin"
    | "crash_dump"
    | "app_logs"
    | "font_cache";
```

### `JunkCleanResult`

```typescript
interface JunkCleanResult {
    success: boolean;
    cleaned_size: number;
    cleaned_count: number;
    failed_count: number;
    errors: { path: string; error: string }[];
    duration_ms: number;
}
```



---

## 4. 数据库 Schema

应用持久化数据放在 **应用 exe 同级 `data/` 目录**（不是 `%AppData%`）。详见 [`development.md` §7](development.md#7-数据持久化)。

### `migrations.db`

```sql
CREATE TABLE migrations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_path TEXT NOT NULL,
    target_path TEXT NOT NULL,
    link_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    status TEXT NOT NULL DEFAULT 'active'
);
```

`link_type` 实际取值：

- 普通迁移：`"Symlink"` / `"Junction"` / `"Hardlink"` / `"Auto"` / `"None"`
- 删除记录：`"Delete"`（target_path 写 `"recycle_bin"` 或 `"permanent"`）
- 游戏迁移：`"game:<Platform>:<appId>"`，如 `"game:Steam:730"`

`status` 取值：`"active"` / `"rolled_back"` / `"broken"` / `"deleted"`

### `scan_cache.db`

```sql
CREATE TABLE scan_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    disk_path TEXT NOT NULL,
    scan_type TEXT NOT NULL,             -- 仅 "deep"
    result_json TEXT NOT NULL,           -- ScanResult 的完整 JSON
    file_count INTEGER NOT NULL,
    total_size INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(disk_path, scan_type)
);
```

每个盘符同一个 `scan_type` 只保留一条最新缓存（`UNIQUE` 约束）。`result_json` 包含 USN checkpoint 字段，用来支撑增量扫描。

### `space_history.db`

```sql
CREATE TABLE disk_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    drive_letter TEXT NOT NULL,
    total_size INTEGER NOT NULL,
    used_size INTEGER NOT NULL,
    scanned_size INTEGER NOT NULL,
    captured_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_disk_snapshots_drive
    ON disk_snapshots(drive_letter, captured_at);
```

每次深扫结束自动写一条；超过 90 天的会在应用启动时自动清掉。

---

## 5. 调用例子

### 5.1 完整扫描 → 迁移流程

```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// 1. 查能力
const cap = await invoke<ScanCapabilities>("get_scan_capabilities", { path: "C:\\" });
if (cap.admin_recommended) {
    // UI 提示用户授权
    await invoke("restart_as_admin");  // 当前进程会立即退出
}

// 2. 订阅进度
const unlisten = await listen<ScanProgress>("deep-scan-progress", (e) => {
    console.log(`${e.payload.progress_percent.toFixed(1)}%  ${e.payload.current_path}`);
});

// 3. 扫
const root = await invoke<ScanResult>("scan_disk_deep", {
    path: "C:\\",
    estimatedFiles: 800000,
});
unlisten();

// 4. 看子目录
const sub = await invoke<ScanResult>("get_directory_snapshot", {
    rootPath: "C:\\",
    path: "C:\\Users\\me\\AppData",
});

// 5. 安全检测
const safety = await invoke<MigrationSafety>("analyze_migration_safety", {
    path: "C:\\Users\\me\\AppData\\Local\\NodeCache",
    size: sub.total_size,
    targetDisk: "D:\\",
});

if (safety.verdict === "safe" || safety.verdict === "blocked") {
    // 6. 迁移
    const migUnlisten = await listen<MigrationProgress>("migration-progress", (e) => {
        console.log(e.payload);
    });
    const result = await invoke<MigrationResult>("migrate_file", {
        source: "C:\\Users\\me\\AppData\\Local\\NodeCache",
        targetDisk: "D:\\",
    });
    migUnlisten();

    // 7. 想后悔？
    if (regret) {
        await invoke<RollbackResult>("rollback_migration", {
            migrationId: result.migration_id,
        });
    }
}
```

### 5.2 找重复文件并删

```typescript
const groups = await invoke<DuplicateGroup[]>("find_duplicates", {
    rootPath: "C:\\",
});

const toDelete = groups.flatMap(g => g.files.slice(1).map(f => f.path));  // 每组保留第一个，其余删

const result = await invoke<DeleteResult>("delete_duplicate_files", {
    paths: toDelete,
    mode: "recycle",
});
console.log(`释放 ${result.deleted_size} bytes，失败 ${result.errors.length} 个`);
```

### 5.3 跑垃圾清理

```typescript
const scanned = await invoke<JunkScanResult>("scan_junk_files", {
    categories: ["system_temp", "browser_cache"],   // 只扫这两类
});

const safePaths = scanned.items
    .filter(it => it.risk_level === "safe" && it.default_selected)
    .map(it => it.path);

const cleanResult = await invoke<JunkCleanResult>("clean_junk_files", {
    paths: safePaths,
    toRecycleBin: true,
});
```

---

## 6. 字段命名约定

- 所有 Rust 端结构体字段为 `snake_case`，serde 默认沿用，前端 TS 使用相同名字
- 命令参数名在 Rust 端是 `snake_case`（如 `target_disk`），但 Tauri 默认会转 `camelCase` 给前端（`targetDisk`）。本文档所有命令参数表格用 `camelCase` 写
- 枚举的 serde 编码是 `#[serde(rename_all = "snake_case")]`，所以前端看到的是 `"system_critical"` 而不是 `"SystemCritical"`
