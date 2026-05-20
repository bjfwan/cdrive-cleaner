use anyhow::Result;
use serde::Serialize;
use std::path::Path;
use std::time::Instant;

use crate::migration::delete::{delete_path, DeleteMode, DeleteProgressCallback};

#[derive(Debug, Clone, Serialize)]
pub struct JunkCleanError {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JunkCleanResult {
    pub success: bool,
    pub cleaned_size: u64,
    pub cleaned_count: usize,
    pub failed_count: usize,
    pub errors: Vec<JunkCleanError>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct JunkCleanProgress {
    pub current_path: String,
    pub completed_items: usize,
    pub total_items: usize,
    pub cleaned_size: u64,
    pub total_size: u64,
    pub current_deleted_files: usize,
    pub current_total_files: usize,
    pub current_file: String,
    pub progress_percent: f64,
    pub error_count: usize,
}

pub type JunkCleanProgressCallback = std::sync::Arc<dyn Fn(JunkCleanProgress) + Send + Sync>;

pub async fn clean_junk_paths(
    paths: Vec<String>,
    to_recycle_bin: bool,
    on_progress: Option<JunkCleanProgressCallback>,
) -> Result<JunkCleanResult> {
    let started = Instant::now();
    let mode = if to_recycle_bin {
        DeleteMode::Recycle
    } else {
        DeleteMode::Permanent
    };

    let mut cleaned_size: u64 = 0;
    let mut cleaned_count: usize = 0;
    let mut failed_count: usize = 0;
    let mut errors: Vec<JunkCleanError> = Vec::new();
    let total_items = paths.len();
    let total_size: u64 = paths.iter().map(|p| path_size(Path::new(p))).sum();
    let mut completed_items: usize = 0;

    emit_clean_progress(
        on_progress.as_ref(),
        JunkCleanProgress {
            current_path: String::new(),
            completed_items,
            total_items,
            cleaned_size,
            total_size,
            current_deleted_files: 0,
            current_total_files: 0,
            current_file: String::new(),
            progress_percent: 0.0,
            error_count: 0,
        },
    );

    for p in paths {
        let current_path = p.clone();
        if !Path::new(&p).exists() {
            completed_items += 1;
            emit_clean_progress(
                on_progress.as_ref(),
                JunkCleanProgress {
                    current_path,
                    completed_items,
                    total_items,
                    cleaned_size,
                    total_size,
                    current_deleted_files: 0,
                    current_total_files: 0,
                    current_file: String::new(),
                    progress_percent: overall_percent(cleaned_size, total_size, completed_items, total_items),
                    error_count: errors.len(),
                },
            );
            continue;
        }

        let base_cleaned_size = cleaned_size;
        let base_completed_items = completed_items;
        let base_error_count = errors.len();
        let progress_callback: Option<DeleteProgressCallback> = on_progress.as_ref().map(|cb| {
            let cb = cb.clone();
            let current_path = p.clone();
            std::sync::Arc::new(move |progress: crate::migration::delete::DeleteProgress| {
                cb(JunkCleanProgress {
                    current_path: current_path.clone(),
                    completed_items: base_completed_items,
                    total_items,
                    cleaned_size: base_cleaned_size.saturating_add(progress.deleted_size),
                    total_size,
                    current_deleted_files: progress.deleted_files,
                    current_total_files: progress.total_files,
                    current_file: progress.current_file,
                    progress_percent: overall_percent(
                        base_cleaned_size.saturating_add(progress.deleted_size),
                        total_size,
                        base_completed_items,
                        total_items,
                    ),
                    error_count: base_error_count.saturating_add(progress.error_count),
                });
            }) as DeleteProgressCallback
        });

        match delete_path(&p, mode, progress_callback).await {
            Ok(res) => {
                cleaned_size += res.deleted_size;
                cleaned_count += res.deleted_files;
                if !res.success {
                    for e in res.errors {
                        errors.push(JunkCleanError {
                            path: e.path,
                            error: e.error,
                        });
                    }
                    failed_count += 1;
                }
            }
            Err(e) => {
                errors.push(JunkCleanError {
                    path: current_path.clone(),
                    error: e.to_string(),
                });
                failed_count += 1;
            }
        }
        completed_items += 1;
        emit_clean_progress(
            on_progress.as_ref(),
            JunkCleanProgress {
                current_path,
                completed_items,
                total_items,
                cleaned_size,
                total_size,
                current_deleted_files: 0,
                current_total_files: 0,
                current_file: String::new(),
                progress_percent: overall_percent(cleaned_size, total_size, completed_items, total_items),
                error_count: errors.len(),
            },
        );
    }

    Ok(JunkCleanResult {
        success: errors.is_empty(),
        cleaned_size,
        cleaned_count,
        failed_count,
        errors,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn emit_clean_progress(callback: Option<&JunkCleanProgressCallback>, progress: JunkCleanProgress) {
    if let Some(callback) = callback {
        callback(progress);
    }
}

fn overall_percent(cleaned_size: u64, total_size: u64, completed_items: usize, total_items: usize) -> f64 {
    if total_size > 0 {
        (cleaned_size as f64 / total_size as f64 * 100.0).clamp(0.0, 100.0)
    } else if total_items > 0 {
        (completed_items as f64 / total_items as f64 * 100.0).clamp(0.0, 100.0)
    } else {
        100.0
    }
}

fn path_size(path: &Path) -> u64 {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    if metadata.is_file() || metadata.file_type().is_symlink() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }

    jwalk::WalkDir::new(path)
        .skip_hidden(false)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .fold(0u64, |acc, len| acc.saturating_add(len))
}
