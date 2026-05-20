use anyhow::{anyhow, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteMode {
    Recycle,
    Permanent,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeleteError {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeleteResult {
    pub success: bool,
    pub source_path: String,
    pub mode: DeleteMode,
    pub deleted_size: u64,
    pub deleted_files: usize,
    pub errors: Vec<DeleteError>,
    pub duration_ms: u64,
}

#[derive(Clone, Serialize)]
pub struct DeleteProgress {
    pub current_file: String,
    pub deleted_files: usize,
    pub total_files: usize,
    pub deleted_size: u64,
    pub total_size: u64,
    pub progress_percent: f64,
    pub error_count: usize,
}

pub type DeleteProgressCallback = Arc<dyn Fn(DeleteProgress) + Send + Sync>;

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Default)]
struct DeleteTracker {
    deleted_size: AtomicU64,
    deleted_files: AtomicUsize,
    error_count: AtomicUsize,
    should_stop: AtomicBool,
    current_file: Mutex<String>,
}

impl DeleteTracker {
    fn snapshot(&self) -> (u64, usize, usize, String) {
        let current = self
            .current_file
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default();
        (
            self.deleted_size.load(Ordering::Relaxed),
            self.deleted_files.load(Ordering::Relaxed),
            self.error_count.load(Ordering::Relaxed),
            current,
        )
    }

    fn set_current(&self, path: &Path) {
        if let Ok(mut current) = self.current_file.lock() {
            *current = path.to_string_lossy().to_string();
        }
    }
}

pub async fn delete_path<P: AsRef<Path>>(
    path: P,
    mode: DeleteMode,
    on_progress: Option<DeleteProgressCallback>,
) -> Result<DeleteResult> {
    let path = path.as_ref().to_path_buf();
    tokio::task::spawn_blocking(move || delete_path_blocking(path, mode, on_progress))
        .await
        .map_err(|e| anyhow!("delete task join failed: {e}"))?
}

fn delete_path_blocking(
    path: PathBuf,
    mode: DeleteMode,
    on_progress: Option<DeleteProgressCallback>,
) -> Result<DeleteResult> {
    let start = Instant::now();
    let source_path = path.to_string_lossy().to_string();

    if !path.exists() {
        return Ok(DeleteResult {
            success: false,
            source_path,
            mode,
            deleted_size: 0,
            deleted_files: 0,
            errors: vec![DeleteError {
                path: path.to_string_lossy().to_string(),
                error: "路径不存在".to_string(),
            }],
            duration_ms: start.elapsed().as_millis() as u64,
        });
    }

    let (total_size, total_files) = stat_total(&path);
    let tracker = Arc::new(DeleteTracker::default());
    let mut errors: Vec<DeleteError> = Vec::new();

    let reporter = start_progress_reporter(&on_progress, &tracker, total_size, total_files);

    match mode {
        DeleteMode::Recycle => {
            delete_to_recycle(&path, &tracker, &mut errors);
        }
        DeleteMode::Permanent => {
            delete_permanently(&path, &tracker, &mut errors);
        }
    }

    tracker.should_stop.store(true, Ordering::Relaxed);
    if let Some(handle) = reporter {
        let _ = handle.join();
    }

    let (deleted_size, deleted_files, _, _) = tracker.snapshot();

    if let Some(callback) = on_progress.as_ref() {
        let progress = DeleteProgress {
            current_file: path.to_string_lossy().to_string(),
            deleted_files,
            total_files,
            deleted_size,
            total_size,
            progress_percent: 100.0,
            error_count: errors.len(),
        };
        callback(progress);
    }

    let success = errors.is_empty();
    Ok(DeleteResult {
        success,
        source_path,
        mode,
        deleted_size,
        deleted_files,
        errors,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

fn delete_to_recycle(path: &Path, tracker: &DeleteTracker, errors: &mut Vec<DeleteError>) {
    tracker.set_current(path);
    let (size, count) = stat_total(path);

    match trash::delete(path) {
        Ok(_) => {
            tracker
                .deleted_size
                .fetch_add(size, Ordering::Relaxed);
            tracker
                .deleted_files
                .fetch_add(count.max(1), Ordering::Relaxed);
            tracing::info!(
                "[delete] moved to recycle bin path={} size={} files={}",
                path.display(),
                size,
                count
            );
        }
        Err(err) => {
            tracker.error_count.fetch_add(1, Ordering::Relaxed);
            errors.push(DeleteError {
                path: path.to_string_lossy().to_string(),
                error: format!("移入回收站失败: {}", err),
            });
            tracing::warn!(
                "[delete] recycle failed path={} error={}",
                path.display(),
                err
            );
        }
    }
}

fn delete_permanently(path: &Path, tracker: &DeleteTracker, errors: &mut Vec<DeleteError>) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(err) => {
            tracker.error_count.fetch_add(1, Ordering::Relaxed);
            errors.push(DeleteError {
                path: path.to_string_lossy().to_string(),
                error: format!("读取元数据失败: {}", err),
            });
            return;
        }
    };

    if metadata.is_file() || metadata.file_type().is_symlink() {
        delete_single_file(path, tracker, errors);
        return;
    }

    if metadata.is_dir() {
        delete_directory_recursive(path, tracker, errors);
    }
}

fn delete_single_file(path: &Path, tracker: &DeleteTracker, errors: &mut Vec<DeleteError>) {
    tracker.set_current(path);
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let _ = clear_readonly(path);
    match fs::remove_file(path) {
        Ok(_) => {
            tracker.deleted_size.fetch_add(size, Ordering::Relaxed);
            tracker.deleted_files.fetch_add(1, Ordering::Relaxed);
        }
        Err(err) => {
            tracker.error_count.fetch_add(1, Ordering::Relaxed);
            errors.push(DeleteError {
                path: path.to_string_lossy().to_string(),
                error: err.to_string(),
            });
        }
    }
}

fn delete_directory_recursive(
    path: &Path,
    tracker: &DeleteTracker,
    errors: &mut Vec<DeleteError>,
) {
    let read_dir = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(err) => {
            tracker.error_count.fetch_add(1, Ordering::Relaxed);
            errors.push(DeleteError {
                path: path.to_string_lossy().to_string(),
                error: format!("读取目录失败: {}", err),
            });
            return;
        }
    };

    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                tracker.error_count.fetch_add(1, Ordering::Relaxed);
                errors.push(DeleteError {
                    path: path.to_string_lossy().to_string(),
                    error: format!("枚举项失败: {}", err),
                });
                continue;
            }
        };

        let child = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(err) => {
                tracker.error_count.fetch_add(1, Ordering::Relaxed);
                errors.push(DeleteError {
                    path: child.to_string_lossy().to_string(),
                    error: err.to_string(),
                });
                continue;
            }
        };

        if file_type.is_dir() && !file_type.is_symlink() {
            delete_directory_recursive(&child, tracker, errors);
        } else {
            delete_single_file(&child, tracker, errors);
        }
    }

    tracker.set_current(path);
    let _ = clear_readonly(path);
    if let Err(err) = fs::remove_dir(path) {
        tracker.error_count.fetch_add(1, Ordering::Relaxed);
        errors.push(DeleteError {
            path: path.to_string_lossy().to_string(),
            error: format!("移除目录失败: {}", err),
        });
    }
}

fn stat_total(path: &Path) -> (u64, usize) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return (0, 0),
    };
    if metadata.is_file() || metadata.file_type().is_symlink() {
        return (metadata.len(), 1);
    }

    let mut total_size = 0u64;
    let mut total_files = 0usize;
    for entry in jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .follow_links(false)
    {
        if let Ok(entry) = entry {
            if let Ok(m) = entry.metadata() {
                if m.is_file() {
                    total_size = total_size.saturating_add(m.len());
                    total_files += 1;
                }
            }
        }
    }
    (total_size, total_files.max(1))
}

fn clear_readonly(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let mut perms = metadata.permissions();
    if perms.readonly() {
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn start_progress_reporter(
    callback: &Option<DeleteProgressCallback>,
    tracker: &Arc<DeleteTracker>,
    total_size: u64,
    total_files: usize,
) -> Option<std::thread::JoinHandle<()>> {
    let callback = callback.clone()?;
    let tracker = Arc::clone(tracker);

    let handle = std::thread::spawn(move || loop {
        std::thread::sleep(PROGRESS_EMIT_INTERVAL);
        let (deleted_size, deleted_files, error_count, current_file) = tracker.snapshot();
        let progress_percent = if total_files == 0 {
            0.0
        } else {
            (deleted_files as f64 / total_files as f64 * 100.0).min(99.9)
        };
        callback(DeleteProgress {
            current_file,
            deleted_files,
            total_files,
            deleted_size,
            total_size,
            progress_percent,
            error_count,
        });

        if tracker.should_stop.load(Ordering::Relaxed) {
            break;
        }

        if total_files > 0 && deleted_files >= total_files {
            tracker.should_stop.store(true, Ordering::Relaxed);
        }
    });

    Some(handle)
}
