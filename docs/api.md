# API 文档

**更新：** 2026-03-04

---

## Commands

### `scan_disk`

**参数：**
```typescript
{
    path: string  // 磁盘路径，如 "C:\\"
}
```

**返回：** `ScanResult`

**事件：** `scan-progress`

---

### `scan_disk_deep`

**参数：**
```typescript
{
    path: string              // 磁盘路径
    estimatedFiles?: number   // 估算文件数，用于进度计算
}
```

**返回：** `ScanResult`

**事件：** `deep-scan-progress`

---

### `scan_directory_files`

**参数：**
```typescript
{
    path: string  // 目录路径
}
```

**返回：** `FileInfo[]`

---

### `migrate_file`

**参数：**
```typescript
{
    source: string        // 源路径
    target_disk: string   // 目标磁盘，如 "D:\\"
    link_type?: string    // 'auto' | 'symlink' | 'junction' | 'hardlink'
}
```

**返回：** `MigrationResult`

**事件：** `migration-progress`

---

### `get_disk_info`

**参数：** 无

**返回：** `DiskInfo[]`

---

### `analyze_migration_safety`

**参数：**
```typescript
{
    path: string   // 要分析的路径
    size: number   // 文件/目录大小（字节）
}
```

**返回：** `MigrationSafety`

---

## Events

### `scan-progress` / `deep-scan-progress`

**Payload：** `ScanProgress`

---

### `migration-progress`

**Payload：** `MigrationProgress`

---

## Types

### ScanResult
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
}
```

### DirectoryNode
```typescript
interface DirectoryNode {
    path: string;
    name: string;
    size: number;
    file_count: number;
    children: DirectoryNode[];
    is_symlink: boolean;
    link_target?: string;
    safety?: MigrationSafety;
}
```

### FileInfo
```typescript
interface FileInfo {
    path: string;
    name: string;
    size: number;
    extension: string;
    modified_at: string;  // YYYY-MM-DD HH:MM:SS
    is_readonly: boolean;
}
```

### MigrationResult
```typescript
interface MigrationResult {
    success: boolean;
    source_path: string;
    target_path: string;
    link_type: string;
    file_size: number;
    duration_ms: number;
    migration_id: number;
    error?: string;
}
```

### DiskInfo
```typescript
interface DiskInfo {
    drive_letter: string;
    label: string;
    file_system: string;
    total_space: number;
    free_space: number;
    used_space: number;
    usage_percent: number;
}
```

### MigrationSafety
```typescript
interface MigrationSafety {
    risk_level: 'safe' | 'moderate' | 'risky' | 'dangerous';
    safety_score: number;        // 0-100
    can_migrate: boolean;
    reasons: string[];
    recommendations: string[];
    app_type: string;
}
```

**risk_level 说明：**
- `safe` - 安全（≥75分）
- `moderate` - 中风险（50-74分）
- `risky` - 高风险（30-49分）
- `dangerous` - 危险（<30分）

**app_type 可能值：**
`"Steam游戏"` | `"Epic游戏"` | `"便携式应用"` | `"用户数据"` | `"媒体文件"` | `"临时文件"` | `"游戏"` | `"系统关键"` | `"未知类型"`

### ScanProgress
```typescript
interface ScanProgress {
    scanned_files: number;
    scanned_dirs: number;
    total_size: number;
    current_path: string;
    elapsed_ms: number;
    files_per_second: number;
    progress_percent: number;
}
```

### MigrationProgress
```typescript
interface MigrationProgress {
    current_file: string;
    progress_percent: number;
    bytes_copied: number;
    total_bytes: number;
    status: 'copying' | 'verifying' | 'creating_link' | 'cleaning_up';
}
```

---

## Database

```sql
CREATE TABLE migrations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_path TEXT NOT NULL,
    target_path TEXT NOT NULL,
    link_type TEXT NOT NULL CHECK(link_type IN ('symlink', 'junction', 'hardlink')),
    file_size INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'restored', 'broken'))
);

CREATE INDEX idx_migrations_status ON migrations(status);
CREATE INDEX idx_migrations_created_at ON migrations(created_at);
CREATE INDEX idx_migrations_source_path ON migrations(source_path);
```
