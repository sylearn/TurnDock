//! 回合（TurnDock）桌面外壳：嵌入协调器，提供界面命令、事件、通知、托盘与全局快捷键。
//! 业务状态全部在 broker；这里只做转发与系统集成。

mod prefs;
mod setup;
mod update;

use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tracing::warn;
use turndock_broker::view::{
    BrokerEvent, Draft, RequestDetail, Snapshot, SubmitReply, SubmitStatus,
};
use turndock_broker::{ActionError, BrokerConfig, BrokerHandle};
use turndock_core::{Phase, RequestId};

use prefs::Prefs;

const EVENT: &str = "turndock://event";
const SELECT: &str = "turndock://select";
const PREFS_CHANGED: &str = "turndock://prefs";
const TRAY_ID: &str = "turndock";

struct AppState {
    broker: BrokerHandle,
    prefs: Mutex<Prefs>,
}

fn parse_id(id: &str) -> Result<RequestId, ActionError> {
    RequestId::parse(id).ok_or(ActionError::NotFound)
}

// ---------- 命令 ----------

#[tauri::command]
async fn snapshot(state: State<'_, AppState>) -> Result<Snapshot, ActionError> {
    state.broker.snapshot().await
}

#[tauri::command]
async fn detail(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<RequestDetail>, ActionError> {
    state.broker.detail(parse_id(&id)?).await
}

#[tauri::command]
async fn submit(
    state: State<'_, AppState>,
    reply: SubmitReply,
) -> Result<SubmitStatus, ActionError> {
    state.broker.submit(reply).await
}

#[tauri::command]
async fn dismiss(state: State<'_, AppState>, id: String) -> Result<(), ActionError> {
    state.broker.dismiss(parse_id(&id)?).await
}

#[tauri::command]
async fn save_draft(
    state: State<'_, AppState>,
    id: String,
    draft: Draft,
) -> Result<(), ActionError> {
    state.broker.save_draft(parse_id(&id)?, draft).await
}

#[tauri::command]
fn get_prefs(state: State<'_, AppState>) -> Prefs {
    state.prefs.lock().map(|p| p.clone()).unwrap_or_default()
}

#[tauri::command]
fn set_prefs(app: AppHandle, state: State<'_, AppState>, prefs: Prefs) -> Result<Prefs, String> {
    let prefs = prefs.sanitized();
    let old = state.prefs.lock().map(|p| p.clone()).unwrap_or_default();
    if prefs.shortcut != old.shortcut {
        register_shortcut(&app, &old.shortcut, &prefs.shortcut).map_err(|e| e.to_string())?;
    }
    prefs.save().map_err(|e| format!("保存失败：{e}"))?;
    if let Ok(mut p) = state.prefs.lock() {
        *p = prefs.clone();
    }
    let _ = app.emit(PREFS_CHANGED, &prefs);
    Ok(prefs)
}

// 检测会启动登录 shell、读取配置文件，放到阻塞线程里做。
#[tauri::command]
async fn setup_status() -> Result<setup::Setup, String> {
    tauri::async_runtime::spawn_blocking(setup::build)
        .await
        .map_err(|e| e.to_string())
}

/// 用户点击「接入」后才执行。Cursor 交给它自己的安装确认。
#[tauri::command]
async fn connect_host(app: AppHandle, host: setup::HostId) -> Result<(), String> {
    if host == setup::HostId::Json {
        tauri::async_runtime::spawn_blocking(|| setup::ensure_command(true))
            .await
            .map_err(|e| e.to_string())??;
        return app
            .opener()
            .open_url(setup::cursor_deeplink(), None::<&str>)
            .map_err(|e| format!("无法打开 Cursor：{e}"));
    }
    tauri::async_runtime::spawn_blocking(move || setup::connect(host))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<update::UpdateInfo>, String> {
    update::check(&app).await
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    update::install(&app).await?;
    // 先断开 bridge，它们会在等待预算内自动重连到新版本。
    if let Some(state) = app.try_state::<AppState>() {
        state.broker.shutdown();
    }
    app.restart();
}

/// 只打开 http/https 链接，交给系统默认浏览器。
#[tauri::command]
fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    let lower = url.trim().to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err("只能打开 http/https 链接".into());
    }
    app.opener()
        .open_url(url.trim(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn show_inbox(app: AppHandle, id: Option<String>) {
    open_inbox(&app, id);
}

#[tauri::command]
fn show_quick(app: AppHandle) {
    toggle_quick(&app, true);
}

#[tauri::command]
fn hide_quick(app: AppHandle) {
    if let Some(w) = app.get_webview_window("quick") {
        let _ = w.hide();
    }
}

fn prefs_check_updates(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .prefs
        .lock()
        .map(|p| p.check_updates)
        .unwrap_or(true)
}

// ---------- 窗口 ----------

fn reveal(w: &WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
}

fn open_inbox(app: &AppHandle, id: Option<String>) {
    if let Some(w) = app.get_webview_window("main") {
        reveal(&w);
        if let Some(id) = id {
            let _ = w.emit(SELECT, id);
        }
    }
}

fn toggle_quick(app: &AppHandle, force_show: bool) {
    let Some(w) = app.get_webview_window("quick") else {
        return;
    };
    let visible = w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false);
    if visible && !force_show {
        let _ = w.hide();
    } else {
        reveal(&w);
        let _ = w.emit(SELECT, Option::<String>::None);
    }
}

fn register_shortcut(app: &AppHandle, old: &str, new: &str) -> anyhow::Result<()> {
    let gs = app.global_shortcut();
    if !old.is_empty() {
        let _ = gs.unregister(old);
    }
    if !new.is_empty() {
        gs.register(new)
            .map_err(|e| anyhow::anyhow!("无法注册快捷键 {new}：{e}"))?;
    }
    Ok(())
}

// ---------- 事件转发、通知、托盘 ----------

async fn update_tray(app: &AppHandle, broker: &BrokerHandle) {
    let Ok(snap) = broker.snapshot().await else {
        return;
    };
    let waiting = snap
        .requests
        .iter()
        .filter(|r| r.phase == Phase::Waiting)
        .count();
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let title = if waiting > 0 {
            Some(waiting.to_string())
        } else {
            None
        };
        let _ = tray.set_title(title);
        let _ = tray.set_tooltip(Some(if waiting > 0 {
            format!("回合：{waiting} 条等你回复")
        } else {
            "回合：没有待回复的请求".to_owned()
        }));
    }
}

fn any_window_focused(app: &AppHandle) -> bool {
    app.webview_windows()
        .values()
        .any(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

fn forward_events(app: AppHandle, broker: BrokerHandle) {
    tauri::async_runtime::spawn(async move {
        let mut rx = broker.subscribe();
        loop {
            let ev = match rx.recv().await {
                Ok(ev) => ev,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // 丢失的事件不影响正确性：界面会重新拉取快照。
                    BrokerEvent::Changed { revision: 0 }
                }
                Err(_) => break,
            };
            let _ = app.emit(EVENT, &ev);
            match &ev {
                BrokerEvent::Changed { .. } => update_tray(&app, &broker).await,
                BrokerEvent::NewRequest {
                    id,
                    client_name,
                    project,
                    title,
                } => {
                    let prefs = app
                        .state::<AppState>()
                        .prefs
                        .lock()
                        .map(|p| p.clone())
                        .unwrap_or_default();
                    if prefs.popup {
                        toggle_quick(&app, true);
                    } else if prefs.notify && !any_window_focused(&app) {
                        let heading = match project {
                            Some(p) => format!("{client_name}　{p}"),
                            None => client_name.clone(),
                        };
                        if let Err(e) = app
                            .notification()
                            .builder()
                            .title(heading)
                            .body(title)
                            .show()
                        {
                            warn!(error = %e, request = %id, "通知发送失败");
                        }
                    }
                }
            }
        }
    });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let quick = MenuItem::with_id(app, "quick", "快速回复", true, None::<&str>)?;
    let inbox = MenuItem::with_id(app, "inbox", "打开收件箱", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出回合", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&quick, &inbox, &sep, &quit])?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray@2x.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("回合")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quick" => toggle_quick(app, true),
            "inbox" => open_inbox(app, None),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_quick(tray.app_handle(), false);
            }
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("TURNDOCK_LOG")
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let result = tauri::Builder::default()
        // 必须最先注册：第二个实例只激活已有窗口。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            open_inbox(app, None);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_quick(app, false);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::Pending::default())
        .setup(|app| {
            let config = BrokerConfig::from_env()?;
            let broker = turndock_broker::start_thread(config)?;
            let prefs = Prefs::load();
            if let Err(e) = register_shortcut(app.handle(), "", &prefs.shortcut) {
                warn!(error = %e, "全局快捷键注册失败");
            }
            app.manage(AppState {
                broker: broker.clone(),
                prefs: Mutex::new(prefs),
            });
            if prefs_check_updates(app.handle()) {
                update::check_in_background(app.handle().clone());
            }
            // 拖拽安装时补上终端命令；需要管理员权限时留到用户点「接入」再说。
            std::thread::spawn(|| {
                if let Err(e) = setup::ensure_command(false) {
                    tracing::debug!(error = %e, "未能静默安装 turndock 命令");
                }
            });
            build_tray(app.handle())?;
            forward_events(app.handle().clone(), broker.clone());
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { update_tray(&handle, &broker).await });
            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭窗口只是收起，不改变任何请求状态。
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            detail,
            submit,
            dismiss,
            save_draft,
            get_prefs,
            set_prefs,
            setup_status,
            connect_host,
            check_update,
            install_update,
            open_external,
            show_inbox,
            show_quick,
            hide_quick
        ])
        .build(tauri::generate_context!());

    match result {
        Ok(app) => app.run(|app, event| match event {
            // macOS：点击 Dock 图标时重新显示收件箱。
            tauri::RunEvent::Reopen { .. } => open_inbox(app, None),
            // 退出时断开 bridge；它们会在等待预算内自动重连，请求保留在库中。
            tauri::RunEvent::Exit => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.broker.shutdown();
                }
            }
            _ => {}
        }),
        Err(e) => {
            eprintln!("TurnDock 启动失败：{e}");
            std::process::exit(1);
        }
    }
}
