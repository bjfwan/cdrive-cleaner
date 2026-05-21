//! 端到端扫描测试：在临时目录里造一棵已知形状的目录树，
//! 用 DiskScanner::scan_deep_silent 扫一次，断言文件数 / 目录数 / 总大小都对。
//!
//! 这些测试不依赖 GUI，直接驱动 Rust 后端，是离线验证扫描器是否正常的可靠手段。

#[cfg(target_os = "windows")]
mod tests {
    use cdrive_cleaner_lib::scanner::DiskScanner;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    struct Workspace {
        root: PathBuf,
    }

    impl Workspace {
        fn new(name: &str) -> Self {
            let unique = format!(
                "csd-scan-{}-{}-{}",
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

    fn write_file(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(bytes).unwrap();
    }

    /// 已知目录树：3 个文件，已知大小，已知层级。
    /// 扫描后应能正确统计文件数、目录数、总大小。
    #[tokio::test]
    async fn scans_known_tree_with_correct_totals() {
        let ws = Workspace::new("known-tree");

        // 文件 1: root/a.txt        (100 字节)
        // 文件 2: root/sub/b.bin    (1024 字节)
        // 文件 3: root/sub/deep/c   (50 字节)
        write_file(&ws.root.join("a.txt"), &[b'a'; 100]);
        write_file(&ws.root.join("sub").join("b.bin"), &[b'b'; 1024]);
        write_file(&ws.root.join("sub").join("deep").join("c"), &[b'c'; 50]);

        let scanner = DiskScanner::new();
        let result = scanner
            .scan_deep_silent(&ws.root, 100)
            .await
            .expect("扫描应成功");

        assert_eq!(result.total_files, 3, "应该扫到 3 个文件，实际 {}", result.total_files);
        assert_eq!(
            result.total_size, 100 + 1024 + 50,
            "总大小应为 1174 字节，实际 {}",
            result.total_size
        );
        // 目录数 = root + sub + deep = 3（不同后端口径可能含或不含 root，放宽到 [2,3]）
        assert!(
            result.total_dirs >= 2 && result.total_dirs <= 3,
            "目录数应在 2~3，实际 {}",
            result.total_dirs
        );
        assert_eq!(result.inaccessible_count, 0, "测试目录不应有访问失败项");
    }

    /// 空目录：只有一个空 root，没有任何文件。
    #[tokio::test]
    async fn scans_empty_dir() {
        let ws = Workspace::new("empty");
        let scanner = DiskScanner::new();
        let result = scanner
            .scan_deep_silent(&ws.root, 10)
            .await
            .expect("空目录扫描应成功");

        assert_eq!(result.total_files, 0);
        assert_eq!(result.total_size, 0);
    }

    /// cancel 语义说明（基于源码 disk_scanner.rs L121 `self.reset_cancel()`）：
    /// 每次 `scan_deep*` 进入时会自动复位取消旗，因此"先 cancel 再 scan"对新扫描
    /// 没有副作用。这是预期行为，不是 bug。
    ///
    /// 此测试验证：cancel 之后立即开始的扫描可以正常完成（取消旗被重置）。
    #[tokio::test]
    async fn cancel_does_not_affect_subsequent_scan() {
        let ws = Workspace::new("cancel-reset");
        write_file(&ws.root.join("x"), b"hello");

        let scanner = DiskScanner::new();
        scanner.cancel();
        let result = scanner
            .scan_deep_silent(&ws.root, 10)
            .await
            .expect("scan_deep 进入时会复位 cancel 旗，扫描应正常完成");
        assert_eq!(result.total_files, 1);
        assert_eq!(result.total_size, 5);
    }
}
