//! v0.1.9 后台扫描调度器（Track A）
//!
//! 设计要点：
//! - Rust 侧只负责「计时 + 条件判定 + 发事件」；扫描动作交给前端复用现有的 scan_disk_deep。
//!   这样不用复制 scan_disk_deep 里的一大堆 tauri::State 注入逻辑，也避免和缓存/会话注册表打架。
//! - 触发流程：
//!     1. tick 任务每 60s 检查一次条件，命中后 emit "scheduler-run-scan { scanId, kind, path }"
//!     2. 前端监听后用 invoke('scan_disk_deep', { path }) 跑实际扫描
//!     3. 前端完成后 invoke('cmd_scheduler_mark_scan_done', { scanId, kind })
//!     4. mark_scan_done 更新时间戳并 emit "scheduler-scan-done { scanId, scan_id }"
//! - 这里 scan_id 设计成「被扫描的磁盘根路径字符串」（例如 "C:\\"）。Track B 的监听器
//!   会用这个值去 DiskScanner state 查刚出炉的快照，所以 scanId == root_path 是必要的。
//!   事件里同时塞 camelCase `scanId` 和 snake_case `scan_id`，便于不同消费者按习惯取值。
//! - Track A 字段（closeBehavior / schedulerEnabled / schedulerIdleMinutes）走 Track B 已经
//!   实现的 SettingsStore（dotted path）。本文件只在内部读取，不重复定义 cmd_settings_*。

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

use crate::ai::SettingsStore;

const INCREMENTAL_INTERVAL_SECS: u64 = 4 * 60 * 60; // 4 小时
const FULL_INTERVAL_SECS: u64 = 7 * 24 * 60 * 60; // 7 天
const DISK_IDLE_SECS: u64 = 30; // 上次扫描结束 30 秒内不再触发
const TICK_INTERVAL_SECS: u64 = 60;
const DEFAULT_SCAN_DISK: &str = "C:\\";

/// 关闭主窗口时的行为
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseBehavior {
    Exit,
    Tray,
}

impl Default for CloseBehavior {
    fn default() -> Self {
        CloseBehavior::Exit
    }
}

/// Track A 的运行期配置快照
#[derive(Debug, Clone)]
pub struct TrackASettings {
    pub close_behavior: CloseBehavior,
    pub scheduler_enabled: bool,
    pub scheduler_idle_minutes: u64,
}

impl Default for TrackASettings {
    fn default() -> Self {
        Self {
            close_behavior: CloseBehavior::Exit,
            scheduler_enabled: true,
            scheduler_idle_minutes: 5,
        }
    }
}

/// 调度器内部状态
pub struct SchedulerInner {
    /// 上次增量扫描完成时间（unix 秒）
    pub last_incremental: Option<u64>,
    /// 上次全量扫描完成时间（unix 秒）
    pub last_full: Option<u64>,
}

#[derive(Clone)]
pub struct SchedulerState {
    inner: Arc<Mutex<SchedulerInner>>,
}

impl SchedulerState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SchedulerInner {
                last_incremental: None,
                last_full: None,
            })),
        }
    }

    fn record_scan_done(&self, kind: ScanKind) {
        let now = unix_now();
        let mut inner = self.inner.lock().unwrap();
        match kind {
            ScanKind::Incremental => {
                inner.last_incremental = Some(now);
            }
            ScanKind::Full => {
                inner.last_full = Some(now);
                // 全量扫描也满足增量需求
                inner.last_incremental = Some(now);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanKind {
    Incremental,
    Full,
}

#[derive(Serialize)]
pub struct SchedulerStatus {
    #[serde(rename = "lastIncremental")]
    pub last_incremental: Option<u64>,
    #[serde(rename = "lastFull")]
    pub last_full: Option<u64>,
    #[serde(rename = "nextScheduled")]
    pub next_scheduled: Option<u64>,
    pub enabled: bool,
}

// ─────────────────────────── 设置读取 ───────────────────────────

pub fn read_track_a_settings(store: &SettingsStore) -> TrackASettings {
    let mut out = TrackASettings::default();
    let snap = store.snapshot();
    if let Some(v) = snap.get("closeBehavior").and_then(|v| v.as_str()) {
        out.close_behavior = match v {
            "tray" => CloseBehavior::Tray,
            _ => CloseBehavior::Exit,
        };
    }
    if let Some(v) = snap.get("schedulerEnabled").and_then(|v| v.as_bool()) {
        out.scheduler_enabled = v;
    }
    if let Some(v) = snap.get("schedulerIdleMinutes").and_then(|v| v.as_u64()) {
        // 强制下限 1 分钟，避免误填 0 把后台扫描变成全速空跑
        out.scheduler_idle_minutes = v.max(1);
    }
    out
}

pub fn current_close_behavior(app: &AppHandle) -> CloseBehavior {
    if let Some(store) = app.try_state::<Arc<SettingsStore>>() {
        return read_track_a_settings(store.inner()).close_behavior;
    }
    CloseBehavior::Exit
}

fn default_target_disk(store: &SettingsStore) -> String {
    let snap = store.snapshot();
    snap.get("defaultTargetDisk")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| DEFAULT_SCAN_DISK.to_string())
}

// ─────────────────────────── 后台 tick 任务 ───────────────────────────

pub fn spawn_ticker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // 启动时延迟 30 秒再开始 tick，避免和应用启动期 IO 抢资源
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            tick_once(&app).await;
            tokio::time::sleep(Duration::from_secs(TICK_INTERVAL_SECS)).await;
        }
    });
}

async fn tick_once(app: &AppHandle) {
    let scheduler = match app.try_state::<SchedulerState>() {
        Some(s) => s.inner().clone(),
        None => return,
    };
    let store = match app.try_state::<Arc<SettingsStore>>() {
        Some(s) => s.inner().clone(),
        None => return,
    };

    let settings = read_track_a_settings(&store);
    if !settings.scheduler_enabled {
        return;
    }

    let now = unix_now();
    let (last_inc, last_full) = {
        let inner = scheduler.inner.lock().unwrap();
        (inner.last_incremental, inner.last_full)
    };

    let inc_due = last_inc
        .map(|t| now.saturating_sub(t) >= INCREMENTAL_INTERVAL_SECS)
        .unwrap_or(true); // 从未跑过：立即合格（仍要满足 idle/AC）
    let full_due = last_full
        .map(|t| now.saturating_sub(t) >= FULL_INTERVAL_SECS)
        .unwrap_or(true);

    if !inc_due && !full_due {
        return;
    }

    // 磁盘 idle：距离上次扫描完成 < 30 秒则跳过
    let recent_scan_close = last_inc.max(last_full);
    if let Some(t) = recent_scan_close {
        if now.saturating_sub(t) < DISK_IDLE_SECS {
            return;
        }
    }

    // AC 电源
    if !is_on_ac_power() {
        tracing::debug!("[scheduler] skip: on battery");
        return;
    }

    // 用户 idle
    let idle_secs = get_user_idle_seconds();
    let idle_threshold = settings.scheduler_idle_minutes.saturating_mul(60);
    if idle_secs < idle_threshold {
        tracing::debug!(
            "[scheduler] skip: user idle {}s < threshold {}s",
            idle_secs,
            idle_threshold
        );
        return;
    }

    let kind = if full_due {
        ScanKind::Full
    } else {
        ScanKind::Incremental
    };
    let disk = default_target_disk(&store);

    emit_run_scan(app, &disk, kind, false);
}

fn emit_run_scan(app: &AppHandle, disk: &str, kind: ScanKind, forced: bool) {
    let scan_id = disk.to_string();
    tracing::info!(
        "[scheduler] trigger scan id={} kind={:?} forced={}",
        scan_id,
        kind,
        forced
    );
    // 同时 emit scanId（驼峰，符合 AGENTS 规范）与 scan_id（下划线，给 Track B 监听器用）
    let payload = serde_json::json!({
        "scanId": scan_id,
        "scan_id": scan_id,
        "path": disk,
        "kind": kind,
        "forced": forced,
    });
    if let Err(err) = app.emit("scheduler-run-scan", payload) {
        tracing::warn!("[scheduler] emit scheduler-run-scan failed: {err}");
    }
}

// ─────────────────────────── Tauri 命令 ───────────────────────────

#[tauri::command]
pub fn cmd_scheduler_run_now(
    app: AppHandle,
    path: Option<String>,
    _state: tauri::State<'_, SchedulerState>,
) -> String {
    let disk = match path.filter(|p| !p.is_empty()) {
        Some(p) => p,
        None => {
            // 没指定就用 settings 里 defaultTargetDisk，再退化到 C:\
            app.try_state::<Arc<SettingsStore>>()
                .map(|s| default_target_disk(s.inner()))
                .unwrap_or_else(|| DEFAULT_SCAN_DISK.to_string())
        }
    };
    tracing::info!("[scheduler] run_now disk={disk}");
    emit_run_scan(&app, &disk, ScanKind::Incremental, true);
    disk
}

#[tauri::command]
pub fn cmd_scheduler_get_status(
    app: AppHandle,
    state: tauri::State<'_, SchedulerState>,
) -> SchedulerStatus {
    let inner = state.inner.lock().unwrap();
    let next_scheduled = inner
        .last_incremental
        .map(|t| t + INCREMENTAL_INTERVAL_SECS);
    let enabled = app
        .try_state::<Arc<SettingsStore>>()
        .map(|s| read_track_a_settings(s.inner()).scheduler_enabled)
        .unwrap_or(true);
    SchedulerStatus {
        last_incremental: inner.last_incremental,
        last_full: inner.last_full,
        next_scheduled,
        enabled,
    }
}

#[tauri::command]
pub fn cmd_scheduler_mark_scan_done(
    app: AppHandle,
    scan_id: String,
    kind: Option<ScanKind>,
    state: tauri::State<'_, SchedulerState>,
) {
    let kind = kind.unwrap_or(ScanKind::Incremental);
    state.record_scan_done(kind);
    let payload = serde_json::json!({
        "scanId": scan_id,
        "scan_id": scan_id,
        "kind": kind,
    });
    let _ = app.emit("scheduler-scan-done", payload);
}

// ─────────────────────────── Windows FFI ───────────────────────────
//
// 这里手写 extern 而不是给 windows crate 加 feature，是为了把 Track A 的
// 改动范围严格限制在新文件 + Cargo.toml 末尾追加，避免和 Track B/C 抢
// windows = { ... features = [...] } 这一行造成合并冲突。

#[cfg(target_os = "windows")]
#[repr(C)]
struct LastInputInfo {
    cb_size: u32,
    dw_time: u32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Default)]
struct SystemPowerStatus {
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    system_status_flag: u8,
    battery_life_time: u32,
    battery_full_life_time: u32,
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn GetLastInputInfo(plii: *mut LastInputInfo) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn GetTickCount() -> u32;
    fn GetSystemPowerStatus(lp_system_power_status: *mut SystemPowerStatus) -> i32;
}

#[cfg(target_os = "windows")]
pub fn get_user_idle_seconds() -> u64 {
    unsafe {
        let mut info = LastInputInfo {
            cb_size: std::mem::size_of::<LastInputInfo>() as u32,
            dw_time: 0,
        };
        if GetLastInputInfo(&mut info) == 0 {
            return 0;
        }
        let now = GetTickCount();
        // u32 算术天然回卷（49.7 天周期），用 wrapping_sub 安全
        let elapsed_ms = now.wrapping_sub(info.dw_time);
        (elapsed_ms as u64) / 1000
    }
}

#[cfg(target_os = "windows")]
pub fn is_on_ac_power() -> bool {
    unsafe {
        let mut status = SystemPowerStatus::default();
        if GetSystemPowerStatus(&mut status) == 0 {
            // 调用失败：保守假定不是电池模式，避免桌面无电池的用户被错误拦截
            return true;
        }
        // ACLineStatus == 1 即接通市电；255 表示未知，按通电处理（桌面/无电池设备常见）
        status.ac_line_status == 1 || status.ac_line_status == 255
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_user_idle_seconds() -> u64 {
    u64::MAX
}

#[cfg(not(target_os = "windows"))]
pub fn is_on_ac_power() -> bool {
    true
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
