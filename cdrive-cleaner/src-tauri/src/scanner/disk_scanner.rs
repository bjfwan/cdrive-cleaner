use super::file_info::{DirectoryNode, ScanResult};
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

pub struct DiskScanner;

impl DiskScanner {
    pub fn new() -> Self {
        Self
    }

    pub async fn scan<P: AsRef<Path>>(&self, path: P) -> Result<ScanResult> {
        let start = Instant::now();
        let root_path = path.as_ref().to_string_lossy().to_string();
        
        let mut total_size: u64 = 0;
        let mut total_files: usize = 0;
        let mut total_dirs: usize = 0;
        let mut inaccessible_count: usize = 0;
        let mut dir_map: HashMap<PathBuf, DirectoryNode> = HashMap::new();

        for entry in WalkDir::new(path.as_ref())
            .follow_links(false)
            .into_iter()
        {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    let metadata = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => {
                            inaccessible_count += 1;
                            continue;
                        }
                    };

                    if metadata.is_file() {
                        total_files += 1;
                        let size = metadata.len();
                        total_size += size;

                        if let Some(parent) = path.parent() {
                            dir_map.entry(parent.to_path_buf())
                                .and_modify(|node| {
                                    node.size += size;
                                    node.file_count += 1;
                                });
                        }
                    } else if metadata.is_dir() {
                        total_dirs += 1;
                        let is_symlink = metadata.file_type().is_symlink();
                        let link_target = if is_symlink {
                            fs::read_link(path).ok().map(|p| p.to_string_lossy().to_string())
                        } else {
                            None
                        };

                        dir_map.entry(path.to_path_buf())
                            .or_insert_with(|| DirectoryNode {
                                path: path.to_string_lossy().to_string(),
                                name: path.file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string(),
                                size: 0,
                                file_count: 0,
                                children: vec![],
                                is_symlink,
                                link_target,
                            });
                    }
                }
                Err(_) => {
                    inaccessible_count += 1;
                }
            }
        }

        let directories = self.build_tree(&dir_map, path.as_ref());

        Ok(ScanResult {
            root_path,
            total_size,
            total_files,
            total_dirs,
            scan_duration_ms: start.elapsed().as_millis() as u64,
            directories,
            inaccessible_count,
        })
    }

    fn build_tree(&self, dir_map: &HashMap<PathBuf, DirectoryNode>, root: &Path) -> Vec<DirectoryNode> {
        let mut root_nodes = Vec::new();
        let mut nodes_by_parent: HashMap<PathBuf, Vec<DirectoryNode>> = HashMap::new();

        for (path, node) in dir_map.iter() {
            if let Some(parent) = path.parent() {
                nodes_by_parent.entry(parent.to_path_buf())
                    .or_insert_with(Vec::new)
                    .push(node.clone());
            }
        }

        if let Some(children) = nodes_by_parent.get(root) {
            for mut child in children.clone() {
                self.attach_children(&mut child, &nodes_by_parent);
                root_nodes.push(child);
            }
        }

        root_nodes
    }

    fn attach_children(&self, node: &mut DirectoryNode, nodes_by_parent: &HashMap<PathBuf, Vec<DirectoryNode>>) {
        let path = PathBuf::from(&node.path);
        if let Some(children) = nodes_by_parent.get(&path) {
            for mut child in children.clone() {
                self.attach_children(&mut child, nodes_by_parent);
                node.children.push(child);
            }
        }
    }
}
