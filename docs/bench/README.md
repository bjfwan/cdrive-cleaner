# 基准测试报告

每次跑 `cargo test --features bench --test bench_runner -- --ignored --nocapture`，
会在本目录之外（默认 `D:\DevTools\cdrive-cleaner-bench\`）生成一对 `<时间戳>.json` + `.md`。

需要长期归档的报告复制到这里。

## 已归档报告

| 文件 | 描述 |
|---|---|
| `20260516-185411.md` | **修复前** baseline，发现 progress thread 500ms 强制延迟 |
| `20260516-190135-after-fix.md` | **修复后**，扫描提速 3×–24× |

## 关键发现：进度线程 500ms 强制延迟（已修复）

### 现象

修复前所有扫描场景（30 文件 / 100 / 1k / 10k / 真实 15k）耗时**全部聚集在 500-510ms**，
变异系数 CV < 0.001——异常稳定。

```
scan_30_files_shallow      mean=501.12 ms (n=30)
scan_100_files_balanced    mean=501.33 ms
scan_1k_files_balanced     mean=503.45 ms
scan_10k_files_realistic   mean=503.64 ms
scan_real_path (15k 文件)  mean=509.66 ms
```

线性扩展严重失效——15k 文件的真实路径只比 30 文件多 8ms，说明扫描的实际工作根本没花时间，
500ms 全部来自别处。

### 根因

`src-tauri/src/scanner/disk_scanner.rs` 里的进度报告线程：

```rust
loop {
    std::thread::sleep(Duration::from_millis(500));   // 先睡满 500ms
    if should_stop_c.load(Ordering::Relaxed) { break; } // 再检查停止旗
    ...emit progress...
}
```

短扫描（如 32ms 完成）时，主线程立刻 `should_stop.store(true)` 然后
`progress_handle.join()`——但进度线程已经在 500ms 的 sleep 里，必须等满才会
醒来检查停止旗，于是**每次扫描都被强制至少 500ms**。

`mft_usn.rs` 里的 `PROGRESS_INTERVAL_MS = 300` 也有同样问题。

### 修复

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

最坏情况只多等 20ms 就能响应停止信号。

### 修复效果

| 场景 | 修复前 | 修复后 | 加速比 |
|---|---|---|---|
| scan_30_files_shallow      | 501.12 ms | 20.96 ms | **23.9×** |
| scan_100_files_balanced    | 501.33 ms | 21.33 ms | **23.5×** |
| scan_1k_files_balanced     | 503.45 ms | 43.15 ms | 11.7× |
| scan_deep_narrow_tree      | 505.56 ms | 46.08 ms | 11.0× |
| scan_realistic_mixed_1k    | 501.89 ms | 21.90 ms | 22.9× |
| scan_10k_files_realistic   | 503.64 ms | 43.82 ms | 11.5× |
| scan_real_path (15k 真实)  | 509.66 ms | 163.81 ms | 3.1× |
| end_to_end                 | 555.30 ms | 82.37 ms | 6.7× |

修复后 1k 文件树扫描 ~43ms，10k 文件真实分布扫描 ~44ms，15k 真实 node_modules 扫描 ~164ms，
吞吐 ~91k files/sec，符合 native 后端预期。

## 其他观察

### 安全检测在某些路径上耗时离谱

修复前后这一项数据没变：

| 路径 | mean | max |
|---|---|---|
| `C:\Windows`               | 0.04 ms | 0.17 ms |
| `C:\Program Files`         | **2806 ms** | **4393 ms** |
| `C:\Program Files (x86)`   | **1428 ms** | 1471 ms |
| `C:\Users\admin\Desktop\…` | 1.21 ms | 1.77 ms |

`C:\Windows` 走的是 short-circuit gate（系统关键目录立刻返回 SystemCritical），所以飞快——这是合理的。

但 `C:\Program Files` 平均 **2.8 秒**、最差近 4.4 秒——说明对真实存在的应用安装目录，
安全检测做了完整的 gate 串，可能涉及递归 stat / 文件系统调用 / 安装注册表查询。
对桌面应用来说，迁移弹窗多等 2-4 秒会被用户感知到延迟。

后续优化方向（待实施）：
- 把"已识别为应用安装目录"的判断结果缓存
- 把多个独立 gate 真正并行（`GateHandles::run_parallel` 看名字应该是并行的，但实测累加耗时
  接近 2.8s，可能存在串行点或单个 gate 自身 2s+，需要 profile）
- 给用户的 UI 上加 loading 指示，避免无反馈

### 数据库性能很好

| 操作 | mean |
|---|---|
| insert_migration               | 0.04 ms |
| get_all at 10 records          | 0.03 ms |
| get_all at 100 records         | 0.14 ms |
| get_all at 1000 records        | 1.27 ms |

SQLite WAL + bundled 性能符合预期，不是瓶颈。

### 冷热缓存差别不大

| 场景 | mean | median | min |
|---|---|---|---|
| scan_cold_first_pass | 38.27 | 43.43 | 22.98 |
| scan_hot_second_pass | 35.99 | 43.22 | 22.65 |

冷热扫差距很小（约 6%），说明 Windows 文件系统缓存对小目录基本是透明命中。
mean 比 median 小是因为有少数运行落在 22-23ms 区间，可能与 jwalk 内部的工作窃取调度
导致首扫与后续扫的快速路径不同有关。

### 端到端流程合理

完整 "扫描 → 安全分析 → junction 迁移 → 回滚" 链路在 ~30 文件的小目录上：

```
end_to_end mean = 82.37 ms
  其中 scan ≈ 21 ms
       safety ≈ 1-2 ms（Temp 目录是普通路径）
       migrate ≈ 30-50 ms（含文件复制 + junction 创建）
       rollback ≈ 15-25 ms（删除 junction + 恢复源）
```

迁移和回滚的写 IO 是主要成本，符合预期。

## 怎么再跑

```powershell
# 标准跑（30 次重复，约 2.5 分钟）
cargo test --features bench --test bench_runner -- --ignored --nocapture

# 加大重复次数提精度
$env:BENCH_REPEAT='100'
cargo test --features bench --test bench_runner -- --ignored --nocapture

# 加上真实路径（强烈建议指向一个 5k+ 文件的真实目录）
$env:BENCH_REAL_PATH='D:\Desktop\cdrive-cleaner\node_modules'
cargo test --features bench --test bench_runner -- --ignored --nocapture

# 自定义输出目录
$env:BENCH_OUTPUT_DIR='D:\my-bench-archive'
```
