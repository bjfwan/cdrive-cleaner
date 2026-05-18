//! USN 失效兜底回归。
//!
//! 真实 USN 走 FSCTL 调用、需要管理员权限，单测里不能直接驱动。
//! 这里改成验证"USN 全量重建"路径的决策契约：
//!
//! 1. 当上层 detect 路径返回 `UsnDetection::JournalReset`，对应的"全量重建候选"
//!    应该来自 `collect_all_cached_dirs_as_recursive`，且每个候选都是 Recursive。
//!    任何后续退化到 mtime 的代码路径都不该在这一步被走到。
//! 2. 候选列表里不会出现根目录本身（根的 own_files 由 `root_files_changed` 单独
//!    处理），同时所有候选都必须落在根目录的子树内。

use cdrive_cleaner_lib::scanner::file_info::DirectoryNode;
use cdrive_cleaner_lib::scanner::incremental::{
    collect_all_cached_dirs_as_recursive, ChangeStatus, RescanMode,
};
use std::path::{Path, PathBuf};

fn make_node(
    path: &Path,
    children: Vec<DirectoryNode>,
) -> DirectoryNode {
    DirectoryNode {
        path: path.to_string_lossy().to_string(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        size: 0,
        file_count: 0,
        dir_count: 1,
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
fn usn_journal_reset_does_not_collapse_to_mtime_walk() {
    // 缓存里有一棵 root/a/{b,c}/d 形状的树。
    let root = PathBuf::from("root");
    let dir_a = root.join("a");
    let dir_a_b = dir_a.join("b");
    let dir_a_c = dir_a.join("c");
    let dir_a_b_d = dir_a_b.join("d");

    let cached_tree = vec![make_node(
        &dir_a,
        vec![
            make_node(&dir_a_b, vec![make_node(&dir_a_b_d, vec![])]),
            make_node(&dir_a_c, vec![]),
        ],
    )];

    // 模拟 detect 路径返回 JournalReset：上层会调 collect_all_cached_dirs_as_recursive
    // 来构造 stage2 的候选列表。
    let candidates = collect_all_cached_dirs_as_recursive(&cached_tree, &root);

    // 期望：4 个候选（a / a\b / a\c / a\b\d），全部是 Recursive。
    assert_eq!(
        candidates.len(),
        4,
        "USN 全量重建应当把 cached 里每个非根目录都纳入候选；得到 {:?}",
        candidates
            .iter()
            .map(|c| (c.path.to_string_lossy().to_string(), c.mode))
            .collect::<Vec<_>>()
    );

    for candidate in &candidates {
        assert_eq!(
            candidate.mode,
            RescanMode::Recursive,
            "USN 全量重建里的候选必须是 Recursive，path={}",
            candidate.path.display()
        );
        assert_ne!(
            candidate.status,
            ChangeStatus::Deleted,
            "候选应当是 Modified，让 stage2 的 Recursive 重扫处理；path={}",
            candidate.path.display()
        );
        assert_ne!(
            candidate.path, root,
            "根目录自身不应出现在候选列表里"
        );
    }
}

#[test]
fn usn_journal_reset_handles_empty_cache_gracefully() {
    let root = PathBuf::from("root");
    let candidates = collect_all_cached_dirs_as_recursive(&[], &root);
    // 空缓存：候选为空。上层会回退到 mtime 全量（详见 incremental.rs 的
    // usn_full_rebuild_fallback_mtime 分支），但这一步本身必须返回空。
    assert!(candidates.is_empty());
}
