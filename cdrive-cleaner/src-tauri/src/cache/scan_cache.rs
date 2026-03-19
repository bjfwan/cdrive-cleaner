use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedNode {
    pub path: PathBuf,
    pub size: u64,
    pub file_count: usize,
    pub dir_count: usize,
    pub modified_time: SystemTime,
    pub children: Vec<PathBuf>,
}

#[derive(Clone)]
pub struct ScanCache {
    cache: Arc<Mutex<HashMap<PathBuf, CachedNode>>>,
    /// 上次扫描的总文件数，用于下次进度估算
    last_total_files: Arc<AtomicUsize>,
}

impl ScanCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            last_total_files: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn get(&self, path: &Path) -> Option<CachedNode> {
        self.cache.lock().unwrap().get(path).cloned()
    }

    pub fn insert(&self, path: PathBuf, node: CachedNode) {
        self.cache.lock().unwrap().insert(path, node);
    }

    pub fn needs_rescan(&self, path: &Path) -> bool {
        let cached = match self.get(path) {
            Some(node) => node,
            None => return true,
        };
        if !path.exists() {
            return true;
        }
        let current_modified = match std::fs::metadata(path).and_then(|m| m.modified()) {
            Ok(time) => time,
            Err(_) => return true,
        };
        cached.modified_time != current_modified
    }

    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
    }

    pub fn size(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    pub fn remove(&self, path: &Path) {
        self.cache.lock().unwrap().remove(path);
    }

    pub fn remove_batch(&self, paths: &[PathBuf]) {
        let mut cache = self.cache.lock().unwrap();
        for path in paths {
            cache.remove(path);
        }
    }

    pub fn last_total_files(&self) -> usize {
        self.last_total_files.load(Ordering::Relaxed)
    }

    pub fn set_last_total_files(&self, count: usize) {
        self.last_total_files.store(count, Ordering::Relaxed);
    }
}

impl Default for ScanCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic() {
        let cache = ScanCache::new();
        let path = PathBuf::from("C:\\test");

        let node = CachedNode {
            path: path.clone(),
            size: 1024,
            file_count: 10,
            dir_count: 3,
            modified_time: SystemTime::now(),
            children: vec![],
        };

        cache.insert(path.clone(), node);

        let retrieved = cache.get(&path);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().size, 1024);
    }

    #[test]
    fn test_cache_clear() {
        let cache = ScanCache::new();
        let path = PathBuf::from("C:\\test");

        let node = CachedNode {
            path: path.clone(),
            size: 1024,
            file_count: 10,
            dir_count: 3,
            modified_time: SystemTime::now(),
            children: vec![],
        };

        cache.insert(path.clone(), node);
        assert_eq!(cache.size(), 1);

        cache.clear();
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn test_last_total_files() {
        let cache = ScanCache::new();
        assert_eq!(cache.last_total_files(), 0);
        cache.set_last_total_files(500000);
        assert_eq!(cache.last_total_files(), 500000);
    }
}
