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
    pub reason: String,
    pub suggestion: String,
}

impl DeleteError {
    pub fn new(path: impl Into<String>, error: impl Into<String>) -> Self {
        let error = error.into();
        let (reason, suggestion) = classify_delete_error(&error);
        Self {
            path: path.into(),
            error,
            reason,
            suggestion,
        }
    }
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
            errors: vec![DeleteError::new(path.to_string_lossy().to_string(), "路径不存在")],
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
            errors.push(DeleteError::new(
                path.to_string_lossy().to_string(),
                format!("移入回收站失败: {}", err),
            ));

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
            errors.push(DeleteError::new(
                path.to_string_lossy().to_string(),
                format!("读取元数据失败: {}", err),
            ));

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
            errors.push(DeleteError::new(
                path.to_string_lossy().to_string(),
                err.to_string(),
            ));
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
            errors.push(DeleteError::new(
                path.to_string_lossy().to_string(),
                format!("读取目录失败: {}", err),
            ));

            return;
        }
    };

    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                tracker.error_count.fetch_add(1, Ordering::Relaxed);
                errors.push(DeleteError::new(
                    path.to_string_lossy().to_string(),
                    format!("枚举项失败: {}", err),
                ));

                continue;
            }
        };

        let child = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(err) => {
                tracker.error_count.fetch_add(1, Ordering::Relaxed);
                errors.push(DeleteError::new(
                    child.to_string_lossy().to_string(),
                    err.to_string(),
                ));

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
        errors.push(DeleteError::new(
            path.to_string_lossy().to_string(),
            format!("移除目录失败: {}", err),
        ));
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

fn classify_delete_error(error: &str) -> (String, String) {
    let lower = error.to_ascii_lowercase();
    if lower.contains("some operations were aborted")
        || lower.contains("operation was aborted")
        || lower.contains("aborted")
    {
        return (
            "Windows 回收站操作被中断".to_string(),
            "常见原因是文件正在被浏览器、系统服务或安全软件占用；关闭相关程序后重试，或改用永久删除。".to_string(),
        );
    }
    if lower.contains("being used")
        || lower.contains("in use")
        || lower.contains("sharing violation")
        || error.contains("正在使用")
        || error.contains("另一个程序")
        || error.contains("占用")
    {
        return (
            "文件正在被其他程序占用".to_string(),
            "关闭 Chrome、Edge、VS Code、Windsurf 或相关后台程序后重新扫描并清理。".to_string(),
        );
    }
    if lower.contains("access is denied")
        || lower.contains("permission denied")
        || lower.contains("unauthorized")
        || lower.contains("os error 5")
        || error.contains("拒绝访问")
        || error.contains("权限")
    {
        return (
            "权限不足或系统保护".to_string(),
            "请确认已用管理员身份运行；系统保护目录和杀毒软件占用的文件可能仍会被 Windows 拦截。".to_string(),
        );
    }
    if lower.contains("directory not empty") || error.contains("目录不是空的") {
        return (
            "目录内仍有未删除内容".to_string(),
            "通常是目录中部分文件被占用，先处理失败文件后再重试删除目录。".to_string(),
        );
    }
    if lower.contains("not found")
        || lower.contains("cannot find")
        || error.contains("找不到")
        || error.contains("路径不存在")
    {
        return (
            "文件已不存在".to_string(),
            "可能已被系统、浏览器或上一轮清理移除，重新扫描后列表会刷新。".to_string(),
        );
    }
    if lower.contains("too long") || error.contains("路径太长") || error.contains("文件名太长") {
        return (
            "路径过长".to_string(),
            "可尝试永久删除，或在资源管理器中缩短上级目录名称后再处理。".to_string(),
        );
    }
    (
        "Windows 未返回明确原因".to_string(),
        "建议关闭相关应用后重试；如果仍失败，可查看详情中的系统错误文本。".to_string(),
    )
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
