//! 系统时钟回拨保护回归。
//!
//! mtime 在未来 +1h 的目录必须被纳入重扫；而 mtime 比缓存早一点点（< 60s 容差）
//! 的目录不应被误判为"过去时间"，避免 NTP 微调引发不必要的全树重扫。

use cdrive_cleaner_lib::scanner::file_info::DirectoryNode;
use cdrive_cleaner_lib::scanner::incremental::{
    check_directory_changes_with_clock, ChangeStatus,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn new(name: &str) -> Self {
        let unique = format!(
            "csd-incremental-clock-{}-{}-{}",
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

fn make_cached_node(path: &Path, modified_secs: Option<u64>) -> DirectoryNode {
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
        children: Vec::new(),
        has_children: false,
        is_symlink: false,
        link_target: None,
        safety: None,
        modified_time: modified_secs,
        file_id: None,
    }
}

#[test]
fn future_mtime_is_treated_as_modified() {
    // 目录的当前 mtime 默认是"刚创建"，给它一个"远在未来"的缓存时间反着比也行——
    // 只要满足 |fs.mtime - cached.mtime| > 0 就会被判 Modified。
    // 但本测试要求"未来 mtime > wall_now + 60s"必须强制 Modified，即便 cached 也指
    // 向未来时间相同的值。最直接的验证：模拟一个 wall_now 远小于当前文件 mtime，
    // 同时 cached 也写一个相等的 modified_time —— 标准比较会判 Unchanged，但时钟
    // 保护会强制 Modified。
    let ws = Workspace::new("future-mtime");
    let dir = ws.root.join("future_dir");
    fs::create_dir_all(&dir).unwrap();
    let actual_mtime = fs::symlink_metadata(&dir)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // cached 故意和真实 mtime 完全一致 —— 走标准路径会判 Unchanged。
    let cached = make_cached_node(&dir, Some(actual_mtime));

    // 给一个比文件 mtime 还落后 1 小时的 wall_now：
    // wall_now = mtime - 3600，那么 mtime > wall_now + 60s 必然成立 → 强制 Modified。
    let wall_now = actual_mtime.saturating_sub(3600);
    let status = check_directory_changes_with_clock(&cached, &dir, wall_now);

    assert_eq!(
        status,
        ChangeStatus::Modified,
        "mtime 远远落后于 wall_now 时（即文件时间在未来），必须强制 Modified；\
         actual_mtime={actual_mtime} wall_now={wall_now}"
    );
}

#[test]
fn small_backward_mtime_drift_is_not_modified() {
    // mtime 比 cached 早一点点（< 60s 容差），属于 NTP 微调，不应判 Modified。
    let ws = Workspace::new("ntp-drift");
    let dir = ws.root.join("ntp_drift_dir");
    fs::create_dir_all(&dir).unwrap();
    let actual_mtime = fs::symlink_metadata(&dir)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // cached 比当前 mtime 晚 30 秒（缓存里看起来"超前"，文件 mtime 反而"过去"了一点点）
    let cached_mtime = actual_mtime + 30;
    let cached = make_cached_node(&dir, Some(cached_mtime));

    // wall_now 设到当前时间附近，避免触发未来时钟保护。
    let wall_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let status = check_directory_changes_with_clock(&cached, &dir, wall_now);

    assert_eq!(
        status,
        ChangeStatus::Unchanged,
        "cached 比 fs mtime 晚 < 60s 时不应被判 Modified；\
         actual_mtime={actual_mtime} cached_mtime={cached_mtime}"
    );
}

#[test]
fn large_backward_mtime_drift_is_modified() {
    // mtime 比 cached 早超过 60s 容差，应当判 Modified —— 真的有人/进程把
    // 时间改回去了 / 替换文件用了更早的时间戳 / etc.
    let ws = Workspace::new("real-rollback");
    let dir = ws.root.join("real_rollback_dir");
    fs::create_dir_all(&dir).unwrap();
    let actual_mtime = fs::symlink_metadata(&dir)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let cached_mtime = actual_mtime + 600; // 相差 10 分钟
    let cached = make_cached_node(&dir, Some(cached_mtime));

    let wall_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let status = check_directory_changes_with_clock(&cached, &dir, wall_now);
    assert_eq!(status, ChangeStatus::Modified);
}
