use crate::safety::MigrationSafety;
use crate::scanner::env_fingerprint::EnvFingerprint;
use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub system_reserved_bytes: u64,
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
    /// 缓存 schema 版本号；落盘时由 commands.rs 显式写入 `CACHE_SCHEMA_VERSION`，
    /// 老缓存反序列化得到 `0`，会被视为脏。
    #[serde(default)]
    pub cache_schema_version: u32,
    /// 这次扫描结果对应的环境指纹。读缓存时与当前指纹做完全相等比较。
    #[serde(default)]
    pub env_fingerprint: EnvFingerprint,
    /// 扫描是否成功完成并落盘。`false` 代表占位记录或中途失败，下次读取直接判脏。
    #[serde(default)]
    pub scan_completed: bool,
}

/// 增量扫描中对单个目录的"重扫决策"。
///
/// - `Recursive`：整棵子树重扫，最贵但最权威；任何祖先一旦被判 `Recursive`，
///   它的后代就不需要再单独 `DirectFilesOnly`，否则会出现"祖先已经替换为最新
///   子树"和"后代 clone 旧 children"的覆盖竞态（参见 incremental.rs 父子统计 bug）。
/// - `DirectFilesOnly`：只刷新目录自己直接持有的文件统计，子目录沿用缓存。
///   适用于只是新增/删除文件、没有触及子目录的场景。
/// - `Skip`：本次扫描里没有变化，直接复用缓存。保留这一项主要是给上层调用者
///   表达"我考虑过这个目录，但没必要重扫"用的。
///
/// 这是个纯 enum、零字段，用来在 incremental 的检测/合并阶段统一表达决策；
/// 不会被序列化进 `ScanResult` 或缓存里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RescanDecision {
    Recursive,
    DirectFilesOnly,
    Skip,
}
