use super::file_info::{DirectoryChildrenSnapshot, DirectoryNode, FileInfo, LargeFilesPage, ScanResult};
use super::path_utils::{normalized_path_key_str, normalized_path_starts_with};
use std::cmp::Ordering;
use std::collections::HashMap;

fn path_matches(a: &str, b: &str) -> bool {
    normalized_path_key_str(a) == normalized_path_key_str(b)
}

#[derive(Clone)]
struct IndexedLargeFile {
    normalized_path: String,
    file: FileInfo,
}

#[derive(Clone)]
pub struct IndexedScanResult {
    root_path: String,
    total_size: u64,
    system_reserved_bytes: u64,
    total_files: usize,
    total_dirs: usize,
    scan_duration_ms: u64,
    inaccessible_count: usize,
    scan_backend: Option<String>,
    root_children: Vec<DirectoryNode>,
    nodes: HashMap<String, DirectoryNode>,
    children_by_path: HashMap<String, Vec<DirectoryNode>>,
    large_files: Vec<FileInfo>,
    large_file_entries: Vec<IndexedLargeFile>,
    root_file_id: Option<u64>,
    usn_journal_id: Option<u64>,
    usn_next_usn: Option<i64>,
}

impl IndexedScanResult {
    pub fn from_scan_result(result: &ScanResult) -> Self {
        let mut large_files = result.large_files.clone();
        large_files.sort_by_key(|f| std::cmp::Reverse(f.size));
        let large_file_entries = large_files
            .iter()
            .map(|file| IndexedLargeFile {
                normalized_path: normalized_path_key_str(&file.path),
                file: file.clone(),
            })
            .collect();

        let mut indexed = Self {
            root_path: result.root_path.clone(),
            total_size: result.total_size,
            system_reserved_bytes: result.system_reserved_bytes,
            total_files: result.total_files,
            total_dirs: result.total_dirs,
            scan_duration_ms: result.scan_duration_ms,
            inaccessible_count: result.inaccessible_count,
            scan_backend: result.scan_backend.clone(),
            root_children: result.directories.iter().map(Self::strip_node).collect(),
            nodes: HashMap::new(),
            children_by_path: HashMap::new(),
            large_files,
            large_file_entries,
            root_file_id: result.root_file_id,
            usn_journal_id: result.usn_journal_id,
            usn_next_usn: result.usn_next_usn,
        };

        indexed.index_children(&result.root_path, &result.directories);
        indexed
    }

    pub fn snapshot_for_path(&self, path: &str) -> Option<ScanResult> {
        let path_key = normalized_path_key_str(path);
        if path_matches(path, &self.root_path) {
            return Some(ScanResult {
                root_path: self.root_path.clone(),
                total_size: self.total_size,
                system_reserved_bytes: self.system_reserved_bytes,
                total_files: self.total_files,
                total_dirs: self.total_dirs,
                scan_duration_ms: self.scan_duration_ms,
                directories: self.root_children.clone(),
                large_files: self.large_files.clone(),
                inaccessible_count: self.inaccessible_count,
                scan_backend: self.scan_backend.clone(),
                root_file_id: self.root_file_id,
                usn_journal_id: self.usn_journal_id,
                usn_next_usn: self.usn_next_usn,
                cache_schema_version: 0,
                env_fingerprint: Default::default(),
                scan_completed: false,
            });
        }

        let node = self.nodes.get(&path_key)?;
        let directories = self
            .children_by_path
            .get(&path_key)
            .cloned()
            .unwrap_or_default();

        Some(ScanResult {
            root_path: node.path.clone(),
            total_size: node.size,
            system_reserved_bytes: 0,
            total_files: node.file_count,
            total_dirs: node.dir_count.saturating_sub(1),
            scan_duration_ms: self.scan_duration_ms,
            directories,
            large_files: self.large_files_for_path_public(path),
            inaccessible_count: self.inaccessible_count,
            scan_backend: self.scan_backend.clone(),
            root_file_id: self.root_file_id,
            usn_journal_id: self.usn_journal_id,
            usn_next_usn: self.usn_next_usn,
            cache_schema_version: 0,
            env_fingerprint: Default::default(),
            scan_completed: false,
        })
    }

    pub fn large_files_for_path_public(&self, path: &str) -> Vec<FileInfo> {
        let prefix = normalized_path_key_str(path);
        self.large_file_entries
            .iter()
            .filter(|entry| normalized_path_starts_with(&entry.normalized_path, &prefix))
            .map(|entry| entry.file.clone())
            .collect()
    }

    pub fn large_files_page_for_path_public(
        &self,
        path: &str,
        min_size: Option<u64>,
        offset: usize,
        limit: usize,
        sort: Option<&str>,
    ) -> LargeFilesPage {
        let prefix = normalized_path_key_str(path);
        let min_size = min_size.unwrap_or(0);
        let sort_mode = sort.unwrap_or("size_desc");
        let mut entries: Vec<&IndexedLargeFile> = self
            .large_file_entries
            .iter()
            .filter(|entry| normalized_path_starts_with(&entry.normalized_path, &prefix))
            .collect();
        let total_size = entries
            .iter()
            .fold(0u64, |acc, entry| acc.saturating_add(entry.file.size));

        if !sort_mode.eq_ignore_ascii_case("size_desc") {
            let sort_mode = sort_mode.to_ascii_lowercase();
            entries.sort_by(|a, b| Self::compare_large_files(&a.file, &b.file, &sort_mode));
        }

        let filtered: Vec<&IndexedLargeFile> = entries
            .into_iter()
            .filter(|entry| entry.file.size >= min_size)
            .collect();
        let filtered_total_size = filtered
            .iter()
            .fold(0u64, |acc, entry| acc.saturating_add(entry.file.size));
        let total = filtered.len();
        let files = filtered
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|entry| entry.file.clone())
            .collect();

        LargeFilesPage {
            files,
            total,
            offset,
            limit,
            total_size,
            filtered_total_size,
            has_more: offset.saturating_add(limit) < total,
        }
    }

    pub fn children_snapshot_for_path(&self, path: &str) -> Option<DirectoryChildrenSnapshot> {
        let path_key = normalized_path_key_str(path);
        if path_matches(path, &self.root_path) {
            return Some(DirectoryChildrenSnapshot {
                root_path: self.root_path.clone(),
                total_size: self.total_size,
                total_files: self.total_files,
                total_dirs: self.total_dirs,
                directories: self.root_children.clone(),
            });
        }

        let node = self.nodes.get(&path_key)?;
        let directories = self
            .children_by_path
            .get(&path_key)
            .cloned()
            .unwrap_or_default();

        Some(DirectoryChildrenSnapshot {
            root_path: node.path.clone(),
            total_size: node.size,
            total_files: node.file_count,
            total_dirs: node.dir_count.saturating_sub(1),
            directories,
        })
    }

    fn index_children(&mut self, parent_path: &str, children: &[DirectoryNode]) {
        let compact_children: Vec<DirectoryNode> = children.iter().map(Self::strip_node).collect();
        self.children_by_path
            .insert(normalized_path_key_str(parent_path), compact_children);

        for child in children {
            if Self::is_root_files_node(&self.root_path, child) {
                continue;
            }

            self.nodes.insert(
                normalized_path_key_str(&child.path),
                Self::strip_node(child),
            );
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
            file_id: node.file_id,
        }
    }

    fn is_root_files_node(root_path: &str, node: &DirectoryNode) -> bool {
        path_matches(&node.path, root_path) && node.name == "根目录文件"
    }

    /// Smart-scan 用：返回根路径。
    pub fn root_path(&self) -> &str {
        &self.root_path
    }

    /// Smart-scan 用：遍历索引内所有目录节点（已剥离 children，只含元数据）。
    pub fn iter_nodes(&self) -> impl Iterator<Item = &DirectoryNode> {
        self.nodes.values()
    }

    /// Smart-scan 用：遍历所有大文件。
    pub fn large_files_iter(&self) -> impl Iterator<Item = &FileInfo> {
        self.large_files.iter()
    }

    /// 返回根目录的直接子目录列表。
    pub fn root_children(&self) -> &[DirectoryNode] {
        &self.root_children
    }

    /// 返回指定路径下的直接子目录列表。
    pub fn children_of(&self, path: &str) -> Option<&[DirectoryNode]> {
        self.children_by_path
            .get(&normalized_path_key_str(path))
            .map(|v| v.as_slice())
    }

    /// 返回扫描的总大小。
    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    fn compare_large_files(a: &FileInfo, b: &FileInfo, sort: &str) -> Ordering {
        match sort {
            "size_asc" => a.size.cmp(&b.size).then_with(|| a.path.cmp(&b.path)),
            "name_asc" => a.name.cmp(&b.name).then_with(|| b.size.cmp(&a.size)),
            "name_desc" => b.name.cmp(&a.name).then_with(|| b.size.cmp(&a.size)),
            "path_asc" => a.path.cmp(&b.path).then_with(|| b.size.cmp(&a.size)),
            "path_desc" => b.path.cmp(&a.path).then_with(|| b.size.cmp(&a.size)),
            "modified_asc" => a
                .modified_at
                .cmp(&b.modified_at)
                .then_with(|| b.size.cmp(&a.size)),
            "modified_desc" => b
                .modified_at
                .cmp(&a.modified_at)
                .then_with(|| b.size.cmp(&a.size)),
            _ => b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)),
        }
    }
}
