//! 应用更新：从 GitHub Releases 的 latest.json 检查新版本，下载后用内置公钥验证签名再安装。
//! `turndock` 命令在应用包里，随应用一起更新；命令链接指向包内路径，不用重装。

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

pub const PROGRESS: &str = "turndock://update-progress";
pub const AVAILABLE: &str = "turndock://update-available";

#[derive(Default)]
pub struct Pending(pub Mutex<Option<Update>>);

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

fn friendly(e: tauri_plugin_updater::Error) -> String {
    let text = e.to_string();
    if text.contains("404") || text.to_ascii_lowercase().contains("not found") {
        "还没有可用的发布版本".into()
    } else if text.to_ascii_lowercase().contains("signature") {
        format!("更新包签名校验失败，已放弃安装：{text}")
    } else {
        format!("检查更新失败：{text}")
    }
}

/// 检查新版本。有新版本时记下它，供 `install` 使用。
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(friendly)?
        .check()
        .await
        .map_err(friendly)?;
    let info = update.as_ref().map(|u| UpdateInfo {
        current: u.current_version.clone(),
        version: u.version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    });
    if let Some(state) = app.try_state::<Pending>()
        && let Ok(mut slot) = state.0.lock()
    {
        *slot = update;
    }
    Ok(info)
}

/// 下载、验签、替换应用包，然后重启。
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let update = app
        .try_state::<Pending>()
        .and_then(|s| s.0.lock().ok().and_then(|mut slot| slot.take()))
        .ok_or_else(|| "请先检查更新".to_owned())?;
    let mut downloaded = 0u64;
    let handle = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = handle.emit(PROGRESS, Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(friendly)?;
    Ok(())
}

/// 启动后静默检查一次；失败不打扰用户。
pub fn check_in_background(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
        match check(&app).await {
            Ok(Some(info)) => {
                let _ = app.emit(AVAILABLE, info);
            }
            Ok(None) => {}
            Err(e) => tracing::debug!(error = %e, "后台检查更新失败"),
        }
    });
}
