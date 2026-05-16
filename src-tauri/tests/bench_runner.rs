//! 重复 N 次的基准测试，给可重复实验做数据样本。
//!
//! ## 运行方式
//!
//! 默认这些测试**带 `#[ignore]`**，避免污染常规 `cargo test`。
//! 显式跑：
//!
//! ```powershell
//! cargo test --features bench --test bench_runner -- --ignored --nocapture
//! ```
//!
//! ## 输出
//!
//! 报告写到 `BENCH_OUTPUT_DIR`（默认 `D:\DevTools\cdrive-cleaner-bench`），
//! 包含 `<时间戳>.json`（完整原始数据 + 系统信息）和 `<时间戳>.md`（摘要 + 表格）。
//!
//! ## 环境变量
//!
//! - `BENCH_REPEAT`: 每个场景重复次数，默认 30
//! - `BENCH_OUTPUT_DIR`: 输出目录
//! - `BENCH_LARGE_REPEAT`: 大规模实验（10k 文件）的重复次数，默认 5
//! - `BENCH_REAL_PATH`: 可选，跑一个真实目录的扫描基准（如 `D:\some\folder`）
//!
//! ## 场景覆盖
//!
//! - 扫描：30 / 100 / 1k / 10k 文件，深窄 / 浅宽，混合大小，冷热缓存对比，可选真实目录
//! - 安全检测：6 类典型路径
//! - 数据库：insert / 查询 / 统计在 10 / 100 / 1000 条规模
//! - 端到端：扫描 → 安全分析 → 迁移 → 回滚

#![cfg(all(target_os = "windows", feature = "bench"))]

use cdrive_cleaner_lib::bench::{
    render_markdown, repeat, repeat_async, BenchReport, BenchSeries, SystemInfo,
};
use cdrive_cleaner_lib::database::MigrationDb;
use cdrive_cleaner_lib::migration::{FileMigrator, LinkType};
use cdrive_cleaner_lib::safety::analyze;
use cdrive_cleaner_lib::scanner::DiskScanner;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

// =========================================================================
// 公共工具
// =========================================================================

fn repeat_count() -> usize {
    std::env::var("BENCH_REPEAT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30)
}

fn large_repeat_count() -> usize {
    std::env::var("BENCH_LARGE_REPEAT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5)
}

fn output_dir() -> PathBuf {
    let dir = std::env::var("BENCH_OUTPUT_DIR")
        .unwrap_or_else(|_| r"D:\DevTools\cdrive-cleaner-bench".to_string());
    let p = PathBuf::from(dir);
    fs::create_dir_all(&p).expect("无法创建基准输出目录");
    p
}

fn timestamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

fn temp_workspace(label: &str) -> PathBuf {
    let unique = format!(
        "csd-bench-{}-{}-{}",
        label,
        std::process::id(),
        chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)
    );
    let root = std::env::temp_dir().join(unique);
    fs::create_dir_all(&root).unwrap();
    root
}

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::File::create(path).unwrap().write_all(bytes).unwrap();
}

/// 在 root 下生成 fanout × depth 棵树，每个叶子有 files_per_leaf 个文件。
fn make_synthetic_tree(
    root: &Path,
    fanout: usize,
    depth: usize,
    files_per_leaf: usize,
    file_size: usize,
) -> usize {
    fn recurse(
        parent: &Path,
        fanout: usize,
        depth: usize,
        files_per_leaf: usize,
        file_size: usize,
        count: &mut usize,
    ) {
        for i in 0..files_per_leaf {
            let f = parent.join(format!("file_{}.bin", i));
            write_file(&f, &vec![b'x'; file_size]);
            *count += 1;
        }
        if depth == 0 {
            return;
        }
        for i in 0..fanout {
            let sub = parent.join(format!("dir_{}", i));
            fs::create_dir_all(&sub).unwrap();
            recurse(&sub, fanout, depth - 1, files_per_leaf, file_size, count);
        }
    }
    let mut count = 0;
    recurse(root, fanout, depth, files_per_leaf, file_size, &mut count);
    count
}

/// 生成混合文件大小的目录树，更接近真实磁盘分布。
/// 70% 小文件（1-4KB）+ 25% 中等（10-100KB）+ 5% 大（1-10MB）。
fn make_realistic_tree(root: &Path, target_count: usize) -> usize {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn pseudo_rand(seed: u64) -> u64 {
        let mut h = DefaultHasher::new();
        seed.hash(&mut h);
        h.finish()
    }

    let mut created = 0usize;
    // 大致 sqrt 分一级目录
    let dir_count = ((target_count as f64).sqrt() as usize).max(1);
    for d in 0..dir_count {
        let sub = root.join(format!("project_{}", d));
        fs::create_dir_all(&sub).unwrap();
        // 每个 dir 再分两层
        let inner_dirs = 3;
        for j in 0..inner_dirs {
            let inner = sub.join(format!("module_{}", j));
            fs::create_dir_all(&inner).unwrap();
            let files_here = (target_count - created) / (dir_count * inner_dirs).max(1) + 1;
            for k in 0..files_here {
                if created >= target_count {
                    return created;
                }
                let r = pseudo_rand((d * 1000 + j * 100 + k) as u64);
                let bucket = r % 100;
                let size = if bucket < 70 {
                    1024 + (r as usize % 3072) // 1-4KB
                } else if bucket < 95 {
                    10 * 1024 + (r as usize % (90 * 1024)) // 10-100KB
                } else {
                    1024 * 1024 + (r as usize % (9 * 1024 * 1024)) // 1-10MB
                };
                let f = inner.join(format!("f_{}.dat", k));
                write_file(&f, &vec![b'r'; size.min(2 * 1024 * 1024)]); // 上限 2MB 避免吃满磁盘
                created += 1;
            }
        }
    }
    created
}

#[derive(Serialize)]
struct ScanPayload {
    total_files: usize,
    total_dirs: usize,
    total_size: u64,
    backend: Option<String>,
    inaccessible: usize,
}

#[derive(Serialize)]
struct SafetyPayload {
    verdict: String,
    findings_count: usize,
    can_migrate: bool,
    duration_ms: u64,
}

#[derive(Serialize)]
struct DbInsertPayload {
    inserted_id: i64,
}

#[derive(Serialize)]
struct DbQueryPayload {
    record_count: usize,
}

#[derive(Serialize)]
struct E2EPayload {
    scan_ms: f64,
    safety_ms: f64,
    migrate_ms: f64,
    rollback_ms: f64,
    bytes: u64,
}

fn scan_payload(r: &cdrive_cleaner_lib::scanner::file_info::ScanResult) -> ScanPayload {
    ScanPayload {
        total_files: r.total_files,
        total_dirs: r.total_dirs,
        total_size: r.total_size,
        backend: r.scan_backend.clone(),
        inaccessible: r.inaccessible_count,
    }
}

// =========================================================================
// 实验 1：扫描——不同规模 + 不同形状
// =========================================================================

async fn scan_series<F>(
    name: &str,
    desc: String,
    n: usize,
    setup: F,
) -> BenchSeries
where
    F: FnOnce(&Path) -> usize,
{
    let ws = temp_workspace(name);
    let actual = setup(&ws);
    println!("[bench] {} —— 实际生成 {} 个文件，重复 {} 次", name, actual, n);

    let scanner = DiskScanner::new();
    let samples = repeat_async(n, |_i| {
        let scanner = &scanner;
        let path = ws.clone();
        async move {
            let r = scanner
                .scan_deep_silent(&path, actual.max(100))
                .await
                .expect("scan 应成功");
            scan_payload(&r)
        }
    })
    .await;

    fs::remove_dir_all(&ws).ok();
    BenchSeries::new(name.to_string(), desc, samples)
}

async fn bench_scan_30_files(n: usize) -> BenchSeries {
    scan_series(
        "scan_30_files_shallow",
        "扫描 30 文件浅树（fanout=2, depth=2, files_per_leaf=2）".to_string(),
        n,
        |p| make_synthetic_tree(p, 2, 2, 2, 256),
    )
    .await
}

async fn bench_scan_100_files(n: usize) -> BenchSeries {
    scan_series(
        "scan_100_files_balanced",
        "扫描 ~100 文件均衡树（fanout=3, depth=3, files_per_leaf=2, file=256B）".to_string(),
        n,
        |p| make_synthetic_tree(p, 3, 3, 2, 256),
    )
    .await
}

async fn bench_scan_1k_files(n: usize) -> BenchSeries {
    scan_series(
        "scan_1k_files_balanced",
        "扫描 ~1k 文件均衡树（fanout=4, depth=4, files_per_leaf=4, file=1KB）".to_string(),
        n,
        |p| make_synthetic_tree(p, 4, 4, 4, 1024),
    )
    .await
}

async fn bench_scan_deep_narrow(n: usize) -> BenchSeries {
    scan_series(
        "scan_deep_narrow_tree",
        "扫描深窄树（fanout=2, depth=8, files_per_leaf=1, file=512B）—— 路径深度大".to_string(),
        n,
        |p| make_synthetic_tree(p, 2, 8, 1, 512),
    )
    .await
}

async fn bench_scan_shallow_wide(n: usize) -> BenchSeries {
    scan_series(
        "scan_shallow_wide_tree",
        "扫描浅宽树（fanout=20, depth=2, files_per_leaf=10, file=512B）—— 单层文件多".to_string(),
        n,
        |p| make_synthetic_tree(p, 20, 2, 10, 512),
    )
    .await
}

async fn bench_scan_realistic_1k(n: usize) -> BenchSeries {
    scan_series(
        "scan_realistic_mixed_1k",
        "扫描 ~1000 文件混合大小目录（70% 小 1-4KB / 25% 中 10-100KB / 5% 大 1-2MB）".to_string(),
        n,
        |p| make_realistic_tree(p, 1000),
    )
    .await
}

async fn bench_scan_10k_files(n: usize) -> BenchSeries {
    scan_series(
        "scan_10k_files_realistic",
        "扫描 ~10000 文件混合大小（默认 5 次重复，可通过 BENCH_LARGE_REPEAT 调整）".to_string(),
        n,
        |p| make_realistic_tree(p, 10_000),
    )
    .await
}

/// 冷热缓存对比：第一次扫（冷）+ 紧接着第二次扫（热）。
/// 系统的文件系统缓存会让第二次显著加速。
async fn bench_scan_cache_warmup(n: usize) -> Vec<BenchSeries> {
    println!("[bench] cache_warmup —— 准备数据...");
    let ws = temp_workspace("cache");
    let _files = make_synthetic_tree(&ws, 4, 4, 4, 1024);
    let scanner = DiskScanner::new();

    // 冷扫：每次先把同一棵树扫一遍，但中间不做缓存清理。第一次因为构建期 IO 还热着，
    // 这里语义上是 "首次加载"。要真彻底冷只能重启或 clear cache，受限于权限不做。
    let mut cold = Vec::with_capacity(n);
    let mut hot = Vec::with_capacity(n);
    for i in 0..n {
        // 冷：当作一次首扫
        let s1 = std::time::Instant::now();
        let r1 = scanner.scan_deep_silent(&ws, 5000).await.expect("scan");
        let d1 = s1.elapsed().as_secs_f64() * 1000.0;
        // 热：紧接着再扫一次
        let s2 = std::time::Instant::now();
        let r2 = scanner.scan_deep_silent(&ws, 5000).await.expect("scan");
        let d2 = s2.elapsed().as_secs_f64() * 1000.0;

        cold.push(cdrive_cleaner_lib::bench::Sample {
            run_index: i,
            duration_ms: d1,
            payload: serde_json::to_value(scan_payload(&r1)).unwrap_or_default(),
        });
        hot.push(cdrive_cleaner_lib::bench::Sample {
            run_index: i,
            duration_ms: d2,
            payload: serde_json::to_value(scan_payload(&r2)).unwrap_or_default(),
        });
    }
    fs::remove_dir_all(&ws).ok();

    vec![
        BenchSeries::new(
            "scan_cold_first_pass",
            "冷扫：每轮第一次扫描 1k 文件树（FS cache 较少命中）",
            cold,
        ),
        BenchSeries::new(
            "scan_hot_second_pass",
            "热扫：紧接冷扫之后第二次扫同一树（FS cache 命中）",
            hot,
        ),
    ]
}

/// 真实目录扫描，需要 BENCH_REAL_PATH 环境变量。
async fn bench_scan_real_path(n: usize) -> Option<BenchSeries> {
    let path = std::env::var("BENCH_REAL_PATH").ok()?;
    let p = PathBuf::from(&path);
    if !p.exists() {
        eprintln!("[bench] BENCH_REAL_PATH={} 不存在，跳过", path);
        return None;
    }
    println!("[bench] 真实路径扫描：{} × {}", path, n);
    let scanner = DiskScanner::new();
    let samples = repeat_async(n, |_i| {
        let scanner = &scanner;
        let path = p.clone();
        async move {
            let r = scanner
                .scan_deep_silent(&path, 100_000)
                .await
                .expect("scan 真实路径应成功");
            scan_payload(&r)
        }
    })
    .await;
    Some(BenchSeries::new(
        "scan_real_path",
        format!("扫描真实路径 {}（来自 BENCH_REAL_PATH 环境变量）", path),
        samples,
    ))
}

// =========================================================================
// 实验 2：安全检测在多种典型路径下的耗时
// =========================================================================

fn bench_safety_typical_paths(n: usize) -> Vec<BenchSeries> {
    let user = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\admin".to_string());
    let cases = vec![
        (
            "safety_windows_dir".to_string(),
            r"C:\Windows".to_string(),
            "C:\\Windows（系统关键目录，期望 Blocker）".to_string(),
        ),
        (
            "safety_program_files".to_string(),
            r"C:\Program Files".to_string(),
            "C:\\Program Files（应用安装目录，期望禁止）".to_string(),
        ),
        (
            "safety_program_files_x86".to_string(),
            r"C:\Program Files (x86)".to_string(),
            "C:\\Program Files (x86)".to_string(),
        ),
        (
            "safety_user_desktop".to_string(),
            format!(r"{}\Desktop\some-folder", user),
            "用户桌面下普通子目录".to_string(),
        ),
        (
            "safety_user_appdata".to_string(),
            format!(r"{}\AppData\Local\some-app", user),
            "用户 AppData\\Local 下子目录".to_string(),
        ),
        (
            "safety_temp_dir".to_string(),
            format!(r"{}\AppData\Local\Temp\foo", user),
            "用户 Temp 目录下子目录".to_string(),
        ),
    ];

    let mut series = Vec::new();
    for (name, path, desc) in cases {
        println!("[bench] {} × {}", name, n);
        let path_clone = path.clone();
        let samples = repeat(n, |_| {
            let s = analyze(
                Path::new(&path_clone),
                LinkType::Auto,
                Some("D:"),
                1024 * 1024 * 1024,
            );
            SafetyPayload {
                verdict: format!("{:?}", s.verdict),
                findings_count: s.findings.len(),
                can_migrate: s.can_migrate,
                duration_ms: s.analysis_duration_ms,
            }
        });
        series.push(BenchSeries::new(name, desc, samples));
    }
    series
}

// =========================================================================
// 实验 3：数据库 —— insert / 查询 / 统计在不同规模
// =========================================================================

fn make_temp_db() -> (MigrationDb, PathBuf) {
    let db_path = std::env::temp_dir().join(format!(
        "csd-bench-db-{}-{}.sqlite",
        std::process::id(),
        chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    let db = MigrationDb::new(db_path.to_string_lossy().as_ref()).unwrap();
    (db, db_path)
}

fn cleanup_db(db_path: &Path) {
    fs::remove_file(db_path).ok();
    fs::remove_file(format!("{}-shm", db_path.display())).ok();
    fs::remove_file(format!("{}-wal", db_path.display())).ok();
}

fn bench_db_insert(n: usize) -> BenchSeries {
    let (db, db_path) = make_temp_db();
    println!("[bench] db_insert × {}", n);
    let samples = repeat(n, |i| {
        let id = db
            .insert_migration(
                &format!(r"C:\src\path_{}", i),
                &format!(r"D:\dst\path_{}", i),
                "Junction",
                (i as u64) * 1024,
            )
            .unwrap();
        DbInsertPayload { inserted_id: id }
    });
    drop(db);
    cleanup_db(&db_path);
    BenchSeries::new(
        "db_insert_migration",
        "向 SQLite migration_db 插入一条记录",
        samples,
    )
}

fn bench_db_query_at_scale(n: usize, scale: usize) -> BenchSeries {
    let (db, db_path) = make_temp_db();
    // 预填 scale 条
    for i in 0..scale {
        db.insert_migration(
            &format!(r"C:\src\{}", i),
            &format!(r"D:\dst\{}", i),
            "Junction",
            (i as u64) * 4096,
        )
        .unwrap();
    }
    println!("[bench] db_query_all_at_{} × {}", scale, n);
    let samples = repeat(n, |_| {
        let recs = db.get_all_migrations().unwrap();
        DbQueryPayload {
            record_count: recs.len(),
        }
    });
    drop(db);
    cleanup_db(&db_path);
    BenchSeries::new(
        format!("db_query_all_at_{}_records", scale),
        format!("get_all_migrations() 在 {} 条记录的库上查询", scale),
        samples,
    )
}

// =========================================================================
// 实验 4：端到端 —— 扫描 + 安全分析 + 文件迁移 + 回滚
// =========================================================================

async fn bench_end_to_end(n: usize) -> BenchSeries {
    println!("[bench] end_to_end × {}", n);
    let scanner = DiskScanner::new();
    let migrator = FileMigrator::new();

    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
        let ws = temp_workspace(&format!("e2e-{}", i));
        let src_root = ws.join("source");
        let dst_root = ws.join("target");
        fs::create_dir_all(&src_root).unwrap();
        fs::create_dir_all(&dst_root).unwrap();

        // 制造一个小目录树作为迁移源
        let src_dir = src_root.join("payload");
        make_synthetic_tree(&src_dir, 2, 2, 3, 4096);

        let total_start = std::time::Instant::now();

        // 1. 扫描
        let s1 = std::time::Instant::now();
        let scan = scanner
            .scan_deep_silent(&src_dir, 100)
            .await
            .expect("scan ok");
        let scan_ms = s1.elapsed().as_secs_f64() * 1000.0;

        // 2. 安全分析
        let s2 = std::time::Instant::now();
        let safety = analyze(&src_dir, LinkType::Junction, Some("D:"), scan.total_size);
        let safety_ms = s2.elapsed().as_secs_f64() * 1000.0;
        if !safety.can_migrate {
            // 应该是 Safe，但万一不让迁移就跳过迁移段
            samples.push(cdrive_cleaner_lib::bench::Sample {
                run_index: i,
                duration_ms: total_start.elapsed().as_secs_f64() * 1000.0,
                payload: serde_json::to_value(E2EPayload {
                    scan_ms,
                    safety_ms,
                    migrate_ms: 0.0,
                    rollback_ms: 0.0,
                    bytes: scan.total_size,
                })
                .unwrap_or_default(),
            });
            fs::remove_dir_all(&ws).ok();
            continue;
        }

        // 3. 迁移（用 junction，跨"盘"在测试里其实是同盘，但 link 创建逻辑一致）
        let s3 = std::time::Instant::now();
        let res = migrator
            .migrate(&src_dir, &dst_root, LinkType::Junction, None, None)
            .await
            .expect("migrate ok");
        let migrate_ms = s3.elapsed().as_secs_f64() * 1000.0;

        // 4. 回滚
        let s4 = std::time::Instant::now();
        let rb = migrator
            .rollback(
                Path::new(&res.source_path),
                Path::new(&res.target_path),
            )
            .await
            .expect("rollback ok");
        let rollback_ms = s4.elapsed().as_secs_f64() * 1000.0;
        assert!(rb.success);

        let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;
        samples.push(cdrive_cleaner_lib::bench::Sample {
            run_index: i,
            duration_ms: total_ms,
            payload: serde_json::to_value(E2EPayload {
                scan_ms,
                safety_ms,
                migrate_ms,
                rollback_ms,
                bytes: scan.total_size,
            })
            .unwrap_or_default(),
        });

        fs::remove_dir_all(&ws).ok();
    }

    BenchSeries::new(
        "end_to_end_scan_safety_migrate_rollback",
        "完整链路：扫描 ~30 文件 → 安全分析 → junction 迁移 → 回滚（payload 字段含每段耗时）",
        samples,
    )
}

// =========================================================================
// 入口测试
// =========================================================================

#[tokio::test]
#[ignore = "重型基准测试，需手动启用：cargo test --features bench --test bench_runner -- --ignored --nocapture"]
async fn run_all_benchmarks() {
    let n = repeat_count();
    let n_large = large_repeat_count();
    let out = output_dir();
    let ts = timestamp();
    println!("===== 基准测试开始 (普通={} 次, 大规模={} 次) =====", n, n_large);

    // 预热
    println!("[warmup] 跑两次空转预热...");
    for _ in 0..2 {
        let ws = temp_workspace("warmup");
        make_synthetic_tree(&ws, 2, 2, 2, 128);
        let _ = DiskScanner::new().scan_deep_silent(&ws, 100).await;
        fs::remove_dir_all(&ws).ok();
    }

    let mut all = Vec::new();

    // 扫描
    all.push(bench_scan_30_files(n).await);
    all.push(bench_scan_100_files(n).await);
    all.push(bench_scan_1k_files(n).await);
    all.push(bench_scan_deep_narrow(n).await);
    all.push(bench_scan_shallow_wide(n).await);
    all.push(bench_scan_realistic_1k(n).await);
    all.push(bench_scan_10k_files(n_large).await);
    all.extend(bench_scan_cache_warmup(n).await);
    if let Some(s) = bench_scan_real_path(n_large.max(3)).await {
        all.push(s);
    }

    // 安全
    all.extend(bench_safety_typical_paths(n));

    // 数据库
    all.push(bench_db_insert(n));
    all.push(bench_db_query_at_scale(n, 10));
    all.push(bench_db_query_at_scale(n, 100));
    all.push(bench_db_query_at_scale(n, 1000));

    // 端到端
    all.push(bench_end_to_end(n.min(15)).await); // 限制次数避免太慢

    let report = BenchReport {
        system: SystemInfo::capture(),
        series: all,
    };

    let json_path = out.join(format!("{}.json", ts));
    fs::write(&json_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();

    let md_path = out.join(format!("{}.md", ts));
    let md = render_markdown(
        &format!("CDrive Cleaner 基准测试报告 · {}", ts),
        &report,
    );
    fs::write(&md_path, &md).unwrap();

    println!("\n===== 基准测试完成 =====");
    println!("JSON: {}", json_path.display());
    println!("Markdown: {}", md_path.display());

    // 打摘要表到 stdout
    let summary: Vec<&str> = md.lines().take_while(|l| !l.starts_with("## scan_") && !l.starts_with("## safety_") && !l.starts_with("## db_") && !l.starts_with("## end_")).collect();
    println!("\n{}", summary.join("\n"));
}
