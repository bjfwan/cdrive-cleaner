#[derive(Clone, serde::Serialize)]
pub struct ScanProgress {
    pub scanned_files: u64,
    pub scanned_dirs: u64,
    pub total_size: u64,
    pub current_path: String,
    pub elapsed_ms: u64,
    pub files_per_second: f64,
    pub progress_percent: f64,
}
