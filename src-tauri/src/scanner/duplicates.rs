use super::file_info::FileInfo;
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::hash::Hasher;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use twox_hash::XxHash3_64;

const QUICK_HASH_BYTES: usize = 64 * 1024;
const DUPLICATE_MIN_SIZE: u64 = 100 * 1024 * 1024;
const FULL_HASH_BUFFER: usize = 1024 * 1024;

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

/// Detect duplicate files >= 100MB by size + xxhash3 (quick + full).
pub fn find_duplicates_blocking(
    candidates: Vec<FileInfo>,
    on_progress: Option<DuplicateProgressEmitter>,
) -> Result<Vec<DuplicateGroup>> {
    let mut by_size: HashMap<u64, Vec<FileInfo>> = HashMap::new();
    for file in candidates {
        if file.size < DUPLICATE_MIN_SIZE {
            continue;
        }
        if file.is_symlink {
            continue;
        }
        by_size.entry(file.size).or_default().push(file);
    }

    let mut groups = Vec::new();
    let mut scanned_files: usize = 0;

    let mut size_buckets: Vec<(u64, Vec<FileInfo>)> = by_size
        .into_iter()
        .filter(|(_, files)| files.len() >= 2)
        .collect();
    size_buckets.sort_by(|a, b| b.0.cmp(&a.0));

    for (size, bucket) in size_buckets {
        let mut by_quick: HashMap<u64, Vec<FileInfo>> = HashMap::new();
        for file in bucket {
            scanned_files += 1;
            match quick_hash_file(Path::new(&file.path)) {
                Ok(hash) => {
                    by_quick.entry(hash).or_default().push(file);
                }
                Err(err) => {
                    tracing::debug!(
                        "[duplicates] quick hash failed path={} error={}",
                        file.path,
                        err
                    );
                }
            }
        }

        for (_, quick_group) in by_quick {
            if quick_group.len() < 2 {
                continue;
            }

            let mut by_full: HashMap<u64, Vec<FileInfo>> = HashMap::new();
            for file in quick_group {
                match full_hash_file(Path::new(&file.path)) {
                    Ok(hash) => {
                        by_full.entry(hash).or_default().push(file);
                    }
                    Err(err) => {
                        tracing::debug!(
                            "[duplicates] full hash failed path={} error={}",
                            file.path,
                            err
                        );
                    }
                }
            }

            for (_, mut full_group) in by_full {
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
                groups.push(group);

                if let Some(emit) = on_progress.as_ref() {
                    emit(DuplicateProgress {
                        current_size: size,
                        scanned_files,
                        found_groups: groups.len(),
                    });
                }
            }
        }
    }

    groups.sort_by(|a, b| b.wasted_bytes.cmp(&a.wasted_bytes));
    Ok(groups)
}

fn quick_hash_file(path: &Path) -> Result<u64> {
    let mut file = File::open(path).map_err(|e| anyhow!("open: {e}"))?;
    let mut buffer = vec![0u8; QUICK_HASH_BYTES];
    let mut total = 0usize;
    while total < QUICK_HASH_BYTES {
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

fn full_hash_file(path: &Path) -> Result<u64> {
    let mut file = File::open(path).map_err(|e| anyhow!("open: {e}"))?;
    let mut hasher = XxHash3_64::new();
    let mut buffer = vec![0u8; FULL_HASH_BUFFER];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hasher.write(&buffer[..n]),
            Err(err) => return Err(anyhow!("read: {err}")),
        }
    }
    Ok(hasher.finish())
}
