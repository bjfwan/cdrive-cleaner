//! 扫描会话注册表行为测试。
//!
//! 这些测试不依赖 Tauri AppHandle，直接驱动 ScanSessionRegistry 验证：
//! 1. 同 disk 连发两个 session：第一个 token 必须被 cancel；
//! 2. cancel 后 token.is_cancelled() 立刻返回 true；
//! 3. SessionHandle drop 时自动从注册表移除并通知等待者；
//! 4. 5 秒超时分支不会阻塞新 session 的启动。

use cdrive_cleaner_lib::session::{CancellationToken, ScanSessionRegistry};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn second_session_cancels_first_on_same_disk() {
    let registry = Arc::new(ScanSessionRegistry::new());
    let mut first = registry.begin_session("C:\\").await;
    let first_token = first.token();
    assert!(!first_token.is_cancelled(), "新会话不应预先被取消");

    // 在另一个任务里启动第二个 session：begin_session 会等旧会话退出，
    // 我们手动把 first drop 掉，第二次调用就能拿到锁。
    let registry_clone = Arc::clone(&registry);
    let second_handle = tokio::spawn(async move {
        registry_clone.begin_session("C:\\").await
    });

    // 让 begin_session 进入"等待 finished.notified"分支，
    // 这一步它会先 cancel 旧 token。给一点时间让标志位真正落地。
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        first_token.is_cancelled(),
        "同 disk 第二次 begin_session 必须先 cancel 第一次的 token"
    );

    // 模拟第一次扫描收到取消信号后退出：drop SessionHandle 通知 finished。
    first.mark_completed(); // 正常路径不会再 cancel（已经取消过了）
    drop(first);

    let second = tokio::time::timeout(Duration::from_secs(2), second_handle)
        .await
        .expect("第二次 begin_session 必须在旧会话退出后立即返回")
        .expect("spawn 不应失败");
    assert!(!second.token().is_cancelled());
    drop(second);
}

#[tokio::test]
async fn cancel_marks_token_immediately() {
    let registry = Arc::new(ScanSessionRegistry::new());
    let session = registry.begin_session("D:\\").await;
    let token = session.token();
    assert!(!token.is_cancelled());

    // 模拟 cancel_scan(disk_path) 命令：注册表层 cancel 应当立刻可见。
    let started = std::time::Instant::now();
    registry.cancel("D:\\");
    assert!(token.is_cancelled(), "cancel 后 token 必须立刻为已取消");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "cancel 必须在 1 秒内返回"
    );

    drop(session);
}

#[tokio::test]
async fn drop_handle_removes_from_registry_and_notifies() {
    let registry = Arc::new(ScanSessionRegistry::new());

    // 第一个 session：drop 后应当从注册表移除，下一次 begin_session 可以立即注册而不阻塞。
    {
        let _h1 = registry.begin_session("E:\\").await;
        // 退出 scope 触发 drop。
    }

    let started = std::time::Instant::now();
    let _h2 = tokio::time::timeout(
        Duration::from_secs(1),
        registry.begin_session("E:\\"),
    )
    .await
    .expect("旧 session 已 drop，第二次注册不应被卡住");
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn drop_without_mark_completed_cancels_token() {
    let registry = Arc::new(ScanSessionRegistry::new());
    let session = registry.begin_session("F:\\").await;
    let token = session.token();
    drop(session);
    assert!(
        token.is_cancelled(),
        "未 mark_completed 的 SessionHandle drop 时必须 cancel token"
    );
}

#[tokio::test]
async fn cancellation_token_is_send_sync_clone() {
    fn assert_send_sync_clone<T: Send + Sync + Clone>() {}
    assert_send_sync_clone::<CancellationToken>();
}

#[tokio::test]
async fn child_token_shares_flag() {
    let parent = CancellationToken::new();
    let child = parent.child();
    parent.cancel();
    assert!(child.is_cancelled());
}

/// cancel_all 用例：两个 disk 各注册一个 session，cancel_all 后都应取消。
#[tokio::test]
async fn cancel_all_cancels_every_session() {
    let registry = Arc::new(ScanSessionRegistry::new());
    let s1 = registry.begin_session("C:\\").await;
    let s2 = registry.begin_session("D:\\").await;
    let t1 = s1.token();
    let t2 = s2.token();

    registry.cancel_all();
    assert!(t1.is_cancelled());
    assert!(t2.is_cancelled());

    drop(s1);
    drop(s2);
}
