# CDrive Cleaner - API 文档

本文档定义前端（Vue）和后端（Rust）之间的接口契约。

---

## 1. Tauri Commands

Tauri Commands 是前端调用后端的主要方式，使用类型安全的 RPC 机制。

### 1.1 磁盘扫描

#### `scan_disk`

扫描指定磁盘的所有文件和文件夹。

**函数签名：**
```rust
#[tauri::command]
async fn scan_disk(path: String) -> Result<ScanResult, String>
```

**前端调用：**
```typescript
import { invoke } from '@tauri-apps/api/tauri';

const result = await invoke<ScanResult>('scan_disk', { 
    path: 'C:\\' 
});
```

**参数：**
- `path: String` - 要扫描的磁盘路径（如 "C:\\"）

**返回值：**
```typescript
interface ScanResult {
    root_path: string;           // 扫描的根路径
    total_size: number;          // 总大小（字节）
    total_files: number;         // 文件总数
    total_dirs: number;          // 目录总数
    scan_duration_ms: number;    // 扫描耗时（毫秒）
    directories: DirectoryNode[]; // 目录树
    inaccessible_count: number;  // 无法访问的文件数
}

interface DirectoryNode {
    path: string;                // 完整路径
    name: string;                // 目录名
    size: number;                // 大小（字节）
    file_count: number;          // 文件数量
    children: DirectoryNode[];   // 子目录
    is_symlink: boolean;         // 是否为符号链接
    link_target?: string;        // 链接目标（如果是符号链接）
}
```

**错误：**
- `"Invalid path"` - 路径无效
- `"Permission denied"` - 权限不足
- `"Disk not found"` - 磁盘不存在

---

#### `get_directory_info`

获取指定目录的详细信息。

**函数签名：**
```rust
#[tauri::command]
async fn get_directory_info(path: String) -> Result<DirectoryInfo, String>
```

**前端调用：**
```typescript
const info = await invoke<DirectoryInfo>('get_directory_info', {
    path: 'C:\\Users\\用户\\Documents'
});
```

**参数：**
- `path: String` - 目录路径

**返回值：**
```typescript
interface DirectoryInfo {
    path: string;
    name: string;
    size: number;
    file_count: number;
    dir_count: number;
    largest_files: FileInfo[];   // 最大的 10 个文件
    largest_dirs: DirectoryNode[]; // 最大的 10 个子目录
    is_symlink: boolean;
    link_target?: string;
    can_migrate: boolean;        // 是否可以迁移
    migrate_warning?: string;    // 迁移警告信息
}

interface FileInfo {
    path: string;
    name: string;
    size: number;
    extension: string;
    modified_at: string;         // ISO 8601 格式
    is_readonly: boolean;
}
```

---

### 1.2 文件迁移

#### `migrate_file`

迁移文件或目录到目标磁盘。

**函数签名：**
```rust
#[tauri::command]
async fn migrate_file(
    source: String,
    target_disk: String,
    link_type: Option<LinkType>
) -> Result<MigrationResult, String>
```

**前端调用：**
```typescript
const result = await invoke<MigrationResult>('migrate_file', {
    source: 'C:\\Users\\用户\\Videos',
    target_disk: 'D:\\',
    link_type: 'auto'  // 'auto' | 'symlink' | 'junction' | 'hardlink'
});
```

**参数：**
- `source: String` - 源文件/目录路径
- `target_disk: String` - 目标磁盘（如 "D:\\"）
- `link_type: Option<LinkType>` - 链接类型（可选，默认 auto）

**LinkType 枚举：**
```typescript
type LinkType = 'auto' | 'symlink' | 'junction' | 'hardlink';
```

**返回值：**
```typescript
interface MigrationResult {
    success: boolean;
    source_path: string;
    target_path: string;
    link_type: LinkType;
    file_size: number;
    duration_ms: number;
    migration_id: number;        // 数据库记录 ID
    error?: string;
}
```

**错误：**
- `"Source not found"` - 源文件不存在
- `"Target disk full"` - 目标磁盘空间不足
- `"File in use"` - 文件被占用
- `"Permission denied"` - 权限不足
- `"Migration failed: {reason}"` - 迁移失败

---

#### `batch_migrate`

批量迁移多个文件/目录。

**函数签名：**
```rust
#[tauri::command]
async fn batch_migrate(
    items: Vec<MigrationItem>,
    target_disk: String
) -> Result<BatchMigrationResult, String>
```

**前端调用：**
```typescript
const result = await invoke<BatchMigrationResult>('batch_migrate', {
    items: [
        { path: 'C:\\Users\\用户\\Videos', link_type: 'auto' },
        { path: 'C:\\Users\\用户\\Downloads', link_type: 'junction' }
    ],
    target_disk: 'D:\\'
});
```

**参数：**
```typescript
interface MigrationItem {
    path: string;
    link_type?: LinkType;
}
```

**返回值：**
```typescript
interface BatchMigrationResult {
    total: number;
    succeeded: number;
    failed: number;
    results: MigrationResult[];
    total_size: number;
    total_duration_ms: number;
}
```

---

### 1.3 迁移记录管理

#### `get_migration_history`

获取所有迁移记录。

**函数签名：**
```rust
#[tauri::command]
async fn get_migration_history(
    filter: Option<MigrationFilter>
) -> Result<Vec<Migration>, String>
```

**前端调用：**
```typescript
const history = await invoke<Migration[]>('get_migration_history', {
    filter: {
        status: 'active',
        from_date: '2026-03-01'
    }
});
```

**参数：**
```typescript
interface MigrationFilter {
    status?: 'active' | 'restored' | 'broken';
    from_date?: string;  // ISO 8601
    to_date?: string;
    min_size?: number;
}
```

**返回值：**
```typescript
interface Migration {
    id: number;
    source_path: string;
    target_path: string;
    link_type: LinkType;
    file_size: number;
    created_at: string;          // ISO 8601
    status: 'active' | 'restored' | 'broken';
}
```

---

#### `verify_links`

验证所有符号链接的完整性。

**函数签名：**
```rust
#[tauri::command]
async fn verify_links() -> Result<VerifyResult, String>
```

**前端调用：**
```typescript
const result = await invoke<VerifyResult>('verify_links');
```

**返回值：**
```typescript
interface VerifyResult {
    total: number;
    valid: number;
    broken: number;
    broken_links: BrokenLink[];
}

interface BrokenLink {
    migration_id: number;
    source_path: string;
    target_path: string;
    reason: string;              // 'target_missing' | 'link_broken' | 'permission_denied'
}
```

---

#### `restore_migration`

还原指定的迁移（第二阶段功能）。

**函数签名：**
```rust
#[tauri::command]
async fn restore_migration(migration_id: i32) -> Result<RestoreResult, String>
```

**前端调用：**
```typescript
const result = await invoke<RestoreResult>('restore_migration', {
    migration_id: 123
});
```

**返回值：**
```typescript
interface RestoreResult {
    success: boolean;
    migration_id: number;
    restored_path: string;
    duration_ms: number;
    error?: string;
}
```

---

### 1.4 系统信息

#### `get_disk_info`

获取所有磁盘信息。

**函数签名：**
```rust
#[tauri::command]
async fn get_disk_info() -> Result<Vec<DiskInfo>, String>
```

**前端调用：**
```typescript
const disks = await invoke<DiskInfo[]>('get_disk_info');
```

**返回值：**
```typescript
interface DiskInfo {
    drive_letter: string;        // 'C:', 'D:', etc.
    label: string;               // 磁盘标签
    file_system: string;         // 'NTFS', 'FAT32', etc.
    total_space: number;         // 总空间（字节）
    free_space: number;          // 可用空间（字节）
    used_space: number;          // 已用空间（字节）
    usage_percent: number;       // 使用百分比
}
```

---

## 2. Tauri Events

Events 用于后端向前端推送实时数据（如进度更新）。

### 2.1 扫描进度

**事件名：** `scan-progress`

**前端监听：**
```typescript
import { listen } from '@tauri-apps/api/event';

const unlisten = await listen<ScanProgress>('scan-progress', (event) => {
    console.log('扫描进度:', event.payload);
});

// 取消监听
unlisten();
```

**Payload：**
```typescript
interface ScanProgress {
    scanned_files: number;       // 已扫描文件数
    scanned_dirs: number;        // 已扫描目录数
    total_size: number;          // 已扫描总大小
    current_path: string;        // 当前扫描路径
    progress_percent: number;    // 进度百分比（估算）
}
```

---

### 2.2 迁移进度

**事件名：** `migration-progress`

**前端监听：**
```typescript
const unlisten = await listen<MigrationProgress>('migration-progress', (event) => {
    console.log('迁移进度:', event.payload);
});
```

**Payload：**
```typescript
interface MigrationProgress {
    migration_id: number;
    current_file: string;
    progress_percent: number;    // 0-100
    bytes_copied: number;
    total_bytes: number;
    status: 'copying' | 'verifying' | 'creating_link' | 'cleaning_up';
}
```

---

### 2.3 扫描完成

**事件名：** `scan-complete`

**前端监听：**
```typescript
const unlisten = await listen<ScanResult>('scan-complete', (event) => {
    console.log('扫描完成:', event.payload);
});
```

**Payload：** 同 `ScanResult`

---

### 2.4 迁移完成

**事件名：** `migration-complete`

**前端监听：**
```typescript
const unlisten = await listen<MigrationResult>('migration-complete', (event) => {
    console.log('迁移完成:', event.payload);
});
```

**Payload：** 同 `MigrationResult`

---

### 2.5 错误通知

**事件名：** `error`

**前端监听：**
```typescript
const unlisten = await listen<ErrorEvent>('error', (event) => {
    console.error('错误:', event.payload);
});
```

**Payload：**
```typescript
interface ErrorEvent {
    code: string;                // 错误代码
    message: string;             // 错误消息
    details?: string;            // 详细信息
    timestamp: string;           // ISO 8601
}
```

---

## 3. CLI 命令

CLI 工具使用相同的 Rust 后端逻辑，但通过命令行参数调用。

### 3.1 扫描命令

```bash
cdrive-cleaner scan <PATH> [OPTIONS]

Options:
  --format <FORMAT>      输出格式 [json|table|tree] (默认: table)
  --min-size <SIZE>      最小文件大小过滤 (如: 1GB, 500MB)
  --output <FILE>        输出到文件
  --depth <DEPTH>        显示深度 (默认: 3)
```

**JSON 输出格式：** 同 `ScanResult`

---

### 3.2 迁移命令

```bash
cdrive-cleaner migrate <SOURCE> <TARGET_DISK> [OPTIONS]

Options:
  --link-type <TYPE>     链接类型 [auto|symlink|junction|hardlink]
  --batch <FILE>         批量迁移（从文件读取路径列表）
  --dry-run              模拟运行，不实际执行
  --force                跳过确认
```

---

### 3.3 列表命令

```bash
cdrive-cleaner list [OPTIONS]

Options:
  --format <FORMAT>      输出格式 [json|table]
  --status <STATUS>      过滤状态 [active|restored|broken]
  --from <DATE>          起始日期 (YYYY-MM-DD)
  --to <DATE>            结束日期
```

**JSON 输出格式：** `Migration[]`

---

### 3.4 验证命令

```bash
cdrive-cleaner verify [OPTIONS]

Options:
  --format <FORMAT>      输出格式 [json|table]
  --fix                  自动修复损坏的链接
```

**JSON 输出格式：** 同 `VerifyResult`

---

## 4. 数据库 Schema

SQLite 数据库用于存储迁移记录。

```sql
-- 迁移记录表
CREATE TABLE migrations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_path TEXT NOT NULL,
    target_path TEXT NOT NULL,
    link_type TEXT NOT NULL CHECK(link_type IN ('symlink', 'junction', 'hardlink')),
    file_size INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'restored', 'broken')),
    error_message TEXT
);

-- 索引
CREATE INDEX idx_migrations_status ON migrations(status);
CREATE INDEX idx_migrations_created_at ON migrations(created_at);
CREATE INDEX idx_migrations_source_path ON migrations(source_path);

-- 日志表（用于调试）
CREATE TABLE logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    level TEXT NOT NULL CHECK(level IN ('debug', 'info', 'warn', 'error')),
    message TEXT NOT NULL,
    details TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_logs_level ON logs(level);
CREATE INDEX idx_logs_created_at ON logs(created_at);
```

---

## 5. 错误代码

统一的错误代码，便于前端处理。

| 错误代码 | 描述 | 处理建议 |
|---------|------|---------|
| `INVALID_PATH` | 路径无效 | 检查路径格式 |
| `PERMISSION_DENIED` | 权限不足 | 以管理员身份运行 |
| `DISK_NOT_FOUND` | 磁盘不存在 | 检查磁盘是否连接 |
| `DISK_FULL` | 磁盘空间不足 | 清理目标磁盘或选择其他磁盘 |
| `FILE_IN_USE` | 文件被占用 | 关闭占用文件的程序 |
| `FILE_NOT_FOUND` | 文件不存在 | 检查文件是否已被删除 |
| `LINK_CREATION_FAILED` | 链接创建失败 | 检查权限和文件系统类型 |
| `COPY_FAILED` | 文件复制失败 | 检查磁盘空间和权限 |
| `VERIFY_FAILED` | 完整性验证失败 | 重新尝试迁移 |
| `DATABASE_ERROR` | 数据库错误 | 检查数据库文件 |

---

## 6. 类型定义汇总

完整的 TypeScript 类型定义文件：

```typescript
// types.ts

export type LinkType = 'auto' | 'symlink' | 'junction' | 'hardlink';
export type MigrationStatus = 'active' | 'restored' | 'broken';
export type MigrationStage = 'copying' | 'verifying' | 'creating_link' | 'cleaning_up';

export interface ScanResult {
    root_path: string;
    total_size: number;
    total_files: number;
    total_dirs: number;
    scan_duration_ms: number;
    directories: DirectoryNode[];
    inaccessible_count: number;
}

export interface DirectoryNode {
    path: string;
    name: string;
    size: number;
    file_count: number;
    children: DirectoryNode[];
    is_symlink: boolean;
    link_target?: string;
}

export interface DirectoryInfo {
    path: string;
    name: string;
    size: number;
    file_count: number;
    dir_count: number;
    largest_files: FileInfo[];
    largest_dirs: DirectoryNode[];
    is_symlink: boolean;
    link_target?: string;
    can_migrate: boolean;
    migrate_warning?: string;
}

export interface FileInfo {
    path: string;
    name: string;
    size: number;
    extension: string;
    modified_at: string;
    is_readonly: boolean;
}

export interface MigrationResult {
    success: boolean;
    source_path: string;
    target_path: string;
    link_type: LinkType;
    file_size: number;
    duration_ms: number;
    migration_id: number;
    error?: string;
}

export interface Migration {
    id: number;
    source_path: string;
    target_path: string;
    link_type: LinkType;
    file_size: number;
    created_at: string;
    status: MigrationStatus;
}

export interface DiskInfo {
    drive_letter: string;
    label: string;
    file_system: string;
    total_space: number;
    free_space: number;
    used_space: number;
    usage_percent: number;
}

export interface ScanProgress {
    scanned_files: number;
    scanned_dirs: number;
    total_size: number;
    current_path: string;
    progress_percent: number;
}

export interface MigrationProgress {
    migration_id: number;
    current_file: string;
    progress_percent: number;
    bytes_copied: number;
    total_bytes: number;
    status: MigrationStage;
}

export interface VerifyResult {
    total: number;
    valid: number;
    broken: number;
    broken_links: BrokenLink[];
}

export interface BrokenLink {
    migration_id: number;
    source_path: string;
    target_path: string;
    reason: string;
}

export interface ErrorEvent {
    code: string;
    message: string;
    details?: string;
    timestamp: string;
}
```

---

## 7. 使用示例

### 7.1 完整的扫描流程

```typescript
import { invoke, listen } from '@tauri-apps/api';

async function scanDisk(path: string) {
    // 监听进度
    const unlisten = await listen<ScanProgress>('scan-progress', (event) => {
        const progress = event.payload;
        console.log(`进度: ${progress.progress_percent}%`);
        console.log(`当前: ${progress.current_path}`);
    });

    try {
        // 开始扫描
        const result = await invoke<ScanResult>('scan_disk', { path });
        
        console.log(`扫描完成！`);
        console.log(`总大小: ${formatBytes(result.total_size)}`);
        console.log(`文件数: ${result.total_files}`);
        console.log(`耗时: ${result.scan_duration_ms}ms`);
        
        return result;
    } finally {
        unlisten();
    }
}
```

### 7.2 完整的迁移流程

```typescript
async function migrateFile(source: string, targetDisk: string) {
    // 监听进度
    const unlisten = await listen<MigrationProgress>('migration-progress', (event) => {
        const progress = event.payload;
        console.log(`${progress.status}: ${progress.progress_percent}%`);
    });

    try {
        // 开始迁移
        const result = await invoke<MigrationResult>('migrate_file', {
            source,
            target_disk: targetDisk,
            link_type: 'auto'
        });

        if (result.success) {
            console.log(`迁移成功！`);
            console.log(`目标: ${result.target_path}`);
            console.log(`链接类型: ${result.link_type}`);
        } else {
            console.error(`迁移失败: ${result.error}`);
        }

        return result;
    } finally {
        unlisten();
    }
}
```

---

这份 API 文档定义了前后端之间的完整接口契约，确保开发过程中双方对接口的理解一致。
