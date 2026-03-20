use serde::{Deserialize, Serialize};
use crate::safety::MigrationSafety;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub extension: String,
    pub modified_at: String,
    pub is_readonly: bool,
    #[serde(default)]
    pub is_symlink: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryNode {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub file_count: usize,
    #[serde(default)]
    pub dir_count: usize,
    pub children: Vec<DirectoryNode>,
    #[serde(default)]
    pub has_children: bool,
    pub is_symlink: bool,
    pub link_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety: Option<MigrationSafety>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_time: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScanResult {
    pub root_path: String,
    pub total_size: u64,
    pub total_files: usize,
    pub total_dirs: usize,
    pub scan_duration_ms: u64,
    pub directories: Vec<DirectoryNode>,
    pub large_files: Vec<FileInfo>,
    pub inaccessible_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_file_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usn_journal_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usn_next_usn: Option<i64>,
}
