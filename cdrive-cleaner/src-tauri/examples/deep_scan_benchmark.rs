use std::path::PathBuf;

use anyhow::Result;

fn main() -> Result<()> {
    let target = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\"));
    let estimated_files = std::env::args()
        .nth(2)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(800_000);

    let report =
        cdrive_cleaner_lib::diagnostics::run_deep_scan_benchmark(&target, estimated_files)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
