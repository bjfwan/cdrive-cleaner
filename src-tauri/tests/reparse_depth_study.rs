//! 在真实路径上比较不同 max_depth 值的 reparse point 检出量与耗时。
//! 跑：
//! ```powershell
//! cargo test --features bench --test reparse_depth_study -- --ignored --nocapture
//! ```

#![cfg(all(target_os = "windows", feature = "bench"))]

use std::path::Path;
use std::time::Instant;

fn is_reparse(p: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    p.symlink_metadata()
        .map(|m| (m.file_attributes() & 0x400) != 0) // FILE_ATTRIBUTE_REPARSE_POINT
        .unwrap_or(false)
}

fn count_with_depth(root: &Path, max_depth: usize, max_entries: usize) -> (usize, usize, u64) {
    let start = Instant::now();
    let walker = jwalk::WalkDir::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .max_depth(max_depth);

    let mut visited = 0usize;
    let mut reparse_count = 0usize;
    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        if visited >= max_entries {
            break;
        }
        visited += 1;
        if entry.path() == root {
            continue;
        }
        if entry.file_type().is_symlink() || is_reparse(&entry.path()) {
            reparse_count += 1;
        }
    }
    (visited, reparse_count, start.elapsed().as_millis() as u64)
}

#[test]
#[ignore = "重型 IO，手动启用"]
fn study_reparse_depth_tradeoff() {
    let user = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\admin".to_string());
    let cases: Vec<(&str, String)> = vec![
        ("C:\\Program Files", r"C:\Program Files".to_string()),
        ("C:\\Program Files (x86)", r"C:\Program Files (x86)".to_string()),
        ("C:\\Users\\<user>", user),
    ];

    let depth_caps = [2usize, 3, 4, 5, 6, 8, 12];
    let entry_cap = 50_000;

    for (label, path) in &cases {
        if !Path::new(path).exists() {
            println!("{} 不存在，跳过", label);
            continue;
        }
        println!("\n===== {} =====", label);
        println!("{:<8} {:<14} {:<14} {:<10}", "depth", "visited", "reparse_pts", "ms");
        println!("{}", "-".repeat(54));
        for &depth in &depth_caps {
            let (visited, found, ms) = count_with_depth(Path::new(path), depth, entry_cap);
            println!("{:<8} {:<14} {:<14} {:<10}", depth, visited, found, ms);
            if visited >= entry_cap {
                println!("  ↑ 触发 entry_cap，已截断");
            }
        }
    }
}
