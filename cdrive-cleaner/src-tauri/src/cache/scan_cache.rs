use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

/// 缓存的目录节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedNode {
    pub path: PathBuf,
    pub size: u64,
    pub file_count: usize,
    pub modified_time: SystemTime,
    pub children: Vec<PathBuf>,
}

/// 扫描缓存管理器
pub struct ScanCache {
    cache: Arc<Mutex<HashMap<PathBuf, CachedNode>>>,
}

impl ScanCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 获取缓存的节点
    pub fn get(&self, path: &Path) -> Option<CachedNode> {
        self.cache.lock().unwrap().get(path).cloned()
    }

    /// 保存节点到缓存
    pub fn insert(&self, path: PathBuf, node: CachedNode) {
        self.cache.lock().unwrap().insert(path, node);
    }

    /// 检查路径是否需要重新扫描
    pub fn needs_rescan(&self, path: &Path) -> bool {
        // 检查缓存中是否存在
        let cached = match self.get(path) {
            Some(node) => node,
            None => return true, // 没有缓存，需要扫描
        };

        // 检查文件是否存在
        if !path.exists() {
            return true;
        }

        // 获取当前修改时间
        let current_modified = match std::fs::metadata(path)
            .and_then(|m| m.modified())
        {
            Ok(time) => time,
            Err(_) => return true, // 无法获取修改时间，需要重新扫描
        };

        // 对比修改时间
        cached.modified_time != current_modified
    }

    /// 清除缓存
    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
    }

    /// 获取缓存大小
    pub fn size(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// 移除指定路径的缓存
    pub fn remove(&self, path: &Path) {
        self.cache.lock().unwrap().remove(path);
    }

    /// 批量移除缓存（用于删除的文件）
    pub fn remove_batch(&self, paths: &[PathBuf]) {
        let mut cache = self.cache.lock().unwrap();
        for path in paths {
            cache.remove(path);
        }
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
    use std::fs;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_cache_basic() {
        let cache = ScanCache::new();
        let path = PathBuf::from("C:\\test");
        
        let node = CachedNode {
            path: path.clone(),
            size: 1024,
            file_count: 10,
            modified_time: SystemTime::now(),
            children: vec![],
        };

        cache.insert(path.clone(), node.clone());
        
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
            modified_time: SystemTime::now(),
            children: vec![],
        };

        cache.insert(path.clone(), node);
        assert_eq!(cache.size(), 1);
        
        cache.clear();
        assert_eq!(cache.size(), 0);
    }
}
