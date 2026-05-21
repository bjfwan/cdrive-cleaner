use cdrive_cleaner_lib::scanner::duplicates::find_duplicates_blocking;
use cdrive_cleaner_lib::scanner::file_info::FileInfo;
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn make_file(dir: &Path, name: &str, content: &[u8]) -> FileInfo {
    let path = dir.join(name);
    let mut f = File::create(&path).unwrap();
    f.write_all(content).unwrap();
    FileInfo {
        path: path.to_string_lossy().to_string(),
        name: name.to_string(),
        size: content.len() as u64,
        extension: String::new(),
        modified_at: "2026-01-01 00:00:00".to_string(),
        is_readonly: false,
        is_symlink: false,
        link_target: None,
    }
}

fn workspace(prefix: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cdrive-dup-{}-{}-{}",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn detects_two_identical_large_files() {
    let dir = workspace("identical");
    let payload = vec![7u8; (100 * 1024 * 1024 + 8) as usize];
    let a = make_file(&dir, "a.bin", &payload);
    let b = make_file(&dir, "b.bin", &payload);
    let mut other_payload = payload.clone();
    other_payload[0] = 9;
    let c = make_file(&dir, "c.bin", &other_payload);

    let groups = find_duplicates_blocking(vec![a, b, c], None, None).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].files.len(), 2);
    assert_eq!(groups[0].wasted_bytes, payload.len() as u64);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ignores_files_below_threshold() {
    let dir = workspace("small");
    let payload = vec![3u8; 1024];
    let a = make_file(&dir, "a.bin", &payload);
    let b = make_file(&dir, "b.bin", &payload);
    let groups = find_duplicates_blocking(vec![a, b], None, None).unwrap();
    assert!(groups.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn distinguishes_same_size_different_content() {
    let dir = workspace("collision");
    let size = 100 * 1024 * 1024 + 1024;
    let mut a_payload = vec![0u8; size];
    a_payload[0] = 0xAB;
    a_payload[size - 1] = 0xCD;
    let mut b_payload = vec![0u8; size];
    b_payload[0] = 0xAB;
    b_payload[size - 1] = 0xEF;

    let a = make_file(&dir, "a.bin", &a_payload);
    let b = make_file(&dir, "b.bin", &b_payload);
    let groups = find_duplicates_blocking(vec![a, b], None, None).unwrap();
    assert!(groups.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
