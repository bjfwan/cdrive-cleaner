use cdrive_cleaner_lib::scanner::file_info::DirectoryNode;
use cdrive_cleaner_lib::scanner::incremental;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn make_node(
    path: &Path,
    size: u64,
    file_count: usize,
    dir_count: usize,
    children: Vec<DirectoryNode>,
) -> DirectoryNode {
    DirectoryNode {
        path: path.to_string_lossy().to_string(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        size,
        file_count,
        dir_count,
        has_children: !children.is_empty(),
        children,
        is_symlink: false,
        link_target: None,
        safety: None,
        modified_time: None,
        file_id: None,
    }
}

#[test]
fn merge_keeps_parent_child_consistent_when_recursive_and_direct_files_overlap() {
    let root = PathBuf::from("root");
    let dir_p = root.join("P");
    let dir_q = dir_p.join("Q");
    let dir_r = dir_q.join("R");

    let old_q = make_node(
        &dir_q,
        25 + 5,
        2,
        2,
        vec![make_node(&dir_r, 25, 1, 1, vec![])],
    );
    let old_tree = vec![make_node(&dir_p, 30, 2, 3, vec![old_q])];

    let new_r = make_node(&dir_r, 100, 4, 1, vec![]);
    let new_q = make_node(
        &dir_q,
        50 + 100,
        5 + 4,
        2,
        vec![new_r.clone()],
    );
    let new_p = make_node(
        &dir_p,
        1 + (50 + 100),
        1 + (5 + 4),
        3,
        vec![new_q.clone()],
    );
    let direct_q = make_node(
        &dir_q,
        50 + 25,
        5 + 1,
        2,
        vec![make_node(&dir_r, 25, 1, 1, vec![])],
    );
    let mut own_overrides: HashMap<String, (u64, usize)> = HashMap::new();
    own_overrides.insert(normalized(&dir_p), (1, 1));
    own_overrides.insert(normalized(&dir_q), (50, 5));

    let merged = incremental::merge_scan_results_with_own(
        old_tree,
        vec![new_p, direct_q],
        &own_overrides,
        Vec::new(),
        &root,
    );

    let health = incremental::inspect_tree_merge_health(&merged, &root);
    assert_eq!(
        health.inconsistent_node_count, 0,
        "增量合并后不应出现父子统计漂移; samples = {:?}",
        health.inconsistent_node_samples
    );
    assert_eq!(
        health.duplicate_paths, 0,
        "增量合并后不应出现重复 path"
    );
}

fn normalized(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
    s
}
#[test]
fn rename_subtree_keeps_totals_stable() {
    let root = PathBuf::from("root");
    let dir_a = root.join("a");
    let dir_a_old = dir_a.join("old");
    let dir_a_new = dir_a.join("new");
    let dir_a_old_child = dir_a_old.join("child");
    let dir_a_new_child = dir_a_new.join("child");

    let cached_old_child = make_node(&dir_a_old_child, 100, 4, 1, vec![]);
    let cached_old = make_node(&dir_a_old, 150, 6, 2, vec![cached_old_child]);
    let cached_a = make_node(&dir_a, 150, 6, 3, vec![cached_old]);
    let _cached_tree = [cached_a];

    let new_child = make_node(&dir_a_new_child, 100, 4, 1, vec![]);
    let new_subtree = make_node(&dir_a_new, 150, 6, 2, vec![new_child]);
    let renamed_tree = vec![make_node(&dir_a, 150, 6, 3, vec![new_subtree])];

    let merged = incremental::merge_scan_results_with_own(
        renamed_tree,
        Vec::new(),
        &HashMap::new(),
        Vec::new(),
        &root,
    );

    let health = incremental::inspect_tree_merge_health(&merged, &root);
    assert_eq!(health.inconsistent_node_count, 0);

    let merged_a = &merged[0];
    assert_eq!(merged_a.size, 150);
    assert_eq!(merged_a.file_count, 6);
    assert_eq!(merged_a.children.len(), 1);
    assert_eq!(merged_a.children[0].path, dir_a_new.to_string_lossy());
    assert_eq!(merged_a.children[0].size, 150);
    assert_eq!(merged_a.children[0].file_count, 6);
}
