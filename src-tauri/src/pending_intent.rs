//! 「以管理员身份重启」前后的扫描意图持久化。
//!
//! 写入路径：`%LOCALAPPDATA%\com.csd.cleaner\pending_scan.json`，
//! 由 `utils::get_app_data_dir()` 提供（实际是 exe 同级 data 目录，
//! 名称沿用任务描述里的"应用数据目录"概念，避免重复实现）。
//!
//! 读取后立刻删除文件 + TTL 10 分钟，保证：
//! 1. 只触发一次自动续扫；
//! 2. UAC 失败 / 用户取消重启的情况下不会一直挂着脏 intent。

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "pending_scan.json";
/// TTL：10 分钟外当作过期，避免几小时前的 intent 还能触发自动扫描。
pub const PENDING_INTENT_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingScanIntent {
    pub disk: String,
    pub requested_at: DateTime<Utc>,
    pub requested_with_elevation: bool,
}

impl PendingScanIntent {
    pub fn new(disk: impl Into<String>, requested_with_elevation: bool) -> Self {
        Self {
            disk: disk.into(),
            requested_at: Utc::now(),
            requested_with_elevation,
        }
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        let elapsed = now.signed_duration_since(self.requested_at);
        match elapsed.to_std() {
            Ok(elapsed) => elapsed > PENDING_INTENT_TTL,
            // 负时长（比如系统时间倒拨） / 超过 i64 范围都视为过期，安全起见。
            Err(_) => true,
        }
    }
}

fn intent_path() -> Result<PathBuf> {
    let dir = crate::utils::get_app_data_dir().context("无法获取应用数据目录")?;
    Ok(dir.join(FILE_NAME))
}

/// 原子写：先写到 `pending_scan.json.tmp`，再 rename 覆盖目标文件。
pub fn write(intent: &PendingScanIntent) -> Result<()> {
    let target = intent_path()?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("无法创建应用数据目录 {}", parent.display())
        })?;
    }
    let tmp = target.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(intent).context("PendingScanIntent 序列化失败")?;
    std::fs::write(&tmp, &json)
        .with_context(|| format!("写入临时文件失败 {}", tmp.display()))?;
    // Windows 上 rename 到已存在文件需要先删旧的；用 std::fs::rename 在 Windows 默认不会覆盖。
    if target.exists() {
        let _ = std::fs::remove_file(&target);
    }
    std::fs::rename(&tmp, &target).with_context(|| {
        format!("rename {} -> {} 失败", tmp.display(), target.display())
    })?;
    Ok(())
}

/// 读出 pending intent 并立刻删除文件。文件缺失 / JSON 损坏 / 过期都返回 `None`，
/// 同时把损坏文件删除以避免反复触发解析失败。
pub fn consume() -> Option<PendingScanIntent> {
    let path = match intent_path() {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!("[pending-intent] 无法定位 intent 文件: {err}");
            return None;
        }
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            tracing::warn!("[pending-intent] 读取 {} 失败: {err}", path.display());
            return None;
        }
    };

    // 先删除文件，无论解析是否成功。这样下次启动不会再被同一份脏数据卡住。
    let _ = std::fs::remove_file(&path);

    let intent: PendingScanIntent = match serde_json::from_slice(&bytes) {
        Ok(i) => i,
        Err(err) => {
            tracing::warn!("[pending-intent] JSON 解析失败 {} -> {err}", path.display());
            return None;
        }
    };

    if intent.is_expired(Utc::now()) {
        tracing::info!(
            "[pending-intent] intent 已过期 disk={} requested_at={}",
            intent.disk,
            intent.requested_at
        );
        return None;
    }

    Some(intent)
}

#[doc(hidden)]
pub fn test_intent_path() -> Result<PathBuf> {
    intent_path()
}
