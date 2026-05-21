#[cfg(target_os = "windows")]
mod tests {
    use cdrive_cleaner_lib::migration::{FileMigrator, LinkType};
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK.get_or_init(|| Mutex::new(()))
    }

    struct TestWorkspace {
        root: PathBuf,
        source_root: PathBuf,
        target_root: PathBuf,
    }

    impl TestWorkspace {
        fn new(name: &str) -> Self {
            let unique = format!(
                "{}-{}-{}",
                name,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );

            let root = std::env::temp_dir().join(unique);
            let source_root = root.join("source");
            let target_root = root.join("target");
            fs::create_dir_all(&source_root).unwrap();
            fs::create_dir_all(&target_root).unwrap();

            Self {
                root,
                source_root,
                target_root,
            }
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn write_file(path: &Path, contents: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        let mut file = fs::File::create(path).unwrap();
        file.write_all(contents).unwrap();
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn migrates_file_with_hardlink_and_cleans_up_via_temp_workspace() {
        let _guard = test_lock().lock().unwrap();
        let workspace = TestWorkspace::new("file-migration");
        let source_file = workspace.source_root.join("test.txt");
        let original = b"temporary migration payload";
        write_file(&source_file, original);

        let migrator = FileMigrator::new();
        let result = migrator
            .migrate(
                &source_file,
                &workspace.target_root,
                LinkType::Hardlink,
                None,
                None,
            )
            .await
            .expect("file migration should complete");

        assert!(result.success, "file migration should succeed");
        assert_eq!(result.link_type, LinkType::Hardlink);
        assert!(
            source_file.exists(),
            "source path should become a hard link"
        );
        assert!(
            Path::new(&result.target_path).exists(),
            "target file should exist"
        );
        assert_eq!(fs::read(&source_file).unwrap(), original);
        assert_eq!(fs::read(&result.target_path).unwrap(), original);
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn migrates_directory_with_junction_and_cleans_up_via_temp_workspace() {
        let _guard = test_lock().lock().unwrap();
        let workspace = TestWorkspace::new("dir-migration");
        let source_dir = workspace.source_root.join("folder");
        let nested_file = source_dir.join("nested").join("data.txt");
        let original = b"directory payload";
        write_file(&nested_file, original);

        let migrator = FileMigrator::new();
        let result = migrator
            .migrate(
                &source_dir,
                &workspace.target_root,
                LinkType::Junction,
                None,
                None,
            )
            .await
            .expect("directory migration should complete");

        assert!(result.success, "directory migration should succeed");
        assert_eq!(result.link_type, LinkType::Junction);
        assert!(source_dir.exists(), "source path should become a junction");
        assert!(
            Path::new(&result.target_path).exists(),
            "target directory should exist"
        );
        assert_eq!(
            fs::read(source_dir.join("nested").join("data.txt")).unwrap(),
            original
        );
        assert_eq!(
            fs::read(
                Path::new(&result.target_path)
                    .join("nested")
                    .join("data.txt")
            )
            .unwrap(),
            original
        );
    }
}
