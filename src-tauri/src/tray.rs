//! v0.1.9 系统托盘（Track A）
//!
//! 只在用户把「关闭主窗口时」设成 `tray` 后才有意义。即使设置是 `exit`，
//! 我们也会建好托盘对象（这样运行期改设置不用重启），但只在 tray 模式下
//! 接管 close 事件、把窗口隐藏到托盘。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

use crate::scheduler::{self, CloseBehavior};

const TRAY_ID: &str = "main-tray";

/// 当托盘菜单的「退出」被点击后置位；close 事件处理读到 true 就放行真正退出。
/// 否则 tray 模式下 app.exit() 触发的 CloseRequested 会被我们自己 prevent_close 抹掉。
static FORCE_QUIT: AtomicBool = AtomicBool::new(false);

fn request_quit(app: &AppHandle) {
    FORCE_QUIT.store(true, Ordering::SeqCst);
    app.exit(0);
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "tray_show", "显示主窗口", true, None::<&str>)?;
    let scan_item = MenuItem::with_id(app, "tray_scan", "立即扫描", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "tray_quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &scan_item, &quit_item])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("CSD · 磁盘工作台")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray_show" => {
                show_main_window(app);
                let _ = app.emit("tray-show-window", ());
            }
            "tray_scan" => {
                // 直接走调度器的 run_now，把 forced 扫描事件 emit 给前端
                let state: tauri::State<'_, scheduler::SchedulerState> = app.state();
                let _ = scheduler::cmd_scheduler_run_now(app.clone(), None, state);
            }
            "tray_quit" => {
                request_quit(app);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // 左键单击：拉回主窗口
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                show_main_window(app);
                let _ = app.emit("tray-show-window", ());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// 把 main window 的 CloseRequested 事件接成「按 closeBehavior 走」。
/// closeBehavior == "tray" 时：拦截关闭、隐藏窗口、保留托盘。
/// closeBehavior == "exit" 时：放行，进程退出。
pub fn install_close_handler(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let app_handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            // 用户从托盘菜单点了「退出」/ 后端主动调用 request_quit：直接放行
            if FORCE_QUIT.load(Ordering::SeqCst) {
                return;
            }
            match scheduler::current_close_behavior(&app_handle) {
                CloseBehavior::Tray => {
                    api.prevent_close();
                    if let Some(win) = app_handle.get_webview_window("main") {
                        let _ = win.hide();
                    }
                }
                CloseBehavior::Exit => {
                    // 默认行为：什么都不做，让 Tauri 关掉窗口、走默认退出流程
                }
            }
        }
    });
}
