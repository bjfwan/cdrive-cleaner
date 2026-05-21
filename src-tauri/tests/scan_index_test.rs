use cdrive_cleaner_lib::scanner::file_info::{DirectoryNode, FileInfo, ScanResult};
use cdrive_cleaner_lib::scanner::scan_index::IndexedScanResult;

fn dir(
    path: &str,
    name: &str,
    size: u64,
    file_count: usize,
    dir_count: usize,
    children: Vec<DirectoryNode>,
) -> DirectoryNode {
    DirectoryNode {
        path: path.to_string(),
        name: name.to_string(),
        size,
        file_count,
        dir_count,
        children,
        has_children: false,
        is_symlink: false,
        link_target: None,
        safety: None,
        modified_time: None,
        file_id: None,
    }
}

fn file(path: &str, name: &str, size: u64) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        name: name.to_string(),
        size,
        extension: String::new(),
        modified_at: String::new(),
        is_readonly: false,
        is_symlink: false,
        link_target: None,
    }
}

fn fixture_result() -> ScanResult {
    let leaf = dir(r"C:\fixture\sub\leaf", "leaf", 100, 1, 1, Vec::new());
    let sub = dir(r"C:\fixture\sub", "sub", 300, 2, 2, vec![leaf]);
    let sibling = dir(r"C:\fixture\submarine", "submarine", 200, 1, 1, Vec::new());

    ScanResult {
        root_path: r"C:\fixture".to_string(),
        total_size: 500,
        system_reserved_bytes: 4096,
        total_files: 3,
        total_dirs: 4,
        scan_duration_ms: 12,
        directories: vec![sub, sibling],
        large_files: vec![
            file(r"C:\fixture\sub\small.bin", "small.bin", 10),
            file(r"C:\fixture\sub\big.bin", "big.bin", 90),
            file(r"C:\fixture\submarine\other.bin", "other.bin", 100),
        ],
        inaccessible_count: 0,
        scan_backend: Some("native".to_string()),
        root_file_id: Some(1),
        usn_journal_id: Some(2),
        usn_next_usn: Some(3),
        cache_schema_version: 2,
        env_fingerprint: Default::default(),
        scan_completed: true,
    }
}

#[test]
fn root_snapshot_preserves_system_reserved_and_child_snapshot_does_not() {
    let indexed = IndexedScanResult::from_scan_result(&fixture_result());

    let root = indexed.snapshot_for_path(r"C:\fixture").unwrap();
    assert_eq!(root.system_reserved_bytes, 4096);
    assert_eq!(root.total_size, 500);
    assert_eq!(root.total_files, 3);
    assert_eq!(root.total_dirs, 4);
    assert_eq!(root.directories.len(), 2);

    let child = indexed.snapshot_for_path(r"C:\fixture\sub").unwrap();
    assert_eq!(child.system_reserved_bytes, 0);
    assert_eq!(child.total_size, 300);
    assert_eq!(child.total_files, 2);
    assert_eq!(child.total_dirs, 1);
    assert_eq!(child.directories.len(), 1);
}

#[test]
fn children_snapshot_returns_direct_children_without_full_scan_payload() {
    let indexed = IndexedScanResult::from_scan_result(&fixture_result());

    let root = indexed.children_snapshot_for_path(r"C:\fixture").unwrap();
    assert_eq!(root.root_path, r"C:\fixture");
    assert_eq!(root.total_size, 500);
    assert_eq!(root.total_files, 3);
    assert_eq!(root.total_dirs, 4);
    assert_eq!(root.directories.len(), 2);
    assert!(root.directories.iter().all(|node| node.children.is_empty()));

    let child = indexed.children_snapshot_for_path(r"C:\fixture\sub").unwrap();
    assert_eq!(child.root_path, r"C:\fixture\sub");
    assert_eq!(child.total_size, 300);
    assert_eq!(child.total_files, 2);
    assert_eq!(child.total_dirs, 1);
    assert_eq!(child.directories.len(), 1);
    assert_eq!(child.directories[0].path, r"C:\fixture\sub\leaf");
}

#[test]
fn large_files_for_path_uses_path_boundary_and_size_order() {
    let indexed = IndexedScanResult::from_scan_result(&fixture_result());

    let large_files = indexed.large_files_for_path_public(r"C:\fixture\sub");
    let paths: Vec<&str> = large_files.iter().map(|file| file.path.as_str()).collect();

    assert_eq!(
        paths,
        vec![r"C:\fixture\sub\big.bin", r"C:\fixture\sub\small.bin"]
    );
}
