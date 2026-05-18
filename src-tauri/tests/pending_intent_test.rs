//! pending_scan.json 持久化往返测试。
//!
//! `utils::get_app_data_dir()` 把 intent 写到 `<exe_dir>/data/pending_scan.json`。
//! 测试期间 `current_exe()` 指向 cargo 测试可执行文件，写入路径稳定可控。
//! 为了避免和真实运行环境串扰，每个测试都先手动清掉残留文件。

use std::time::Duration;

use cdrive_cleaner_lib::pending_intent::{self, PendingScanIntent};
use chrono::Utc;
use serial_test::serial;

fn intent_path() -> std::path::PathBuf {
    pending_intent::test_intent_path().expect("intent path 解析失败")
}

fn clear_intent() {
    let path = intent_path();
    if path.exists() {
        std::fs::remove_file(&path).ok();
    }
}

#[test]
#[serial]
fn write_then_consume_round_trip() {
    clear_intent();
    let intent = PendingScanIntent::new("C:\\", true);
    pending_intent::write(&intent).expect("写入 intent");

    let loaded = pending_intent::consume().expect("读到上一次写入的 intent");
    assert_eq!(loaded.disk, "C:\\");
    assert!(loaded.requested_with_elevation);
    assert!(!loaded.is_expired(Utc::now()));
}

#[test]
#[serial]
fn consume_removes_file() {
    clear_intent();
    let intent = PendingScanIntent::new("D:\\", false);
    pending_intent::write(&intent).expect("写入 intent");

    assert!(intent_path().exists(), "写入后文件应当存在");
    let _ = pending_intent::consume();
    assert!(
        !intent_path().exists(),
        "consume 后 pending_scan.json 必须被删除，避免下次启动重复触发"
    );
}

#[test]
#[serial]
fn consume_returns_none_when_missing() {
    clear_intent();
    assert!(pending_intent::consume().is_none());
}

#[test]
#[serial]
fn malformed_json_returns_none_and_does_not_panic() {
    clear_intent();
    let path = intent_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, b"{not valid json").expect("写损坏 JSON");
    let result = pending_intent::consume();
    assert!(result.is_none());
    assert!(
        !path.exists(),
        "consume 必须把损坏的 intent 文件清理掉，否则会反复触发解析失败"
    );
}

#[test]
#[serial]
fn ttl_expired_returns_none() {
    clear_intent();
    let mut intent = PendingScanIntent::new("E:\\", true);
    // 把 requested_at 倒推 11 分钟，超过 10 分钟 TTL。
    intent.requested_at = Utc::now() - chrono::Duration::minutes(11);
    pending_intent::write(&intent).expect("写入过期 intent");

    let loaded = pending_intent::consume();
    assert!(loaded.is_none(), "TTL 过期的 intent 应当返回 None");
    assert!(
        !intent_path().exists(),
        "过期 intent 仍然要被删除，避免每次启动都触发"
    );
}

#[test]
#[serial]
fn write_overwrites_previous_intent() {
    clear_intent();
    pending_intent::write(&PendingScanIntent::new("C:\\", true)).unwrap();
    pending_intent::write(&PendingScanIntent::new("D:\\", false)).unwrap();
    let loaded = pending_intent::consume().expect("应当读到最后写入的 intent");
    assert_eq!(loaded.disk, "D:\\");
    assert!(!loaded.requested_with_elevation);
}

#[test]
fn ttl_struct_method_classifies_correctly() {
    let mut intent = PendingScanIntent::new("C:\\", true);
    assert!(!intent.is_expired(Utc::now()));
    intent.requested_at = Utc::now() - chrono::Duration::minutes(11);
    assert!(intent.is_expired(Utc::now()));
}

#[test]
fn ttl_constant_is_ten_minutes() {
    assert_eq!(
        pending_intent::PENDING_INTENT_TTL,
        Duration::from_secs(10 * 60)
    );
}
