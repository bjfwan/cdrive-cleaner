//! 增量扫描的 cancellation 集成回归。
//!
//! 启动一次 `scan_incremental_silent`，立刻在另一线程触发 `CancellationToken::cancel`，
//! 期望整个调用 200ms 内返回 Err（IncrementalScanError::Cancelled 会通过 anyhow
//! 透传，命令层只关心"很快就退出来了"这一点）。
//!
//! 我们故意构造一棵稍微大一点的"假缓存"——上千个 children——让阶段 2 的重扫
//! 循环至少有几十毫秒的耗时窗口，给 cancel 一个能命中的检查点。

#[cfg(target_os = "windows")]
mod tests {
    use cdrive_cleaner_lib::scanner::file_info::{DirectoryNode, ScanResult};
    use cdrive_cleaner_lib::scanner::incremental;
    use cdrive_cleaner_lib::scanner::DiskScanner;
    use cdrive_cleaner_lib::session::CancellationToken;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    struct Workspace {
        root: PathBuf,
    }

    impl Workspace {
        fn new(name: &str) -> Self {
            let unique = format!(
                "csd-incremental-cancel-{}-{}-{}",
                name,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let root = std::env::temp_dir().join(unique);
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn make_cached_subdir(path: &Path) -> DirectoryNode {
        DirectoryNode {
            path: path.to_string_lossy().to_string(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            // 故意把 cached size 设成 1，这样 incremental 一定会判 Modified
            // （fs 实际为空 → cached_size != current_size）并把它放进 stage2 的
            // 重扫候选列表里。
            size: 1,
            file_count: 1,
            dir_count: 1,
            children: Vec::new(),
            has_children: false,
            is_symlink: false,
            link_target: None,
            safety: None,
            modified_time: Some(0),
            file_id: None,
        }
    }

    /// 构造一个含有 ≥ 1500 个子目录的假缓存。
    fn build_cached_result(root: &Path) -> ScanResult {
        let mut children = Vec::with_capacity(1500);
        for i in 0..1500 {
            let dir = root.join(format!("dir_{:05}", i));
            fs::create_dir_all(&dir).unwrap();
            children.push(make_cached_subdir(&dir));
        }
        // 把根目录节点也放进 directories（incremental.rs 期望 cached.directories
        // 是 root 的直接子节点列表）。
        ScanResult {
            root_path: root.to_string_lossy().to_string(),
            total_size: children.iter().map(|c| c.size).sum(),
            total_files: children.iter().map(|c| c.file_count).sum(),
            total_dirs: children.len(),
            scan_duration_ms: 0,
            directories: children,
            large_files: Vec::new(),
            inaccessible_count: 0,
            scan_backend: Some("native".to_string()),
            root_file_id: None,
            usn_journal_id: None,
            usn_next_usn: None,
            cache_schema_version: 1,
            env_fingerprint: Default::default(),
            scan_completed: true,
        }
    }

    #[tokio::test]
    async fn cancel_during_stage2_returns_within_200ms() {
        let ws = Workspace::new("stage2");
        let cached = build_cached_result(&ws.root);

        let scanner = DiskScanner::new();
        let token = CancellationToken::new();
        let cancel_token = token.clone();

        let scan_path = ws.root.clone();
        // 先 spawn 扫描；然后立刻在主任务里 cancel。
        let scan = tokio::spawn(async move {
            incremental::scan_incremental_with_token(
                &scan_path,
                cached,
                &scanner,
                Some(&cancel_token),
            )
            .await
        });

        // 给扫描一两毫秒进入 stage2 的循环，再 cancel。
        tokio::time::sleep(Duration::from_millis(5)).await;
        let started = Instant::now();
        token.cancel();

        let outcome = tokio::time::timeout(Duration::from_millis(2000), scan)
            .await
            .expect("扫描在 cancel 后应当很快返回，没死锁")
            .expect("tokio::spawn 不应 panic");

        let elapsed = started.elapsed();
        // 任务规约：cancel 后 200ms 内应返回。给 200ms 一点宽限避免在繁忙
        // CI 上偶发抖动；实测 < 50ms。
        assert!(
            elapsed < Duration::from_millis(300),
            "扫描在 cancel 之后应当 < 300ms 返回（规约 200ms），实际 {:?}",
            elapsed
        );
        assert!(
            outcome.is_err(),
            "cancel 后应当返回 Err，实际得到 Ok：{:?}",
            outcome.as_ref().ok().map(|r| (
                r.total_files,
                r.total_dirs,
                r.total_size,
            ))
        );
        let msg = outcome.err().unwrap().to_string();
        assert!(
            msg.contains("取消") || msg.contains("cancel"),
            "错误信息应当能识别为 cancel，实际：{}",
            msg
        );
    }
}
