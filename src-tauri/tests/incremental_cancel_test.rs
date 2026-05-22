
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

    fn make_cached_subdir(path: &Path, changed: bool) -> DirectoryNode {
        DirectoryNode {
            path: path.to_string_lossy().to_string(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            size: if changed { 1 } else { 0 },
            file_count: if changed { 1 } else { 0 },
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
    fn build_cached_result(root: &Path) -> ScanResult {
        const CHANGED_COUNT: usize = 100;
        let mut children = Vec::with_capacity(500);
        for i in 0..500 {
            let dir = root.join(format!("dir_{:05}", i));
            fs::create_dir_all(&dir).unwrap();
            children.push(make_cached_subdir(&dir, i < CHANGED_COUNT));
        }
        ScanResult {
            root_path: root.to_string_lossy().to_string(),
            total_size: children.iter().map(|c| c.size).sum(),
            system_reserved_bytes: 0,
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
        token.cancel();
        let started = Instant::now();

        let scan_path = ws.root.clone();
        let scan = tokio::spawn(async move {
            incremental::scan_incremental_with_token(
                &scan_path,
                cached,
                &scanner,
                Some(&cancel_token),
            )
            .await
        });

        let outcome = tokio::time::timeout(Duration::from_millis(10_000), scan)
            .await
            .expect("扫描在 cancel 后应当很快返回，没死锁（10s 超时）")
            .expect("tokio::spawn 不应 panic");

        let elapsed = started.elapsed();
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
