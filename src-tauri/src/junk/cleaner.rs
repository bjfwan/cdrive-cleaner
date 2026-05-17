use anyhow::Result;
use serde::Serialize;
use std::path::Path;
use std::time::Instant;

use crate::migration::delete::{delete_path, DeleteMode};

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

pub async fn clean_junk_paths(
    paths: Vec<String>,
    to_recycle_bin: bool,
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

    for p in paths {
        if !Path::new(&p).exists() {
            continue;
        }

        match delete_path(&p, mode, None).await {
            Ok(res) => {
                if res.success {
                    cleaned_size += res.deleted_size;
                    cleaned_count += res.deleted_files;
                } else {
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
                    path: p,
                    error: e.to_string(),
                });
                failed_count += 1;
            }
        }
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
