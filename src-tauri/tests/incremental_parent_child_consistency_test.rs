//! 增量合并的父子统计一致性回归。
//!
//! 复现 EBWebView\Default 案例：父目录 P 走 Recursive 重扫，同一次扫描里子目录 Q
//! 又走 DirectFilesOnly 重扫；如果 DirectFilesOnly 的"clone 旧 children"路径
//! 把 P 已经更新过的最新 Q 子树覆盖回旧版本，会出现父 size < Σ子 size。
//! 修复后 `inspect_tree_merge_health` 必须 inconsistent_node_count == 0。

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
    // 树结构：
    //   root/
    //     P/                    <- 走 Recursive 重扫，最新 size = 80
    //       Q/                  <- 同时也作为 DirectFilesOnly 候选
    //         R/
    //
    // 旧缓存里 P 含一个旧版的 Q；本次扫描 P 走 Recursive，得到的最新 P 已经
    // 包含最新版的 Q。如果合并阶段还把"旧 Q clone 一份"作为 DirectFilesOnly 节点
    // upsert 上去，就会把最新版 Q 覆盖回旧版，从而出现父子统计漂移。
    let root = PathBuf::from("root");
    let dir_p = root.join("P");
    let dir_q = dir_p.join("Q");
    let dir_r = dir_q.join("R");

    let old_q = make_node(
        &dir_q,
        // 旧 Q 自己：1 个文件 5 字节 + 子 R = 25 字节，1 个文件
        25 + 5,
        2,
        2,
        vec![make_node(&dir_r, 25, 1, 1, vec![])],
    );
    let old_tree = vec![make_node(&dir_p, 30, 2, 3, vec![old_q])];

    // P 走 Recursive：最新 P 自带最新的 Q（Q 自己 50 字节 5 文件 + 最新 R 100 字节 4 文件）
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
        // P 自身 1 字节 1 文件 + Q 子树
        1 + (50 + 100),
        1 + (5 + 4),
        3,
        vec![new_q.clone()],
    );

    // 同时还有一个 DirectFilesOnly 候选 Q：
    // 这一节点是按"复制旧 cached_node + 把 direct_size 替换"的方式得到的；
    // 它的 children 还是旧 R（25 字节 1 文件）
    let direct_q = make_node(
        &dir_q,
        // 自身 50 字节 5 文件 + 旧 R 25 字节 1 文件
        50 + 25,
        5 + 1,
        2,
        vec![make_node(&dir_r, 25, 1, 1, vec![])],
    );

    // 模拟 incremental.rs 阶段 3 的入参：把两个 changed_dirs 同时丢进合并器，
    // 同时把它们各自 own 的精确值用 own_overrides 透传进去（模拟 RescannedDirectory
    // 的 own_size / own_file_count 字段）。
    let mut own_overrides: HashMap<String, (u64, usize)> = HashMap::new();
    own_overrides.insert(normalized(&dir_p), (1, 1));
    own_overrides.insert(normalized(&dir_q), (50, 5));

    let merged = incremental::merge_scan_results_with_own(
        old_tree,
        // 注意顺序：P 在前 Q 在后；和实际线上 (changes 列表里两者并存) 一致。
        // keyed merge 会按深度从浅到深排序后 upsert，因此 Q 在 P 之后被插入。
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


/// 重命名识别（best-effort）：cached 树里把 dir_a/old 子树平移到 dir_a/new 之后，
/// 再走 keyed merge 应当等价于"什么都没动"，size/file_count/dir_count 完全不变。
///
/// 这是个纯逻辑回归——直接把 old 子树搬到 new 路径再 merge，验证
/// merge_scan_results_with_own 的"权威回填"在这种场景下是稳定的，
/// 不会因为 path 字符串变了就把统计漂移掉。
#[test]
fn rename_subtree_keeps_totals_stable() {
    let root = PathBuf::from("root");
    let dir_a = root.join("a");
    let dir_a_old = dir_a.join("old");
    let dir_a_new = dir_a.join("new");
    let dir_a_old_child = dir_a_old.join("child");
    let dir_a_new_child = dir_a_new.join("child");

    // cached：root/a/old/child  (size 100 / 4 files; old 自身 50 / 2 files)
    let cached_old_child = make_node(&dir_a_old_child, 100, 4, 1, vec![]);
    let cached_old = make_node(&dir_a_old, 150, 6, 2, vec![cached_old_child]);
    let cached_a = make_node(&dir_a, 150, 6, 3, vec![cached_old]);
    let _cached_tree = vec![cached_a];

    // 模拟 rename：把缓存里的 old 子树平移成 new 的 path 字符串。
    // 这正是 incremental.rs 里 apply_rename_pre_merge 干的事。
    let new_child = make_node(&dir_a_new_child, 100, 4, 1, vec![]);
    let new_subtree = make_node(&dir_a_new, 150, 6, 2, vec![new_child]);
    let renamed_tree = vec![make_node(&dir_a, 150, 6, 3, vec![new_subtree])];

    // 用空的 changed_dirs 走一遍 merge：等价于"rename 之后什么都没动"。
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
