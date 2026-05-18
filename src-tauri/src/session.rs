//! 扫描会话注册表 + CancellationToken。
//!
//! - `CancellationToken` 是对 `Arc<AtomicBool>` 的薄封装，`Send + Sync + Clone`，
//!   可以在 tokio 任务、Rayon worker、增量扫描主循环之间自由穿越。
//! - `CancellationGuard` 是 `CancellationToken` 的 RAII 封装：改名（renamed）后
//!   只在持有期间有效，Drop 时自动 `cancel()`，确保一次扫描走到一半 panic、
//!   提前 return 也不会留下"已注册但永远不会取消"的会话。
//! - `ScanSessionRegistry` 维护 `disk_path -> 会话` 的映射。新扫描进入前会先把
//!   同 disk 的旧会话 `cancel`，并通过 `Notify` 等它真正退出（带 5 秒超时）。
//!
//! 设计目标：
//! 1. 同一 disk_path 同时只有一份"权威"扫描会话；
//! 2. UAC 重启 / 进程退出 / panic 时不会留下半成品会话；
//! 3. 增量扫描的内部主循环可以通过 `is_cancelled()` 在合适的检查点退出，
//!    具体集成由任务 C 负责，本模块只暴露稳定接口。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;

/// 取消信号。多份 clone 共享同一份原子标志位。
///
/// `is_cancelled()` 频繁被增量扫描主循环调用，必须保持轻量（一次 atomic load）。
#[derive(Clone)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 显式触发取消。多次调用幂等。
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }

    /// 检查是否已取消。增量扫描主循环应在每个目录 / 每批 USN 记录后调用一次。
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    /// 返回一个共享同一标志位的子 token。语义上和 `clone()` 相同，
    /// 但保留显式名字以便业务代码表达"派生子任务"的意图。
    pub fn child(&self) -> CancellationToken {
        self.clone()
    }

    /// 以 `Arc<AtomicBool>` 形式暴露内部 flag，便于和现有 scanner / mft_usn
    /// 的 `Arc<AtomicBool>` 接口对齐——这些路径上的循环已经在频繁 load 同一种
    /// 形态的原子量，没必要再包一层。
    pub fn as_atomic(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.flag)
    }
}

/// `CancellationToken` 的 RAII 句柄：
/// - 持有期间共享同一标志位；
/// - Drop 时自动 `cancel()`，避免"会话 panic 后忘记取消"。
///
/// 命令体应该把 `CancellationGuard` 绑定在栈上，把 clone 出来的 `CancellationToken`
/// 透传给后台任务；命令返回 / panic 时 guard drop，token 自动 cancel。
pub struct CancellationGuard {
    token: CancellationToken,
    armed: bool,
}

impl CancellationGuard {
    pub fn new(token: CancellationToken) -> Self {
        Self { token, armed: true }
    }

    /// 取出共享同一 flag 的 token 副本。
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// 解除自动取消。当扫描正常完成、不希望 Drop 时再 cancel 时调用。
    pub fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CancellationGuard {
    fn drop(&mut self) {
        if self.armed {
            self.token.cancel();
        }
    }
}

struct SessionEntry {
    token: CancellationToken,
    /// 会话退出时 `notify_waiters()`，让正在等"前一个会话退出"的新会话醒过来。
    finished: Arc<Notify>,
}

/// 进程内扫描会话注册表。
pub struct ScanSessionRegistry {
    inner: Mutex<HashMap<String, SessionEntry>>,
}

impl Default for ScanSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 注册新会话时返回的句柄。drop 时自动从注册表移除并通知等待者。
pub struct SessionHandle {
    disk_key: String,
    token: CancellationToken,
    finished: Arc<Notify>,
    registry: Arc<ScanSessionRegistry>,
    cancel_on_drop: bool,
}

impl SessionHandle {
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// 标记会话正常完成：drop 时仍然从注册表移除并通知等待者，
    /// 但不会再额外触发 `cancel()`（已经是终态了）。
    pub fn mark_completed(&mut self) {
        self.cancel_on_drop = false;
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub fn disk_key(&self) -> &str {
        &self.disk_key
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        if self.cancel_on_drop {
            // panic / 提前返回路径：把 flag 标记为已取消，避免未完成的扫描
            // 还在写缓存。
            self.token.cancel();
        }
        self.registry.unregister_if_matches(&self.disk_key, &self.token);
        self.finished.notify_waiters();
    }
}

impl ScanSessionRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn normalize_key(disk_path: &str) -> String {
        let mut s = disk_path.trim().to_string();
        #[cfg(windows)]
        {
            s = s.replace('/', "\\").to_ascii_lowercase();
        }
        while s.ends_with('\\') && s.len() > 3 {
            s.pop();
        }
        s
    }

    fn unregister_if_matches(&self, disk_path: &str, token: &CancellationToken) {
        let key = Self::normalize_key(disk_path);
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        if let Some(entry) = guard.get(&key) {
            if Arc::ptr_eq(&entry.token.as_atomic(), &token.as_atomic()) {
                guard.remove(&key);
            }
        }
    }

    /// 调旧 token.cancel() 并返回它的 `Notify`，调用方可以 await 这个 notify
    /// 以等到旧会话真正退出。返回 `None` 表示当前没有旧会话。
    fn take_existing(&self, disk_path: &str) -> Option<(CancellationToken, Arc<Notify>)> {
        let key = Self::normalize_key(disk_path);
        let guard = self.inner.lock().ok()?;
        let entry = guard.get(&key)?;
        let token = entry.token.clone();
        let finished = Arc::clone(&entry.finished);
        // 立刻先把旧 token 标记为取消；移除注册表条目放到旧任务自然退出时。
        // 这样如果旧任务在 5 秒超时内退不出去，超时分支也能保证旧 token 已取消。
        token.cancel();
        // 不在这里 remove —— 旧任务退出时会调 unregister_if_matches，
        // 保证只有"对得上的那把 token"才能被它移除，避免误清新会话。
        drop(guard);
        Some((token, finished))
    }

    fn register(self: &Arc<Self>, disk_path: &str) -> SessionHandle {
        let key = Self::normalize_key(disk_path);
        let token = CancellationToken::new();
        let finished = Arc::new(Notify::new());

        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(
                key.clone(),
                SessionEntry {
                    token: token.clone(),
                    finished: Arc::clone(&finished),
                },
            );
        }

        SessionHandle {
            disk_key: key,
            token,
            finished,
            registry: Arc::clone(self),
            cancel_on_drop: true,
        }
    }

    /// 命令入口：开始一个新的扫描会话。
    /// - 若同 disk 已有 inflight，调旧 token.cancel() 并 await 它结束，最长等 5 秒；
    /// - 超时则丢弃旧 task（旧 task 自己退出时会清理注册表），新 session 仍然开始；
    /// - 返回的 `SessionHandle` 是 RAII，drop 时自动从注册表移除 + cancel。
    pub async fn begin_session(self: &Arc<Self>, disk_path: &str) -> SessionHandle {
        if let Some((_, finished)) = self.take_existing(disk_path) {
            tracing::info!(
                "[scan-session] 同 disk 已有 inflight 会话，先 cancel 旧会话再启动新会话 disk={}",
                disk_path
            );
            // 等旧会话自己走到 SessionHandle::drop（finished.notify_waiters）。
            let wait = tokio::time::timeout(Duration::from_secs(5), finished.notified()).await;
            if wait.is_err() {
                tracing::warn!(
                    "[scan-session] 旧会话 5 秒内没退出，继续启动新会话 disk={}",
                    disk_path
                );
            }
        }

        self.register(disk_path)
    }

    /// 主动取消某个 disk 的扫描。
    pub fn cancel(&self, disk_path: &str) {
        let key = Self::normalize_key(disk_path);
        if let Ok(guard) = self.inner.lock() {
            if let Some(entry) = guard.get(&key) {
                entry.token.cancel();
            }
        }
    }

    /// 取消所有 inflight 扫描。Tauri 关窗 / 应用退出时使用。
    pub fn cancel_all(&self) {
        if let Ok(guard) = self.inner.lock() {
            for entry in guard.values() {
                entry.token.cancel();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_clone_shares_flag() {
        let token = CancellationToken::new();
        let child = token.child();
        assert!(!token.is_cancelled());
        assert!(!child.is_cancelled());
        child.cancel();
        assert!(token.is_cancelled());
        assert!(child.is_cancelled());
    }

    #[test]
    fn guard_cancels_on_drop() {
        let token = CancellationToken::new();
        {
            let _guard = CancellationGuard::new(token.clone());
            assert!(!token.is_cancelled());
        }
        assert!(token.is_cancelled());
    }

    #[test]
    fn guard_disarm_skips_drop_cancel() {
        let token = CancellationToken::new();
        {
            let mut guard = CancellationGuard::new(token.clone());
            guard.disarm();
        }
        assert!(!token.is_cancelled());
    }
}
