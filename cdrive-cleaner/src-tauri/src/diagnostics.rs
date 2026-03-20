use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::database::ScanCacheDb;
use crate::scanner::{DiskScanner, file_info::{DirectoryNode, ScanResult}, incremental, mft_usn};
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

#[derive(Debug, Clone, Serialize)]
pub struct ScanBenchmarkRun {
    pub strategy: String,
    pub cache_present_before: bool,
    pub incremental_candidate_before: bool,
    pub wall_duration_ms: u64,
    pub reported_scan_duration_ms: u64,
    pub scan_backend: Option<String>,
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_size: u64,
    pub inaccessible_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeepScanBenchmarkReport {
    pub target_path: String,
    pub is_elevated: bool,
    pub file_system: String,
    pub mft_available: bool,
    pub deep_first: ScanBenchmarkRun,
    pub deep_second: ScanBenchmarkRun,
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

struct BenchmarkWorkspace {
    root: PathBuf,
}

impl BenchmarkWorkspace {
    fn new() -> Result<Self> {
        let unique = format!(
            "cdrive-cleaner-scan-benchmark-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
}

impl Drop for BenchmarkWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn run_deep_scan_benchmark(target_path: &Path, estimated_files: usize) -> Result<DeepScanBenchmarkReport> {
    if !target_path.exists() || !target_path.is_dir() {
        return Err(anyhow!("benchmark target must be an existing directory"));
    }

    let target_key = target_path.to_string_lossy().to_string();
    let is_elevated = crate::commands::is_elevated();
    let volume = winfs::query_volume_details(target_path)
        .ok_or_else(|| anyhow!("failed to resolve volume information for {}", target_path.display()))?;
    let mft_available = winfs::supports_mft_scan(target_path);
    let workspace = BenchmarkWorkspace::new()?;
    let deep_cache = ScanCacheDb::new(workspace.root.join("deep_scan_cache.db").to_string_lossy().as_ref())?;
    let deep_scanner = DiskScanner::new();

    let runtime = tokio::runtime::Runtime::new().context("failed to create tokio runtime for deep benchmark")?;
    let (deep_first, deep_second) = runtime.block_on(async {
        let deep_first = run_deep_scan_pass(&deep_scanner, &deep_cache, target_path, estimated_files).await?;
        let deep_second = run_deep_scan_pass(
            &deep_scanner,
            &deep_cache,
            target_path,
            deep_first.total_files.max(1),
        )
        .await?;

        Ok::<_, anyhow::Error>((deep_first, deep_second))
    })?;

    let mut notes = vec![
        "wall_duration_ms 是本次 benchmark 用于判断快慢的主时间，代表命令级端到端耗时。".to_string(),
        "reported_scan_duration_ms 来自 ScanResult.scan_duration_ms；如果增量扫描命中“无变化直接返回缓存”，这个值可能沿用上一次扫描结果。".to_string(),
        "deep 的二扫会优先尝试 USN 增量合并；如果变化过多会回退到全量深度扫描。".to_string(),
    ];

    if !is_elevated {
        notes.push("当前不是管理员会话，深度扫描大概率不会走 MFT + USN 快路径。".to_string());
    } else if !mft_available {
        notes.push(format!(
            "当前卷文件系统是 {}，但 MFT + USN 仍不可用，深度扫描会回退到原生枚举。",
            volume.file_system
        ));
    } else {
        notes.push("当前卷支持 MFT + USN，深度扫描首扫预计会优先尝试 mft_usn。".to_string());
    }

    Ok(DeepScanBenchmarkReport {
        target_path: target_key,
        is_elevated,
        file_system: volume.file_system,
        mft_available,
        deep_first,
        deep_second,
        notes,
    })
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

        if changes.all_changed_dirs().contains(&nested_dir_str) {
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

fn persist_scan_result_sync(cache_db: &ScanCacheDb, disk_path: &str, scan_type: &str, result: &ScanResult) -> Result<()> {
    let json = serde_json::to_string(result)?;
    cache_db.save_scan_result(disk_path, scan_type, &json, result.total_files as i64, result.total_size as i64)?;
    Ok(())
}

fn cache_has_usable_usn_checkpoint(result: &ScanResult) -> bool {
    result.usn_journal_id.is_some() && result.usn_next_usn.is_some()
}

fn should_rebuild_cached_deep_scan(path: &Path, result: &ScanResult) -> bool {
    let backend = result.scan_backend.as_deref().unwrap_or("unknown");
    let expects_usn = matches!(backend, "mft_usn" | "incremental_usn");

    (expects_usn && !cache_has_usable_usn_checkpoint(result))
        || (winfs::supports_mft_scan(path) && !cache_has_usable_usn_checkpoint(result) && result.total_dirs > 50_000)
}

fn build_scan_benchmark_run(
    strategy: &str,
    cache_present_before: bool,
    incremental_candidate_before: bool,
    wall_start: Instant,
    result: &ScanResult,
) -> ScanBenchmarkRun {
    ScanBenchmarkRun {
        strategy: strategy.to_string(),
        cache_present_before,
        incremental_candidate_before,
        wall_duration_ms: wall_start.elapsed().as_millis() as u64,
        reported_scan_duration_ms: result.scan_duration_ms,
        scan_backend: result.scan_backend.clone(),
        total_files: result.total_files,
        total_dirs: result.total_dirs,
        total_size: result.total_size,
        inaccessible_count: result.inaccessible_count,
    }
}

async fn run_deep_scan_pass(
    scanner: &DiskScanner,
    cache_db: &ScanCacheDb,
    target_path: &Path,
    estimated_files: usize,
) -> Result<ScanBenchmarkRun> {
    let target_key = target_path.to_string_lossy().to_string();
    let wall_start = Instant::now();
    let cached = cache_db.get_scan_result(&target_key, "deep")?;
    let cache_present_before = cached.is_some();

    let (strategy, incremental_candidate_before, full_result) = match cached {
        Some(cached) => match serde_json::from_str::<ScanResult>(&cached.result_json) {
            Ok(cached_result) if should_rebuild_cached_deep_scan(target_path, &cached_result) => (
                "fresh_rebuild_stale_deep_cache",
                false,
                scanner.scan_deep_silent(target_path, estimated_files).await?,
            ),
            Ok(cached_result) => (
                "incremental_cache",
                true,
                incremental::scan_incremental_silent(target_path, cached_result).await?,
            ),
            Err(_) => (
                "fresh_rebuild_corrupt_deep_cache",
                false,
                scanner.scan_deep_silent(target_path, estimated_files).await?,
            ),
        },
        None => (
            "fresh_scan",
            false,
            scanner.scan_deep_silent(target_path, estimated_files).await?,
        ),
    };

    persist_scan_result_sync(cache_db, &target_key, "deep", &full_result)?;
    scanner.store_indexed_scan_result(&full_result);
    let result = scanner
        .get_directory_snapshot(&target_key, &target_key)
        .ok_or_else(|| anyhow!("failed to retrieve root snapshot for deep benchmark"))?;

    Ok(build_scan_benchmark_run(
        strategy,
        cache_present_before,
        incremental_candidate_before,
        wall_start,
        &result,
    ))
}
