//! 单独 profile 安全检测各个 gate，找出 `C:\Program Files` 慢在哪。
//!
//! 跑：
//! ```powershell
//! cargo test --features bench --test safety_profile -- --ignored --nocapture
//! ```

#![cfg(all(target_os = "windows", feature = "bench"))]

use cdrive_cleaner_lib::migration::LinkType;
use cdrive_cleaner_lib::safety::analyze;
use std::path::Path;
use std::time::Instant;

fn run_n(label: &str, path: &str, link_type: LinkType, n: usize) -> Vec<f64> {
    println!("\n=== {} @ {} ===", label, path);
    let mut times = Vec::with_capacity(n);
    let mut last_durations: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    for i in 0..n {
        let start = Instant::now();
        let s = analyze(Path::new(path), link_type.clone(), Some("D:"), 1024 * 1024 * 1024);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        times.push(elapsed);
        last_durations = s.gate_durations_ms.clone();
        if i == 0 || i == n - 1 {
            println!(
                "  run {}: {:.1} ms | verdict={:?} | findings={}",
                i,
                elapsed,
                s.verdict,
                s.findings.len(),
            );
        }
    }
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let mut sorted = times.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[n / 2];
    println!("  → total: mean={:.1} ms, median={:.1} ms, min={:.1}, max={:.1}",
             mean, median, sorted[0], sorted[n - 1]);
    // 各 gate 单独耗时（只打最后一次的，足够定位瓶颈）
    let mut gate_pairs: Vec<_> = last_durations.iter().collect();
    gate_pairs.sort_by_key(|(_, ms)| std::cmp::Reverse(**ms));
    println!("  → per-gate (last run, sorted desc):");
    for (name, ms) in gate_pairs {
        println!("       {:>20} : {} ms", name, ms);
    }
    times
}

#[test]
#[ignore = "profile 用，手动启用"]
fn profile_program_files_breakdown() {
    println!("===== Safety Detector Profile =====\n");
    println!("先 warmup 一次让 OnceLock 注册表索引建好...");
    let _ = analyze(
        Path::new(r"C:\Windows"),
        LinkType::Auto,
        Some("D:"),
        0,
    );
    println!("warmup 完成。\n");

    // 分场景测，重点看 LinkType=Auto vs LinkType=None 是不是触发了不同 gate
    let n = 5;

    // Auto link type（默认会用 junction，不触发 registry_bindings 重检查）
    run_n("program_files (LinkType::Auto)", r"C:\Program Files", LinkType::Auto, n);
    run_n("program_files_x86 (Auto)", r"C:\Program Files (x86)", LinkType::Auto, n);
    run_n("windows (Auto)", r"C:\Windows", LinkType::Auto, n);

    // None link type → gate_registry_bindings 真的会跑（会全表搜服务/COM/AppPaths/Tasks/Uninstall）
    run_n("program_files (LinkType::None)", r"C:\Program Files", LinkType::None, n);

    // 不存在的路径
    run_n("nonexistent (Auto)", r"C:\definitely-does-not-exist-12345", LinkType::Auto, n);

    // 用户目录
    let user = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\admin".to_string());
    run_n("user_profile (Auto)", &user, LinkType::Auto, n);
}
