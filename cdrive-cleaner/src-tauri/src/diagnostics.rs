use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::scanner::{file_info::DirectoryNode, mft_usn};
use crate::winfs;

#[derive(Debug, Clone, Serialize)]
pub struct MftValidationReport {
    pub target_path: String,
    pub validation_root: String,
    pub is_elevated: bool,
    pub file_system: String,
    pub passed: bool,
    pub scan_backend: Option<String>,
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_size: u64,
    pub usn_detected: bool,
    pub duration_ms: u64,
    pub notes: Vec<String>,
}

struct ValidationWorkspace {
    root: PathBuf,
}

impl ValidationWorkspace {
    fn new(base_path: &Path) -> Result<Self> {
        let unique = format!(
            "cdrive-cleaner-mft-validation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        let root = base_path.join(unique);
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
}

impl Drop for ValidationWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn run_mft_end_to_end_validation(target_path: &Path) -> Result<MftValidationReport> {
    if !target_path.exists() || !target_path.is_dir() {
        return Err(anyhow!("validation target must be an existing directory"));
    }

    let start = Instant::now();
    let is_elevated = crate::commands::is_elevated();
    let volume = winfs::query_volume_details(target_path)
        .ok_or_else(|| anyhow!("failed to resolve volume information for {}", target_path.display()))?;
    let mut notes = Vec::new();

    if !is_elevated {
        notes.push("当前不是管理员会话，MFT 验证需要先提权".to_string());
        return Ok(MftValidationReport {
            target_path: target_path.to_string_lossy().to_string(),
            validation_root: String::new(),
            is_elevated,
            file_system: volume.file_system,
            passed: false,
            scan_backend: None,
            total_files: 0,
            total_dirs: 0,
            total_size: 0,
            usn_detected: false,
            duration_ms: start.elapsed().as_millis() as u64,
            notes,
        });
    }

    if !volume.file_system.eq_ignore_ascii_case("NTFS") {
        notes.push("目标卷不是 NTFS，无法验证 MFT + USN 路径".to_string());
        return Ok(MftValidationReport {
            target_path: target_path.to_string_lossy().to_string(),
            validation_root: String::new(),
            is_elevated,
            file_system: volume.file_system,
            passed: false,
            scan_backend: None,
            total_files: 0,
            total_dirs: 0,
            total_size: 0,
            usn_detected: false,
            duration_ms: start.elapsed().as_millis() as u64,
            notes,
        });
    }

    if !winfs::supports_mft_scan(target_path) {
        notes.push("当前管理员会话仍无法打开卷句柄，MFT 访问不可用".to_string());
        return Ok(MftValidationReport {
            target_path: target_path.to_string_lossy().to_string(),
            validation_root: String::new(),
            is_elevated,
            file_system: volume.file_system,
            passed: false,
            scan_backend: None,
            total_files: 0,
            total_dirs: 0,
            total_size: 0,
            usn_detected: false,
            duration_ms: start.elapsed().as_millis() as u64,
            notes,
        });
    }

    let workspace = ValidationWorkspace::new(target_path)?;
    let nested_dir = workspace.root.join("alpha").join("beta");
    let empty_dir = workspace.root.join("empty");
    fs::create_dir_all(&nested_dir)?;
    fs::create_dir_all(&empty_dir)?;
    write_file(&workspace.root.join("root.log"), b"root-bytes")?;
    write_file(&workspace.root.join("alpha").join("a.bin"), &[7u8; 128])?;
    write_file(&nested_dir.join("b.txt"), b"payload")?;

    let scan_result = mft_usn::scan_path(
        &workspace.root,
        None,
        64,
        Arc::new(AtomicBool::new(false)),
    )?
    .ok_or_else(|| anyhow!("MFT backend was not selected for {}", workspace.root.display()))?;

    let expected_size = "root-bytes".len() as u64 + 128 + "payload".len() as u64;
    if scan_result.total_files != 3 || scan_result.total_dirs != 3 || scan_result.total_size != expected_size {
        return Err(anyhow!(
            "unexpected scan stats: files={}, dirs={}, size={}",
            scan_result.total_files,
            scan_result.total_dirs,
            scan_result.total_size
        ));
    }

    let (Some(root_file_id), Some(journal_id), Some(next_usn)) =
        (scan_result.root_file_id, scan_result.usn_journal_id, scan_result.usn_next_usn)
    else {
        return Err(anyhow!("scan result did not include a usable USN checkpoint"));
    };

    std::thread::sleep(Duration::from_millis(50));
    write_file(&nested_dir.join("b.txt"), b"payload-updated")?;

    let mut frn_to_path = HashMap::new();
    frn_to_path.insert(root_file_id, workspace.root.to_string_lossy().to_string());
    build_file_id_map(&scan_result.directories, &mut frn_to_path);

    let checkpoint = winfs::UsnJournalCheckpoint { journal_id, next_usn };
    let nested_dir_str = nested_dir.to_string_lossy().to_string();
    let mut usn_detected = false;

    for _ in 0..10 {
        let changes = winfs::collect_usn_changed_dirs(
            &workspace.root,
            checkpoint,
            Some(root_file_id),
            &frn_to_path,
        )?
        .context("USN journal is unavailable for the validation workspace")?;

        if changes.changed_dirs.contains(&nested_dir_str) {
            usn_detected = true;
            break;
        }

        std::thread::sleep(Duration::from_millis(200));
    }

    if !usn_detected {
        return Err(anyhow!("USN journal did not report the modified validation directory"));
    }

    notes.push("MFT 枚举成功，统计结果与验证目录一致".to_string());
    notes.push("USN 变化集成功捕获到被修改的子目录".to_string());
    notes.push("验证临时目录已在退出时自动清理".to_string());

    Ok(MftValidationReport {
        target_path: target_path.to_string_lossy().to_string(),
        validation_root: workspace.root.to_string_lossy().to_string(),
        is_elevated,
        file_system: volume.file_system,
        passed: true,
        scan_backend: scan_result.scan_backend,
        total_files: scan_result.total_files,
        total_dirs: scan_result.total_dirs,
        total_size: scan_result.total_size,
        usn_detected,
        duration_ms: start.elapsed().as_millis() as u64,
        notes,
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    Ok(())
}

fn build_file_id_map(nodes: &[DirectoryNode], map: &mut HashMap<u64, String>) {
    for node in nodes {
        if let Some(file_id) = node.file_id {
            map.insert(file_id, node.path.clone());
        }
        build_file_id_map(&node.children, map);
    }
}
