use std::path::PathBuf;

use anyhow::Result;

fn main() -> Result<()> {
    let target = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\"));

    let report = cdrive_cleaner_lib::diagnostics::run_mft_end_to_end_validation(&target)?;
    println!("{}", serde_json::to_string_pretty(&report)?);

    if !report.passed {
        std::process::exit(2);
    }

    Ok(())
}
