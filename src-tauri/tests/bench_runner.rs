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
//! 包含 `<时间戳>.json`（原始数据）和 `<时间戳>.md`（人类可读摘要）。
//!
//! 可通过环境变量调参：
//! - `BENCH_REPEAT`: 每个场景重复次数，默认 30
//! - `BENCH_OUTPUT_DIR`: 输出目录
//!
//! ## 设计目标
//!
//! 给"扫描 / 安全检测 / 数据库写入"三类操作做可重复实验，输出均值、中位数、p95、
//! 标准差、变异系数，方便事后做数据分析。

#![cfg(all(target_os = "windows", feature = "bench"))]

use cdrive_cleaner_lib::bench::{render_markdown, repeat, repeat_async, BenchSeries};
use cdrive_cleaner_lib::database::MigrationDb;
use cdrive_cleaner_lib::migration::LinkType;
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
/// 返回总共创建的文件数，方便后续断言。
fn make_synthetic_tree(root: &Path, fanout: usize, depth: usize, files_per_leaf: usize, file_size: usize) -> usize {
    fn recurse(parent: &Path, fanout: usize, depth: usize, files_per_leaf: usize, file_size: usize, count: &mut usize) {
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

#[derive(Serialize)]
struct ScanPayload {
    total_files: usize,
    total_dirs: usize,
    total_size: u64,
    backend: Option<String>,
}

#[derive(Serialize)]
struct SafetyPayload {
    verdict: String,
    findings_count: usize,
    can_migrate: bool,
}

#[derive(Serialize)]
struct DbPayload {
    inserted_id: i64,
}

// =========================================================================
// 实验 1：扫描小规模目录树（约 ~100 文件）
// =========================================================================

async fn bench_scan_small_tree(n: usize) -> BenchSeries {
    let ws = temp_workspace("scan-small");
    let expected_files = make_synthetic_tree(&ws, 3, 3, 2, 256);
    println!("[bench-scan-small] 生成 {} 个文件，开始 {} 次重复扫描...", expected_files, n);

    let scanner = DiskScanner::new();
    let samples = repeat_async(n, |_i| {
        let scanner = &scanner;
        let path = ws.clone();
        async move {
            let r = scanner.scan_deep_silent(&path, 1000).await.expect("scan 应成功");
            ScanPayload {
                total_files: r.total_files,
                total_dirs: r.total_dirs,
                total_size: r.total_size,
                backend: r.scan_backend.clone(),
            }
        }
    })
    .await;

    fs::remove_dir_all(&ws).ok();
    BenchSeries::new(
        "scan_small_tree",
        format!(
            "扫描合成目录树（fanout=3, depth=3, files_per_leaf=2, file_size=256B），共 {} 文件",
            expected_files
        ),
        samples,
    )
}

// =========================================================================
// 实验 2：扫描中等规模目录树（约 ~1k 文件）
// =========================================================================

async fn bench_scan_medium_tree(n: usize) -> BenchSeries {
    let ws = temp_workspace("scan-medium");
    let expected_files = make_synthetic_tree(&ws, 4, 4, 4, 1024);
    println!("[bench-scan-medium] 生成 {} 个文件，开始 {} 次重复扫描...", expected_files, n);

    let scanner = DiskScanner::new();
    let samples = repeat_async(n, |_i| {
        let scanner = &scanner;
        let path = ws.clone();
        async move {
            let r = scanner.scan_deep_silent(&path, 5000).await.expect("scan 应成功");
            ScanPayload {
                total_files: r.total_files,
                total_dirs: r.total_dirs,
                total_size: r.total_size,
                backend: r.scan_backend.clone(),
            }
        }
    })
    .await;

    fs::remove_dir_all(&ws).ok();
    BenchSeries::new(
        "scan_medium_tree",
        format!(
            "扫描合成目录树（fanout=4, depth=4, files_per_leaf=4, file_size=1KB），共 {} 文件",
            expected_files
        ),
        samples,
    )
}

// =========================================================================
// 实验 3：安全检测在典型路径下的耗时
// =========================================================================

fn bench_safety_typical_paths(n: usize) -> Vec<BenchSeries> {
    let cases = [
        ("safety_windows_dir", r"C:\Windows", "C:\\Windows（系统关键目录）"),
        ("safety_program_files", r"C:\Program Files", "C:\\Program Files（应用安装目录）"),
        ("safety_user_desktop", r"C:\Users\admin\Desktop\some-app", "用户桌面下普通子目录"),
        ("safety_temp_dir", r"C:\Users\admin\AppData\Local\Temp\foo", "用户 Temp 目录"),
    ];

    let mut series = Vec::new();
    for (name, path, desc) in cases {
        println!("[bench-safety] {} × {}...", name, n);
        let path = path.to_string();
        let samples = repeat(n, |_| {
            let s = analyze(Path::new(&path), LinkType::Auto, Some("D:"), 1024 * 1024 * 1024);
            SafetyPayload {
                verdict: format!("{:?}", s.verdict),
                findings_count: s.findings.len(),
                can_migrate: s.can_migrate,
            }
        });
        series.push(BenchSeries::new(name.to_string(), desc.to_string(), samples));
    }
    series
}

// =========================================================================
// 实验 4：数据库写入吞吐
// =========================================================================

fn bench_db_inserts(n: usize) -> BenchSeries {
    let db_path = std::env::temp_dir().join(format!("csd-bench-db-{}.sqlite", chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)));
    let db = MigrationDb::new(db_path.to_string_lossy().as_ref()).unwrap();
    println!("[bench-db] 开始 {} 次 insert...", n);

    let samples = repeat(n, |i| {
        let id = db
            .insert_migration(
                &format!(r"C:\src\path_{}", i),
                &format!(r"D:\dst\path_{}", i),
                "Junction",
                (i as u64) * 1024,
            )
            .unwrap();
        DbPayload { inserted_id: id }
    });

    drop(db);
    fs::remove_file(&db_path).ok();
    fs::remove_file(format!("{}-shm", db_path.display())).ok();
    fs::remove_file(format!("{}-wal", db_path.display())).ok();

    BenchSeries::new(
        "db_insert_migration",
        "向 SQLite migration_db 插入一条记录的耗时",
        samples,
    )
}

// =========================================================================
// 入口测试
// =========================================================================

/// 跑全部基准并落盘报告。带 #[ignore]，平时 `cargo test` 不会触发。
#[tokio::test]
#[ignore = "重型基准测试，需手动启用：cargo test --features bench --test bench_runner -- --ignored --nocapture"]
async fn run_all_benchmarks() {
    let n = repeat_count();
    let out = output_dir();
    let ts = timestamp();
    println!("===== 基准测试开始 ({} 次重复) =====", n);

    // 预热：先无统计跑一次每个场景，避免首次缓存影响数据。
    println!("[warmup] 跑一次空转预热...");
    {
        let ws = temp_workspace("warmup");
        make_synthetic_tree(&ws, 2, 2, 2, 128);
        let s = DiskScanner::new();
        let _ = s.scan_deep_silent(&ws, 100).await;
        fs::remove_dir_all(&ws).ok();
    }

    let mut all = Vec::new();
    all.push(bench_scan_small_tree(n).await);
    all.push(bench_scan_medium_tree(n).await);
    all.extend(bench_safety_typical_paths(n));
    all.push(bench_db_inserts(n));

    // JSON：完整原始数据
    let json_path = out.join(format!("{}.json", ts));
    let json = serde_json::to_string_pretty(&all).unwrap();
    fs::write(&json_path, json).unwrap();

    // Markdown：人类可读摘要
    let md = render_markdown(
        &format!("CDrive Cleaner 基准测试报告 · {}", ts),
        &all,
    );
    let md_path = out.join(format!("{}.md", ts));
    fs::write(&md_path, &md).unwrap();

    println!("\n===== 基准测试完成 =====");
    println!("JSON: {}", json_path.display());
    println!("Markdown: {}", md_path.display());
    println!("\n摘要：");
    println!("{}", md.lines().take_while(|l| !l.starts_with("## ") || l.contains("摘要")).collect::<Vec<_>>().join("\n"));
}
