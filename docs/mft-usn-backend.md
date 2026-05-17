# NTFS MFT + USN 扫描后端

CSD 的扫描子系统在 Windows 上有两条路：

- **`mft_usn`**：直接读 NTFS 卷的 MFT (Master File Table) 做全量枚举，用 USN journal 做增量。需要管理员权限 + NTFS 卷。
- **`native`**：用 `FindFirstFileExW` 递归 + jwalk 并行的常规枚举。任何卷、任何权限都能跑。

`mft_usn` 只是性能加速，**功能上跟 native 等价**。两条路输出同一份 [`ScanResult`](api.md#scanresult)，前端无感知。

> 这篇文档讲架构、文件分布、运行流程、回退条件。具体的数据结构定义看 [`api.md`](api.md)，性能数字看 [`bench/README.md`](bench/README.md)。

---

## 1. 为什么要做这个

### 1.1 native 的瓶颈

直接 `FindFirstFileExW` + 递归遍历 C 盘有几个问题：

- 几十万到上百万文件，cold scan 通常 30-120 秒
- 每个目录一次 syscall，cache miss 概率高
- 没有"上次扫到哪里"的概念，每次都从根重头来

### 1.2 NTFS 的两个外挂

NTFS 自己维护了两个数据结构：

- **MFT（Master File Table）**：每个文件 / 目录在 MFT 里都有一条记录，包含名字、大小、修改时间、父目录的 FRN（File Reference Number）。整张表能用 `FSCTL_ENUM_USN_DATA` 一次性流式读出来。
- **USN Journal**：NTFS 把每次文件变更（创建、删除、改大小、改属性）写一条 USN 记录到 journal。可以用 `FSCTL_QUERY_USN_JOURNAL` 拿当前游标，用 `FSCTL_READ_USN_JOURNAL` 读从某个游标到现在的所有变更。

把这两个结合起来：

- **首扫**：MFT enum → 一次性拿到全卷文件元数据 → 按 FRN 父子关系搭目录树
- **二扫**：从上次保存的 USN 游标读出所有变更 → 只重扫被改过的子目录 → 合并

实测在我们的工作机上，C 盘 ~50 万文件首扫 < 1 秒，二扫 < 200 ms。

---

## 2. 后端选择

入口是 `scanner/backend.rs::select_backend`：

```rust
pub fn select_backend(path: &Path) -> ScanBackendKind {
    if winfs::supports_mft_scan(path) {
        ScanBackendKind::MftUsn
    } else {
        ScanBackendKind::Native
    }
}
```

`supports_mft_scan` 在 `winfs.rs` 里检查三件事，**全部满足才返回 true**：

1. 路径所在卷的 file system 是 `"NTFS"`（`query_volume_details` 通过 `GetVolumeInformationW` 查）
2. 当前进程能成功 `CreateFileW(\\?\<volume>, ...)` 拿到卷句柄（这步隐式要求管理员权限或被授予 `SE_BACKUP_NAME` / `SE_RESTORE_NAME`）
3. 卷句柄能成功响应 `FSCTL_QUERY_USN_JOURNAL`

任何一项失败 → 直接降级到 native，**不抛错、不让用户看到**。这是有意的：让标准权限会话也能正常用，只是慢一点。

前端可以通过 `get_scan_capabilities` 命令查到当前选择的后端和原因，对应 UI 上的"开启管理员模式"按钮。

---

## 3. 文件分布

```
src-tauri/src/
├── scanner/
│   ├── backend.rs           ScanBackendKind 枚举 + select_backend()
│   ├── disk_scanner.rs      DiskScanner（对外的扫描入口）
│   │                         编排两个后端 + 进度发射 + 索引建立
│   ├── mft_usn.rs           NTFS 后端核心：MFT enum + 树搭建 + 元数据 hydrate
│   ├── incremental.rs       增量扫描三阶段流程
│   ├── progress.rs          ScanProgress 数据结构（GUI 解耦）
│   ├── timing.rs            StageTimer，记录各阶段耗时到 tracing
│   ├── scan_index.rs        IndexedScanResult（按路径前缀索引快照）
│   ├── duplicates.rs        重复文件检测（不依赖后端）
│   └── smart_scan.rs        智能分组（不依赖后端）
└── winfs.rs                 Windows 文件系统底层：
                              - FindFirstFileExW 浅枚举
                              - FSCTL_ENUM_USN_DATA       MFT 流读
                              - FSCTL_QUERY_USN_JOURNAL   游标查询
                              - FSCTL_READ_USN_JOURNAL    增量读
                              - OpenFileById              通过 FRN 反查路径
```

设计上 `scanner/mft_usn.rs` **不依赖 `tauri::AppHandle`**，进度通过 `ProgressCallback = Arc<dyn Fn(ScanProgress) + Send + Sync>` 抽象。这样后端可以在测试 / CLI / GUI 三种宿主里复用，benchmark 和 admin 验证才能脱离 GUI 跑。

---

## 4. 全量扫描流程（首扫）

```
                    DiskScanner::scan_deep
                            │
              select_backend(path)
                  ┌─────────┴─────────┐
                  ▼                   ▼
         ScanBackendKind::Native   ScanBackendKind::MftUsn
                  │                   │
                  ▼                   ▼
       jwalk 递归 + FindFirst...    mft_usn::scan_path
                  │                   │
                  └─────────┬─────────┘
                            ▼
                       ScanResult
                            │
                  存进 scan_cache.db
                  建 IndexedScanResult
                            │
                            ▼
                  返回根目录的 snapshot
```

### 4.1 mft_usn 的内部六阶段

`mft_usn::scan_path` 内部细分成 6 个阶段（`STAGE_*` 常量），都通过 `ProgressCallback` 报进度：

| Stage | 名字 | 干什么 |
|---|---|---|
| 0 | INIT | 拿 USN 起始 checkpoint（`query_usn_journal`），打开卷句柄 |
| 1 | ENUM_MFT | `FSCTL_ENUM_USN_DATA` 流式读全卷 MFT，得 `Vec<MftEntry>` |
| 2 | COLLECT_TREE | 按 FRN 父子关系搭 `HashMap<FRN, Vec<child FRN>>` |
| 3 | RESOLVE_PATHS | 从根 FRN 出发 BFS，用 `OpenFileById` 反查每个 FRN 对应的真实路径 |
| 4 | HYDRATE | 并行（rayon）拿每个文件的 size / modified_time / readonly 属性 |
| 5 | AGGREGATE | 按目录聚合 size / file_count / 大文件采样，建 `DirectoryNode` 树 |

最后再 `query_usn_journal` 拿一次 end checkpoint，记到 `ScanResult.usn_journal_id` / `usn_next_usn`。这两个值是后续增量扫描的关键。

### 4.2 为什么要 hydrate

MFT 里其实就有 size 和 modified_time，但 `FSCTL_ENUM_USN_DATA` 返回的 USN_RECORD_V3 里 size 字段不一定是最新的（USN 仅在文件**关闭**时同步 size）。所以正在写入的文件、稀疏文件、压缩文件可能 size 不准。

为了精度，hydrate 阶段对每个 FRN 走 `OpenFileById` + `GetFileInformationByHandleEx` 拿一次"真"的 size。这步是 IO 密集的，所以用 rayon 并行。

### 4.3 漏算的处理

少数文件会在 enum_mft 之后、hydrate 之前被删掉。这种情况 `OpenFileById` 会失败，hydrate 跳过这条记录，并把它计入 `inaccessible_count`。

debug 日志里会看到：

```
[winfs] metadata path fallback hits=N
```

`N` 一般是个位数。如果某次扫描这个数字很大，要么是真有大量并发删除，要么是后端实现有 bug。

---

## 5. 增量扫描流程（二扫起）

入口在 `commands::scan_disk_deep`：发现 `scan_cache.db` 有缓存且 USN checkpoint 完整，就调 `incremental::scan_incremental`，否则走全量。

`scan_incremental` 内部有四个阶段，**全部 tracing 日志带 `[阶段X]` 前缀**，方便 grep：

### 阶段 0：缓存摘要

读出 `cached_result`，打印关键统计：`backend` / `total_files` / `total_dirs` / `usn_journal_id` / `usn_next_usn`。

### 阶段 1：变化检测

两条路：

- **USN 路径**：从 `cached_result.usn_journal_id` + `cached_result.usn_next_usn` 起，读 USN journal 到现在，得到一组"被改过的目录路径"
- **mtime 路径**：缓存里没 USN checkpoint 时（比如老缓存、之前是 native 扫的），降级到递归 stat 比较 mtime

USN 路径几十毫秒搞定；mtime 路径在大目录上可能慢几秒，所以日志会明确打：

```
[阶段1] 缓存缺少 USN checkpoint，本次只能使用 mtime 递归检测
[阶段1] USN checkpoint 存在，但本次未能直接使用，已回退到 mtime 递归检测
[阶段1] 本次增量检测使用了 USN 日志
```

### 阶段 2：重扫变化目录

把阶段 1 给出的变化目录列表丢给 native 后端逐个浅扫（不递归整个子树，只扫到该层），合成一个 `Vec<DirectoryNode>` 增量集。

> 为什么这一步用 native 而不是 mft_usn？因为重扫范围只有一两个目录，不值得为这点活动用 MFT enum。

如果变化数太多（变化比例 > 30%），日志会打：

```
[阶段2] 变化超过30% (35.2%)，切换到全量扫描
```

然后整个扔掉增量结果，回去跑全量 `scan_deep`。这个阈值是经验值，避免在大量变化时增量重扫开销超过全量。

### 阶段 3：合并

把阶段 2 的增量集合并回 `cached_result.directories`：

- 路径已存在 → 替换该子树
- 路径不存在 → 插入到合适的父节点
- 阶段 1 收集到的删除集 → 从树里删掉对应节点

合并完做一次结构健康检查（`merge_health`）：

- 是否有重复路径节点
- 是否有孤儿 root（没有父引用）
- size / file_count 累加是否一致

任何一项异常 → 放弃增量结果，回退全量：

```
[阶段3] 检测到增量合并结构异常，放弃本次增量结果并切换到全量深度扫描重建缓存
```

最后再 query 一次 USN checkpoint 写回 `ScanResult`，下次增量从这个新 checkpoint 起。

---

## 6. 权限模型

`mft_usn` 后端的核心 syscall 都需要卷句柄 (`\\?\C:`)，而打开卷句柄又需要：

- 进程是 admin / 已 elevated（最常见）
- 或者进程被授予 `SE_BACKUP_NAME` 特权（Windows 服务用得多，桌面应用通常无）

CSD 不会做"提权失败时假装成功"那种事。`supports_mft_scan` 严格检查：能开句柄就用 mft_usn，否则用 native。

UI 上有两处会显示"开启管理员模式"按钮：

1. **设置 → 权限 → 管理员模式**
2. **主扫描卡**：检测到当前盘是 NTFS 但 `mft_available=false` 时显示

点按钮会调 `restart_as_admin` 命令，触发 UAC，重启当前进程为管理员。重启完再扫，自然就能用 mft_usn 了。

---

## 7. 验证

两个手段。

### 7.1 admin_mft_validation 自检

完整端到端验证脚本 + Rust example，会建临时目录、写文件、跑 MFT 扫描、改文件、读 USN journal 验证变化捕获。**必须管理员**：

```powershell
# 仓库根目录跑（自提权）
powershell -ExecutionPolicy Bypass -File scripts\run-admin-mft-validation.ps1 C:\

# 或在管理员 shell 里直接跑
cd src-tauri
cargo run --example admin_mft_validation -- C:\
```

通过会打印 `passed: true` 的 JSON 报告并退出 0；失败退出 2。详细字段见 [`development.md` §6.1](development.md#61-admin_mft_validation--mft--usn-端到端验证)。

### 7.2 集成测试

`src-tauri/tests/scanner_test.rs` 等里覆盖了：

- native 后端的递归正确性
- 大目录的 size 累加
- symlink / junction 处理
- 增量合并的边界情况

跑：

```powershell
cd src-tauri
cargo test --tests --no-fail-fast
```

---

## 8. 设计决策记录

### 8.1 为什么 `mft_usn` 没拆成 trait

最早实现时考虑过 `trait Backend { fn scan(&self) -> Result<ScanResult> }` 这种抽象，但实际上两个后端的差异太大：

- native 是递归驱动，每个目录都进 syscall
- mft_usn 是流式驱动，先全量再分阶段 hydrate

强行 trait 化会在 hot path 加 dyn dispatch + 把进度回调和取消信号挤进抽象，得不偿失。最后选择把"选哪个后端"放在 `disk_scanner.rs` 里手动 if/else，两个后端各自暴露 `pub fn scan_path(...) -> Result<ScanResult>`。

### 8.2 为什么 ScanResult 自带 USN checkpoint

最初 USN checkpoint 单独存在另一张表，但有并发问题：

- 进程 A 扫完写 checkpoint 到表 1
- 进程 B 扫完写 ScanResult 到 scan_cache.db
- 程序崩溃，两张表不一致

把 checkpoint 嵌到 `ScanResult` 里、跟 result 一起写一行 JSON 进 `scan_cache.db`，原子性靠 SQLite 单条 UPSERT 解决。这也是为什么 `incremental.rs::scan_incremental` 要从 `cached_result.usn_journal_id` 取 checkpoint 而不是查独立表。

### 8.3 为什么变化超 30% 就回退全量

经验值。增量扫的边际成本是"重扫一个目录 + 合并一个子树"，全量扫的边际成本是"枚举 MFT 一次"。当变化超过 30% 时：

- 重扫开销 ≈ 全量开销
- 合并健康检查的失败概率上升（变化集越大，越可能命中 race condition）
- 用户感知到的"增量很慢"反而比直接全量糟糕

阈值在 `incremental.rs` 里是硬编码 `change_ratio > 0.3`。后续可以做成可配置，但目前没必要。

### 8.4 为什么 progress 用 callback 而不是 Tauri Channel

`tauri::ipc::Channel` 看起来更简洁，但它绑死 Tauri 运行时。我们想让后端：

- 在 `cargo test` 里跑（无 Tauri）
- 在 `cargo run --example` CLI 里跑（无 Tauri）
- 在 GUI 里跑（有 Tauri）

callback `Arc<dyn Fn(ScanProgress) + Send + Sync>` 是最小公分母：测试 / CLI 传 noop，GUI 传一个把 `ScanProgress` emit 到 Tauri event 的闭包。

`disk_scanner.rs` 里的 `scan_deep` 接受 `AppHandle` 然后 wrap 成 callback，`scan_deep_silent`（diagnostics 用）接受 None，两条路走同一个核心 `scan_deep_with_progress`。

---

## 9. 已知限制

- **Windows only**：MFT / USN 是 NTFS 特有，不打算 port 到其他文件系统
- **不处理硬链接合并**：同一个 FRN 多名字的文件会按 MFT 里的"主名"算，多名字下计数会重复（已知；NTFS 上极罕见，遇到再说）
- **大于 4 GB 的 USN journal 极端情况**：USN 是 64 位计数器，单次 read 最多取 16 KB 记录。journal 太大时连续 read 几十次也能拿全，但理论存在 wrap-around 风险。当前用 `current.NextUsn < checkpoint.next_usn` 比较检测旧 checkpoint 失效，并降级到 mtime 路径；实战未观察到 wrap
- **管理员状态变更不会自动切换后端**：进程启动时一次性决定权限，UI 上"开启管理员模式"会重启进程，重启后才用 mft_usn

---

## 10. 与 native 后端的对照表

| 维度 | mft_usn | native |
|---|---|---|
| 触发条件 | NTFS + admin + 卷句柄可开 | 任意 |
| 全量首扫 | 流式 MFT | 递归 jwalk |
| 增量 | USN journal 游标 | mtime 递归比对 |
| C 盘 50 万文件实测 | < 1 秒 | 30-120 秒 |
| 内存峰值 | 高（一次性持有所有 MFT 记录） | 低（流式处理目录） |
| 漏算文件 | 极少（hydrate 失败的） | 极少（permission denied 的） |
| 失败回退 | 自动 → native | 无（不能再降） |
| 日志前缀 | `[mft-usn]` `[winfs-usn]` `[scan-timing]` | `[scan-timing]` |
| 单独验证工具 | `admin_mft_validation` example | 直接 `cargo test --tests` |

设计目标始终是：**有 mft_usn 用最好；没有也能用，性能差但功能不打折**。
