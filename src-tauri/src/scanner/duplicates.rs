use super::file_info::FileInfo;
use crate::session::CancellationToken;
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::hash::Hasher;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};
use twox_hash::XxHash3_64;

const QUICK_HASH_BYTES: usize = 64 * 1024;
const DUPLICATE_MIN_SIZE: u64 = 100 * 1024 * 1024;
const FULL_HASH_BUFFER: usize = 4 * 1024 * 1024;
const PARTIAL_HASH_BYTES: usize = 128 * 1024;

pub struct DuplicateScanRegistry {
    inner: Mutex<HashMap<String, CancellationToken>>,
}

impl Default for DuplicateScanRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DuplicateScanRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn normalize_key(root_path: &str) -> String {
        let mut s = root_path.trim().to_string();
        #[cfg(windows)]
        {
            s = s.replace('/', "\\").to_ascii_lowercase();
        }
        while s.ends_with('\\') && s.len() > 3 {
            s.pop();
        }
        s
    }

    pub fn begin_scan(&self, root_path: &str) -> CancellationToken {
        let key = Self::normalize_key(root_path);
        let token = CancellationToken::new();
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(existing) = guard.insert(key, token.clone()) {
                existing.cancel();
            }
        }
        token
    }

    pub fn finish_scan(&self, root_path: &str, token: &CancellationToken) {
        let key = Self::normalize_key(root_path);
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        if let Some(existing) = guard.get(&key) {
            if Arc::ptr_eq(&existing.as_atomic(), &token.as_atomic()) {
                guard.remove(&key);
            }
        }
    }

    pub fn cancel(&self, root_path: &str) {
        let key = Self::normalize_key(root_path);
        if let Ok(guard) = self.inner.lock() {
            if let Some(token) = guard.get(&key) {
                token.cancel();
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateFile {
    pub path: String,
    pub modified_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    pub size: u64,
    pub files: Vec<DuplicateFile>,
    pub wasted_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateProgress {
    pub current_size: u64,
    pub scanned_files: usize,
    pub found_groups: usize,
}

/// Callback used to forward progress to the UI without coupling this module to
/// the Tauri runtime (keeps integration tests free of Wry/WebView2 imports).
pub type DuplicateProgressEmitter = Arc<dyn Fn(DuplicateProgress) + Send + Sync>;
pub type DuplicateGroupEmitter = Arc<dyn Fn(DuplicateGroup) + Send + Sync>;

/// Detect duplicate files >= 100MB by size + xxhash3 (quick + full).
pub fn find_duplicates_blocking(
    candidates: Vec<FileInfo>,
    on_progress: Option<DuplicateProgressEmitter>,
    cancellation: Option<CancellationToken>,
) -> Result<Vec<DuplicateGroup>> {
    find_duplicates_blocking_with_events(candidates, on_progress, None, cancellation)
}

pub fn find_duplicates_blocking_with_events(
    candidates: Vec<FileInfo>,
    on_progress: Option<DuplicateProgressEmitter>,
    on_group: Option<DuplicateGroupEmitter>,
    cancellation: Option<CancellationToken>,
) -> Result<Vec<DuplicateGroup>> {
    find_duplicates_blocking_with_events_and_min_size(
        candidates,
        on_progress,
        on_group,
        cancellation,
        DUPLICATE_MIN_SIZE,
    )
}

#[cfg(test)]
fn find_duplicates_blocking_with_min_size(
    candidates: Vec<FileInfo>,
    on_progress: Option<DuplicateProgressEmitter>,
    cancellation: Option<CancellationToken>,
    min_size: u64,
) -> Result<Vec<DuplicateGroup>> {
    find_duplicates_blocking_with_events_and_min_size(
        candidates,
        on_progress,
        None,
        cancellation,
        min_size,
    )
}

fn find_duplicates_blocking_with_events_and_min_size(
    candidates: Vec<FileInfo>,
    on_progress: Option<DuplicateProgressEmitter>,
    on_group: Option<DuplicateGroupEmitter>,
    cancellation: Option<CancellationToken>,
    min_size: u64,
) -> Result<Vec<DuplicateGroup>> {
    let cancellation = cancellation.as_ref();
    let mut by_size: HashMap<u64, Vec<FileInfo>> = HashMap::new();
    for file in candidates {
        ensure_not_cancelled(cancellation)?;
        if file.size < min_size {
            continue;
        }
        if file.is_symlink {
            continue;
        }
        by_size.entry(file.size).or_default().push(file);
    }

    let mut groups = Vec::new();
    let mut scanned_files = 0usize;

    let mut size_buckets: Vec<(u64, Vec<FileInfo>)> = by_size
        .into_iter()
        .filter(|(_, files)| files.len() >= 2)
        .collect();
    size_buckets.sort_by_key(|b| std::cmp::Reverse(b.0));

    for (size, bucket) in size_buckets {
        ensure_not_cancelled(cancellation)?;
        let bucket_len = bucket.len();

        let quick_results = hash_files(&bucket, cancellation, "quick", |file, token| {
            quick_hash_file(Path::new(&file.path), token)
        })?;
        let by_quick = group_by_hash(quick_results);

        for (_, quick_group) in by_quick {
            ensure_not_cancelled(cancellation)?;
            if quick_group.len() < 2 {
                continue;
            }

            let partial_results = hash_files(&quick_group, cancellation, "partial", |file, token| {
                partial_hash_file(Path::new(&file.path), file.size, token)
            })?;
            let by_partial = group_by_hash(partial_results);

            for (_, partial_group) in by_partial {
                ensure_not_cancelled(cancellation)?;
                if partial_group.len() < 2 {
                    continue;
                }

                let full_results = hash_files(&partial_group, cancellation, "full", |file, token| {
                    full_hash_file(Path::new(&file.path), token)
                })?;
                let by_full = group_by_hash(full_results);

                for (_, mut full_group) in by_full {
                    ensure_not_cancelled(cancellation)?;
                    if full_group.len() < 2 {
                        continue;
                    }
                    full_group.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
                    let wasted = size.saturating_mul(full_group.len() as u64 - 1);
                    let group = DuplicateGroup {
                        size,
                        files: full_group
                            .into_iter()
                            .map(|f| DuplicateFile {
                                path: f.path,
                                modified_at: f.modified_at,
                            })
                            .collect(),
                        wasted_bytes: wasted,
                    };
                    if let Some(emit) = on_group.as_ref() {
                        emit(group.clone());
                    }
                    groups.push(group);
                }
            }
        }

        scanned_files += bucket_len;
        if let Some(emit) = on_progress.as_ref() {
            emit(DuplicateProgress {
                current_size: size,
                scanned_files,
                found_groups: groups.len(),
            });
        }
    }

    groups.sort_by_key(|b| std::cmp::Reverse(b.wasted_bytes));
    Ok(groups)
}

fn ensure_not_cancelled(cancellation: Option<&CancellationToken>) -> Result<()> {
    if cancellation
        .map(|token| token.is_cancelled())
        .unwrap_or(false)
    {
        return Err(anyhow!("重复文件分析已取消"));
    }
    Ok(())
}

fn hash_files<F>(
    files: &[FileInfo],
    cancellation: Option<&CancellationToken>,
    stage: &str,
    mut hash_fn: F,
) -> Result<Vec<(u64, FileInfo)>>
where
    F: FnMut(&FileInfo, Option<&CancellationToken>) -> Result<u64>,
{
    let mut results = Vec::with_capacity(files.len());
    for file in files {
        ensure_not_cancelled(cancellation)?;
        match hash_fn(file, cancellation) {
            Ok(hash) => results.push((hash, file.clone())),
            Err(err) => {
                ensure_not_cancelled(cancellation)?;
                tracing::debug!(
                    "[duplicates] {stage} hash failed path={} error={}",
                    file.path,
                    err
                );
            }
        }
    }
    Ok(results)
}

fn group_by_hash(items: Vec<(u64, FileInfo)>) -> HashMap<u64, Vec<FileInfo>> {
    let mut grouped: HashMap<u64, Vec<FileInfo>> = HashMap::new();
    for (hash, file) in items {
        grouped.entry(hash).or_default().push(file);
    }
    grouped
}

fn quick_hash_file(path: &Path, cancellation: Option<&CancellationToken>) -> Result<u64> {
    ensure_not_cancelled(cancellation)?;
    let mut file = File::open(path).map_err(|e| anyhow!("open: {e}"))?;
    let mut buffer = vec![0u8; QUICK_HASH_BYTES];
    let mut total = 0usize;
    while total < QUICK_HASH_BYTES {
        ensure_not_cancelled(cancellation)?;
        match file.read(&mut buffer[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(err) => return Err(anyhow!("read: {err}")),
        }
    }
    let mut hasher = XxHash3_64::new();
    hasher.write(&buffer[..total]);
    Ok(hasher.finish())
}

fn partial_hash_file(
    path: &Path,
    file_size: u64,
    cancellation: Option<&CancellationToken>,
) -> Result<u64> {
    ensure_not_cancelled(cancellation)?;
    let mut file = File::open(path).map_err(|e| anyhow!("open: {e}"))?;
    let chunk = PARTIAL_HASH_BYTES.min(file_size as usize);
    let chunk_u64 = chunk as u64;
    let mut positions = vec![
        0,
        file_size.saturating_div(2).saturating_sub(chunk_u64 / 2),
        file_size.saturating_sub(chunk_u64),
    ];
    positions.sort_unstable();
    positions.dedup();

    let mut hasher = XxHash3_64::new();
    hasher.write(&file_size.to_le_bytes());
    let mut buffer = vec![0u8; chunk];
    for position in positions {
        ensure_not_cancelled(cancellation)?;
        file.seek(SeekFrom::Start(position))
            .map_err(|e| anyhow!("seek: {e}"))?;
        let mut total = 0usize;
        while total < chunk {
            ensure_not_cancelled(cancellation)?;
            match file.read(&mut buffer[total..chunk]) {
                Ok(0) => break,
                Ok(n) => total += n,
                Err(err) => return Err(anyhow!("read: {err}")),
            }
        }
        hasher.write(&position.to_le_bytes());
        hasher.write(&buffer[..total]);
    }
    Ok(hasher.finish())
}

fn full_hash_file(path: &Path, cancellation: Option<&CancellationToken>) -> Result<u64> {
    ensure_not_cancelled(cancellation)?;
    let mut file = File::open(path).map_err(|e| anyhow!("open: {e}"))?;
    let mut hasher = XxHash3_64::new();
    let mut buffer = vec![0u8; FULL_HASH_BUFFER];
    loop {
        ensure_not_cancelled(cancellation)?;
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hasher.write(&buffer[..n]),
            Err(err) => return Err(anyhow!("read: {err}")),
        }
    }
    Ok(hasher.finish())
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::{Seek, SeekFrom, Write};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> Self {
            let unique = format!(
                "csd-dupes-{}-{}-{}",
                label,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn join(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn info(path: &Path) -> FileInfo {
        let size = fs::metadata(path).unwrap().len();
        FileInfo {
            path: path.to_string_lossy().to_string(),
            name: path.file_name().unwrap().to_string_lossy().to_string(),
            size,
            extension: path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_string(),
            modified_at: String::new(),
            is_readonly: false,
            is_symlink: false,
            link_target: None,
        }
    }

    fn write_pattern(path: &Path, len: usize, seed: u8) {
        let mut data = vec![0u8; len];
        for (idx, byte) in data.iter_mut().enumerate() {
            *byte = seed.wrapping_add((idx % 251) as u8);
        }
        fs::write(path, data).unwrap();
    }

    fn write_sparse(path: &Path, len: u64, writes: &[(u64, &[u8])]) {
        let mut file = File::create(path).unwrap();
        file.set_len(len).unwrap();
        for (offset, bytes) in writes {
            file.seek(SeekFrom::Start(*offset)).unwrap();
            file.write_all(bytes).unwrap();
        }
    }

    #[test]
    fn identical_large_files_are_grouped() {
        let dir = TestDir::new("same");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        write_pattern(&a, 384 * 1024, 17);
        fs::copy(&a, &b).unwrap();

        let groups = find_duplicates_blocking_with_min_size(vec![info(&a), info(&b)], None, None, 1).unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
        assert_eq!(groups[0].wasted_bytes, groups[0].size);
    }

    #[test]
    fn group_emitter_receives_confirmed_groups() {
        let dir = TestDir::new("emit");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        write_pattern(&a, 384 * 1024, 19);
        fs::copy(&a, &b).unwrap();
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let emitted_sink = emitted.clone();
        let on_group: DuplicateGroupEmitter = Arc::new(move |group| {
            emitted_sink.lock().unwrap().push(group);
        });

        let groups = find_duplicates_blocking_with_events_and_min_size(
            vec![info(&a), info(&b)],
            None,
            Some(on_group),
            None,
            1,
        )
        .unwrap();

        let emitted = emitted.lock().unwrap();
        assert_eq!(emitted.len(), 1);
        assert_eq!(groups.len(), emitted.len());
        assert_eq!(groups[0].wasted_bytes, emitted[0].wasted_bytes);
        assert_eq!(emitted[0].files.len(), 2);
    }

    #[test]
    fn same_size_different_content_is_not_grouped() {
        let dir = TestDir::new("different");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        write_pattern(&a, 384 * 1024, 1);
        write_pattern(&b, 384 * 1024, 2);

        let groups = find_duplicates_blocking_with_min_size(vec![info(&a), info(&b)], None, None, 1).unwrap();

        assert!(groups.is_empty());
    }

    #[test]
    fn partial_hash_separates_same_head_with_middle_or_tail_changes() {
        let dir = TestDir::new("partial");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        let len = 512 * 1024;
        write_sparse(&a, len, &[(0, b"same-head"), (260 * 1024, b"middle-a"), (500 * 1024, b"tail-a")]);
        write_sparse(&b, len, &[(0, b"same-head"), (260 * 1024, b"middle-b"), (500 * 1024, b"tail-b")]);

        let quick_a = quick_hash_file(&a, None).unwrap();
        let quick_b = quick_hash_file(&b, None).unwrap();
        let partial_a = partial_hash_file(&a, len, None).unwrap();
        let partial_b = partial_hash_file(&b, len, None).unwrap();
        let groups = find_duplicates_blocking_with_min_size(vec![info(&a), info(&b)], None, None, 1).unwrap();

        assert_eq!(quick_a, quick_b);
        assert_ne!(partial_a, partial_b);
        assert!(groups.is_empty());
    }

    #[test]
    fn full_hash_confirms_partial_hash_collisions() {
        let dir = TestDir::new("full");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        let len = 512 * 1024;
        write_sparse(&a, len, &[(0, b"same-head"), (150 * 1024, b"gap-a")]);
        write_sparse(&b, len, &[(0, b"same-head"), (150 * 1024, b"gap-b")]);

        let quick_a = quick_hash_file(&a, None).unwrap();
        let quick_b = quick_hash_file(&b, None).unwrap();
        let partial_a = partial_hash_file(&a, len, None).unwrap();
        let partial_b = partial_hash_file(&b, len, None).unwrap();
        let groups = find_duplicates_blocking_with_min_size(vec![info(&a), info(&b)], None, None, 1).unwrap();

        assert_eq!(quick_a, quick_b);
        assert_eq!(partial_a, partial_b);
        assert!(groups.is_empty());
    }

    #[test]
    fn cancelled_token_returns_error() {
        let dir = TestDir::new("cancel");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        write_pattern(&a, 128 * 1024, 1);
        fs::copy(&a, &b).unwrap();
        let token = CancellationToken::new();
        token.cancel();

        let err = find_duplicates_blocking_with_min_size(vec![info(&a), info(&b)], None, Some(token), 1).unwrap_err();

        assert!(err.to_string().contains("\u{5df2}\u{53d6}\u{6d88}"));
    }

    #[test]
    fn hash_stage_cancellation_is_not_swallowed() {
        let file = FileInfo {
            path: "x".to_string(),
            name: "x".to_string(),
            size: 1,
            extension: String::new(),
            modified_at: String::new(),
            is_readonly: false,
            is_symlink: false,
            link_target: None,
        };
        let token = CancellationToken::new();

        let err = hash_files(&[file], Some(&token), "test", |_file, token| {
            token.unwrap().cancel();
            Err(anyhow!("interrupted"))
        })
        .unwrap_err();

        assert!(err.to_string().contains("\u{5df2}\u{53d6}\u{6d88}"));
    }

    #[cfg(windows)]
    #[test]
    fn registry_finish_only_removes_matching_token() {
        let registry = DuplicateScanRegistry::new();
        let first = registry.begin_scan(r"C:/Foo/");
        let second = registry.begin_scan(r"c:\foo");

        assert!(first.is_cancelled());
        registry.finish_scan(r"C:\foo\", &first);
        assert_eq!(registry.inner.lock().unwrap().len(), 1);
        registry.finish_scan(r"C:\foo\", &second);
        assert!(registry.inner.lock().unwrap().is_empty());
    }
}
