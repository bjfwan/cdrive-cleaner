use super::file_info::{DirectoryNode, ScanResult};
use anyhow::Result;
use std::path::Path;
use std::time::Instant;

pub struct DiskScanner;

impl DiskScanner {
    pub fn new() -> Self {
        Self
    }

    pub async fn scan<P: AsRef<Path>>(&self, path: P) -> Result<ScanResult> {
        let start = Instant::now();
        let root_path = path.as_ref().to_string_lossy().to_string();

        let directories = vec![];
        
        Ok(ScanResult {
            root_path,
            total_size: 0,
            total_files: 0,
            total_dirs: 0,
            scan_duration_ms: start.elapsed().as_millis() as u64,
            directories,
            inaccessible_count: 0,
        })
    }
}
