# 基准测试

CSD 的性能基线和回归对比都在这里。

跑一次基准测试就出一对 `<时间戳>.json`（完整原始数据 + 系统信息）和 `<时间戳>.md`（带表格的人读摘要）。**默认输出在 `D:\DevTools\cdrive-cleaner-bench\`**，本目录只放需要长期归档的报告。

> 如果只想看历史数据，跳到 [§4 已归档报告](#4-已归档报告)。
> 如果要新跑一遍，看 [§1 快速开始](#1-快速开始)。

---

## 1. 快速开始

```powershell
# 标准跑（30 次重复，约 2-3 分钟）
cd src-tauri
cargo test --features bench --test bench_runner -- --ignored --nocapture
```

`--features bench` 启用 `cdrive_cleaner_lib::bench` 模块，`--ignored` 跑标记 `#[ignore]` 的基准测试，`--nocapture` 让 stdout 直接打印（看进度）。

跑完会在输出目录看到：

```
D:\DevTools\cdrive-cleaner-bench\
├── 20260516-190135.json
└── 20260516-190135.md
```

打开 `.md` 看表格，打开 `.json` 拿原始数据做后续分析。

### 1.1 环境变量

| 变量 | 默认值 | 说明 |
|---|---|---|
| `BENCH_REPEAT` | `30` | 每个常规场景的重复次数 |
| `BENCH_LARGE_REPEAT` | `5` | 大规模实验（10k 文件 / 真实路径）的重复次数 |
| `BENCH_OUTPUT_DIR` | `D:\DevTools\cdrive-cleaner-bench` | 报告输出目录 |
| `BENCH_REAL_PATH` | 未设置时跳过 | 跑一个真实目录的扫描基准（建议指向 5k+ 文件的真实路径，比如 `node_modules`） |

### 1.2 常用组合

```powershell
# 重复次数翻 3 倍，提精度（约 8 分钟）
$env:BENCH_REPEAT = '100'
cargo test --features bench --test bench_runner -- --ignored --nocapture

# 加上真实路径
$env:BENCH_REAL_PATH = 'D:\Desktop\cdrive-cleaner\node_modules'
cargo test --features bench --test bench_runner -- --ignored --nocapture

# 自定义输出目录
$env:BENCH_OUTPUT_DIR = 'D:\my-bench-archive'
cargo test --features bench --test bench_runner -- --ignored --nocapture

# 只跑某一组场景（cargo test 标准过滤）
cargo test --features bench --test bench_runner scan_30 -- --ignored --nocapture
```

---

## 2. 场景覆盖

`tests/bench_runner.rs` 当前覆盖 **5 个领域 21 个场景**。

### 2.1 扫描

| 场景名 | 拓扑 | 文件 / 目录数 | 备注 |
|---|---|---|---|
| `scan_30_files_shallow` | fanout=2, depth=2, leaf=2 | 14 文件 / 6 目录 | 微基准，验证最小开销 |
| `scan_100_files_balanced` | fanout=3, depth=3, leaf=2 | 80 文件 / 39 目录 | 验证浅树扫描 |
| `scan_1k_files_balanced` | 综合 | ~1k 文件 | 中等规模 |
| `scan_deep_narrow_tree` | 深窄 | ~1k 文件 | 验证目录嵌套深的情况 |
| `scan_shallow_wide_tree` | 浅宽 | ~1k 文件 | 验证单目录大量文件 |
| `scan_realistic_mixed_1k` | 70% 小 / 25% 中 / 5% 大 | ~1k 文件 | 模拟真实磁盘分布 |
| `scan_10k_files_realistic` | 同上 | ~10k 文件 | 大规模 |
| `scan_cold_first_pass` | 同 1k | 冷启动（每次新临时目录） | 测无 OS 缓存命中 |
| `scan_hot_second_pass` | 同 1k | 热扫（重复同目录） | 测有 OS 缓存命中 |
| `scan_real_path` | 用户指定 | 视目标 | 真实数据（要 `BENCH_REAL_PATH`） |

### 2.2 安全检测

| 场景名 | 路径 | 类型 |
|---|---|---|
| `safety_windows_dir` | `C:\Windows` | `system_critical` 短路 |
| `safety_program_files` | `C:\Program Files` | 集合根，应被立即拒绝 |
| `safety_program_files_x86` | `C:\Program Files (x86)` | 同上 |
| `safety_user_desktop` | `C:\Users\<u>\Desktop` | 普通用户路径 |
| `safety_user_appdata` | `C:\Users\<u>\AppData\Local` | 应用缓存路径 |
| `safety_temp_dir` | `%TEMP%` | 临时目录 |

### 2.3 数据库

| 场景名 | 操作 |
|---|---|
| `db_insert_migration` | 插一条迁移记录 |
| `db_query_all_at_10_records` | 表里 10 条时全表查 |
| `db_query_all_at_100_records` | 表里 100 条时 |
| `db_query_all_at_1000_records` | 表里 1000 条时 |

### 2.4 端到端

| 场景名 | 内容 |
|---|---|
| `end_to_end_scan_safety_migrate_rollback` | 扫 30 文件 → 安全分析 → junction 迁移 → 回滚 |

---

## 3. 解读报告

### 3.1 摘要表字段

```markdown
| 实验 | n | mean (ms) | median | p95 | min | max | stddev | CV |
```

| 字段 | 含义 |
|---|---|
| `n` | 重复次数 |
| `mean` | 算术平均，受离群值影响大 |
| `median` | 中位数，对长尾不敏感，最稳 |
| `p95` | 第 95 百分位，上界参考 |
| `min` / `max` | 最小 / 最大值 |
| `stddev` | 标准差 |
| `CV` | `stddev / mean`，**变异系数**——越小越稳 |

经验阈值：

- **CV < 0.05** ：超稳，可作为回归基线
- **CV 0.05-0.15** ：可重复，可信
- **CV 0.15-0.30** ：有抖动，看场景判断（端到端通常这个区间）
- **CV > 0.30** ：不稳，需要更多样本或拆解噪声源

### 3.2 怎么做回归对比

提了一个改动，怀疑可能影响性能 → 分别在改前 / 改后跑一次基准测试 → 对比同名场景的 `median`。

例（修复进度线程硬延迟前后）：

```
                       修复前 median   修复后 median   加速
scan_30_files_shallow  501.18 ms       20.91 ms        24×
scan_1k_files_balanced 503.41 ms       43.77 ms        12×
scan_real_path (15k)   509.66 ms       171.58 ms       3×
```

只有 `median` 差异 > 20% 且 `CV` 都健康时，才能下"确实变快"或"确实变慢"的结论。

### 3.3 常见模式

**变异系数极小 + 数字异常稳**：可能撞到了"硬性等待"。比如修复前所有扫描场景都 ~500 ms，CV < 0.001——典型的硬延迟特征（详见 §4 归档报告里的进度线程 bug）。

**冷热扫差不多**：说明 OS 文件缓存对小目录基本透明命中，不要硬抠"冷扫优化"。

**真实路径远慢于 10k 合成路径**：合成树是顺序生成的，FS 已经很友好；真实路径有更分散的 inode、链接、reparse point，1×1 不能等价。

---

## 4. 已归档报告

| 文件 | 描述 |
|---|---|
| [`20260516-190135-after-fix.md`](20260516-190135-after-fix.md) / `.json` | **当前性能基线**。修复了进度线程 500ms / 300ms 硬 sleep 之后的数据，未来回归对比的参照点 |

> 修复前的 baseline 数据（exhibits 500ms 硬延迟）已移除。关键现象与修复细节见下文。

### 4.1 关键事件：进度线程 500ms 强制延迟（已修复）

#### 现象

修复前所有扫描场景（30 / 100 / 1k / 10k / 真实 15k 文件）耗时**全部聚集在 500-510 ms**，CV < 0.001——异常稳定。

```
scan_30_files_shallow      mean = 501.12 ms (n=30)
scan_100_files_balanced    mean = 501.33 ms
scan_1k_files_balanced     mean = 503.45 ms
scan_10k_files_realistic   mean = 503.64 ms
scan_real_path (15k 文件)  mean = 509.66 ms
```

线性扩展严重失效——15k 文件的真实路径只比 30 文件多 8 ms，说明扫描的实际工作根本没花时间，500 ms 全部来自别处。

#### 根因

`scanner/disk_scanner.rs` 里的进度报告线程长这样：

```rust
loop {
    std::thread::sleep(Duration::from_millis(500));   // 先睡满 500ms
    if should_stop_c.load(Ordering::Relaxed) { break; } // 再检查停止旗
    ...emit progress...
}
```

短扫描（如 32 ms 完成）时，主线程立刻 `should_stop.store(true)` 然后 `progress_handle.join()`——但进度线程已经在 500 ms 的 sleep 里，必须等满才会醒来检查停止旗。结果：**每次扫描都被强制至少 500 ms**。

`mft_usn.rs` 里的 `PROGRESS_INTERVAL_MS = 300` 同理。

#### 修复

把硬 sleep 拆成可中断的小片段：

```rust
loop {
    let mut waited = 0u64;
    while waited < 500 {
        if should_stop_c.load(Ordering::Relaxed) || cancelled_c.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
        waited += 20;
    }
    if should_stop_c.load(Ordering::Relaxed) || cancelled_c.load(Ordering::Relaxed) {
        break;
    }
    ...emit progress...
}
```

最坏情况只多等 20 ms 就能响应停止信号。

#### 修复效果

| 场景 | 修复前 | 修复后 | 加速 |
|---|---|---|---|
| `scan_30_files_shallow` | 501.12 ms | 20.96 ms | **23.9×** |
| `scan_100_files_balanced` | 501.33 ms | 21.33 ms | **23.5×** |
| `scan_1k_files_balanced` | 503.45 ms | 43.15 ms | 11.7× |
| `scan_deep_narrow_tree` | 505.56 ms | 46.08 ms | 11.0× |
| `scan_realistic_mixed_1k` | 501.89 ms | 21.90 ms | 22.9× |
| `scan_10k_files_realistic` | 503.64 ms | 43.82 ms | 11.5× |
| `scan_real_path (15k 真实)` | 509.66 ms | 163.81 ms | 3.1× |
| `end_to_end` | 555.30 ms | 82.37 ms | 6.7× |

修复后 1k 文件树扫描 ~43 ms，10k 文件真实分布扫描 ~44 ms，15k 真实 `node_modules` 扫描 ~164 ms，吞吐 ~91k files/s——符合 native 后端预期。

### 4.2 其他观察

#### 安全检测在某些路径上耗时离谱

修复前后这一项数据没变：

| 路径 | mean | max |
|---|---|---|
| `C:\Windows` | 0.04 ms | 0.17 ms |
| `C:\Program Files` | **2806 ms** | **4393 ms** |
| `C:\Program Files (x86)` | **1428 ms** | 1471 ms |
| `C:\Users\<u>\Desktop\…` | 1.21 ms | 1.77 ms |

`C:\Windows` 走的是 short-circuit gate（系统关键目录立刻返回 `system_critical`），所以飞快——这是合理的。

但 `C:\Program Files` 平均 **2.8 秒**、最差近 4.4 秒——说明对真实存在的应用安装目录，安全检测做了完整的 gate 串，可能涉及递归 stat / 文件系统调用 / 安装注册表查询。对桌面应用来说，迁移弹窗多等 2-4 秒会被用户感知到延迟。

后续优化方向（待实施）：

- 把"已识别为应用安装目录"的判断结果持久化缓存
- 把多个独立 gate 真正并行（`GateHandles::run_parallel` 看名字应该是并行的，但实测累加耗时接近 2.8 s，可能存在串行点或单个 gate 自身 2 s+，需要 profile）
- 给用户的 UI 上加 loading 指示，避免无反馈

#### 数据库性能很好

| 操作 | mean |
|---|---|
| `insert_migration` | 0.04 ms |
| `get_all` at 10 records | 0.03 ms |
| `get_all` at 100 records | 0.14 ms |
| `get_all` at 1000 records | 1.27 ms |

SQLite WAL + bundled 性能符合预期，不是瓶颈。

#### 冷热缓存差别不大

| 场景 | mean | median | min |
|---|---|---|---|
| `scan_cold_first_pass` | 38.27 | 43.43 | 22.98 |
| `scan_hot_second_pass` | 35.99 | 43.22 | 22.65 |

冷热扫差距很小（约 6%），说明 Windows 文件系统缓存对小目录基本是透明命中。`mean` 比 `median` 小是因为有少数运行落在 22-23 ms 区间，可能与 jwalk 内部的工作窃取调度导致首扫与后续扫的快速路径不同有关。

#### 端到端流程合理

完整 "扫描 → 安全分析 → junction 迁移 → 回滚" 链路在 ~30 文件的小目录上：

```
end_to_end mean = 82.37 ms
  其中  scan     ≈ 21 ms
        safety   ≈ 1-2 ms（Temp 目录是普通路径）
        migrate  ≈ 30-50 ms（含文件复制 + junction 创建）
        rollback ≈ 15-25 ms（删除 junction + 恢复源）
```

迁移和回滚的写 IO 是主要成本，符合预期。

---

## 5. 添加新场景

`tests/bench_runner.rs` 是普通 `#[test] #[ignore]` 函数，加新场景就是加一个 fn：

```rust
#[test]
#[ignore]
fn bench_scan_my_scenario() -> anyhow::Result<()> {
    let workspace = temp_workspace("my-scenario");
    let count = make_synthetic_tree(&workspace, /* fanout */ 5, /* depth */ 4, /* leaf */ 3, /* size */ 1024);

    let series = repeat("scan_my_scenario", repeat_count(), || {
        let scanner = DiskScanner::new();
        let started = Instant::now();
        let result = futures::executor::block_on(
            scanner.scan_deep_silent(&workspace, count.max(1)),
        )?;
        let elapsed = started.elapsed();
        Ok((elapsed, ScanPayload::from(&result)))
    })?;

    write_series_to_report(series)?;
    Ok(())
}
```

设计原则：

- **场景名是关键 identifier**，跑两次基准时同名场景才能直接比对。改名前考虑兼容性
- **报告统一用 `repeat()` / `repeat_async()` 框架**，不要手写 `Instant::now()` + 循环——会丢失 stddev / p95 等统计
- **不依赖网络 / 真实磁盘大目录**，除非显式用 `BENCH_REAL_PATH`。其它场景必须用 `temp_workspace` 在临时目录搭合成树，跑完自动清理
- **避免在测试间共享状态**——`temp_workspace` 每次创建唯一名字，保证不同场景独立

---

## 6. 排查异常

| 现象 | 可能原因 | 检查 |
|---|---|---|
| 所有场景耗时都聚集在某个固定值（CV < 0.001） | 撞到硬延迟（睡眠 / 网络 / 锁等待） | 看 `[scan-timing]` debug 日志，找 stage 之间的 fixed gap |
| `scan_real_path` 比 `scan_10k_files_realistic` 慢 10× 以上 | 真实路径有大量 reparse / symlink / 权限拒绝 | 看 `inaccessible_count`，配合 `[winfs] metadata path fallback hits` |
| `safety_*` 远慢于以往 | 安全检测 gate 改变 / 缓存失效 | 看 `MigrationSafety.gate_durations_ms`，找最长的 gate |
| 数据库场景超过 5 ms / 条 | 写 amplification / WAL 文件没切换 | 检查 `migrations.db-wal` 大小，必要时 `VACUUM` |
| 端到端方差极大（CV > 0.5） | 复制 IO 受 OS 调度影响 | 提高 `BENCH_REPEAT` 到 100；多次跑取 median |

---

## 7. 未来计划

- [ ] 把 `scan_real_path` 拆成多目标（`node_modules` / `Steam/steamapps` / `WindowsApps`）
- [ ] 加 `incremental` 增量扫描的专门基准（首扫 + 修改 N 个目录 + 二扫）
- [ ] 加 mft_usn 后端的基准（需要管理员，可能放单独 `bench_admin_runner.rs`）
- [ ] 加 CI 任务：每次 push main 跑标准基准、把 markdown 报告 comment 到 commit
