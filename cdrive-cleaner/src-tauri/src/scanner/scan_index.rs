use super::file_info::{DirectoryNode, FileInfo, ScanResult};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone)]
pub struct IndexedScanResult {
    root_path: String,
    total_size: u64,
    total_files: usize,
    total_dirs: usize,
    scan_duration_ms: u64,
    inaccessible_count: usize,
    root_children: Vec<DirectoryNode>,
    nodes: HashMap<String, DirectoryNode>,
    children_by_path: HashMap<String, Vec<DirectoryNode>>,
    large_files: Vec<FileInfo>,
}

impl IndexedScanResult {
    pub fn from_scan_result(result: &ScanResult) -> Self {
        let mut indexed = Self {
            root_path: result.root_path.clone(),
            total_size: result.total_size,
            total_files: result.total_files,
            total_dirs: result.total_dirs,
            scan_duration_ms: result.scan_duration_ms,
            inaccessible_count: result.inaccessible_count,
            root_children: result
                .directories
                .iter()
                .map(Self::strip_node)
                .collect(),
            nodes: HashMap::new(),
            children_by_path: HashMap::new(),
            large_files: result.large_files.clone(),
        };

        indexed.index_children(&result.root_path, &result.directories);
        indexed
    }

    pub fn snapshot_for_path(&self, path: &str) -> Option<ScanResult> {
        if path == self.root_path {
            return Some(ScanResult {
                root_path: self.root_path.clone(),
                total_size: self.total_size,
                total_files: self.total_files,
                total_dirs: self.total_dirs,
                scan_duration_ms: self.scan_duration_ms,
                directories: self.root_children.clone(),
                large_files: self.large_files.clone(),
                inaccessible_count: self.inaccessible_count,
            });
        }

        let node = self.nodes.get(path)?;
        let directories = self.children_by_path.get(path).cloned().unwrap_or_default();

        Some(ScanResult {
            root_path: node.path.clone(),
            total_size: node.size,
            total_files: node.file_count,
            total_dirs: node.dir_count.saturating_sub(1),
            scan_duration_ms: self.scan_duration_ms,
            directories,
            large_files: self.large_files_for_path(path),
            inaccessible_count: self.inaccessible_count,
        })
    }

    fn large_files_for_path(&self, path: &str) -> Vec<FileInfo> {
        let current = Path::new(path);
        let mut filtered: Vec<FileInfo> = self
            .large_files
            .iter()
            .filter(|file| Path::new(&file.path).starts_with(current))
            .cloned()
            .collect();
        filtered.sort_by(|a, b| b.size.cmp(&a.size));
        filtered
    }

    fn index_children(&mut self, parent_path: &str, children: &[DirectoryNode]) {
        let compact_children: Vec<DirectoryNode> = children.iter().map(Self::strip_node).collect();
        self.children_by_path
            .insert(parent_path.to_string(), compact_children);

        for child in children {
            if Self::is_root_files_node(&self.root_path, child) {
                continue;
            }

            self.nodes.insert(child.path.clone(), Self::strip_node(child));
            self.index_children(&child.path, &child.children);
        }
    }

    fn strip_node(node: &DirectoryNode) -> DirectoryNode {
        DirectoryNode {
            path: node.path.clone(),
            name: node.name.clone(),
            size: node.size,
            file_count: node.file_count,
            dir_count: node.dir_count,
            children: vec![],
            is_symlink: node.is_symlink,
            link_target: node.link_target.clone(),
            has_children: node.has_children || !node.children.is_empty(),
            safety: node.safety.clone(),
            modified_time: node.modified_time,
        }
    }

    fn is_root_files_node(root_path: &str, node: &DirectoryNode) -> bool {
        node.path == root_path && node.name == "根目录文件"
    }
}
