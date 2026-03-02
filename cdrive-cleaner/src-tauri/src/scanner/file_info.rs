use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub extension: String,
    pub modified_at: String,
    pub is_readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryNode {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub file_count: usize,
    pub children: Vec<DirectoryNode>,
    pub is_symlink: bool,
    pub link_target: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResult {
    pub root_path: String,
    pub total_size: u64,
    pub total_files: usize,
    pub total_dirs: usize,
    pub scan_duration_ms: u64,
    pub directories: Vec<DirectoryNode>,
    pub inaccessible_count: usize,
}
