use cdrive_cleaner_lib::migration::delete::{
    delete_path, DeleteMode, DeleteProgress, DeleteProgressCallback,
};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct TestWorkspace {
    root: PathBuf,
}

impl TestWorkspace {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(format!("cdrive-cleaner-delete-{unique}"));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TestWorkspace {
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

#[tokio::test]
async fn delete_file_to_recycle_bin_succeeds() {
    let ws = TestWorkspace::new("recycle-file");
    let target = ws.path().join("trash-me.bin");
    write_file(&target, &[7u8; 4096]);
    assert!(target.exists());

    let result = delete_path(&target, DeleteMode::Recycle, None).await.unwrap();

    assert!(result.success, "delete failed: {:?}", result.errors);
    assert!(!target.exists());
    assert_eq!(result.errors.len(), 0);
    assert!(result.deleted_size >= 4096);
    assert!(result.deleted_files >= 1);
}

#[tokio::test]
async fn delete_directory_permanent_recursive() {
    let ws = TestWorkspace::new("perm-dir");
    let target = ws.path().join("nested");
    write_file(&target.join("a.txt"), b"alpha");
    write_file(&target.join("sub").join("b.bin"), &[1u8; 2048]);
    write_file(&target.join("sub").join("c.bin"), &[2u8; 1024]);

    let result = delete_path(&target, DeleteMode::Permanent, None).await.unwrap();

    assert!(result.success, "delete failed: {:?}", result.errors);
    assert!(!target.exists());
    assert_eq!(result.deleted_files, 3);
    assert!(result.deleted_size >= 5 + 2048 + 1024);
}

#[tokio::test]
async fn delete_partial_failure_when_file_locked() {
    let ws = TestWorkspace::new("locked");
    let target = ws.path().join("with-lock");
    write_file(&target.join("free.txt"), b"free");
    let locked_path = target.join("locked.bin");
    write_file(&locked_path, &[9u8; 256]);

    #[cfg(target_os = "windows")]
    let _lock = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked_path)
            .ok()
    };

    #[cfg(not(target_os = "windows"))]
    let _lock: Option<()> = None;

    let result = delete_path(&target, DeleteMode::Permanent, None).await.unwrap();

    if cfg!(target_os = "windows") {
        assert!(!result.success);
        assert!(!result.errors.is_empty());
        assert!(target.exists());
        assert!(result.deleted_files >= 1);
    } else {
        assert!(result.success);
        assert!(!target.exists());
    }
}

#[tokio::test]
async fn delete_emits_progress_events() {
    let ws = TestWorkspace::new("progress");
    let target = ws.path().join("many");
    for i in 0..20 {
        write_file(&target.join(format!("f{i}.bin")), &[i as u8; 4096]);
    }

    let events = Arc::new(Mutex::new(Vec::<DeleteProgress>::new()));
    let cb: DeleteProgressCallback = {
        let events = Arc::clone(&events);
        Arc::new(move |p| events.lock().unwrap().push(p))
    };

    let result = delete_path(&target, DeleteMode::Permanent, Some(cb))
        .await
        .unwrap();

    assert!(result.success, "delete failed: {:?}", result.errors);
    let events = events.lock().unwrap();
    assert!(!events.is_empty());
    assert!(events.iter().any(|e| e.progress_percent >= 99.9));
}

#[tokio::test]
async fn delete_missing_path_reports_error() {
    let ws = TestWorkspace::new("missing");
    let target = ws.path().join("nope.bin");

    let result = delete_path(&target, DeleteMode::Recycle, None).await.unwrap();

    assert!(!result.success);
    assert_eq!(result.errors.len(), 1);
}
