//! 本机数据目录与 socket 位置。bridge 与 broker 必须得到相同结果。

use std::path::PathBuf;

/// 数据目录。`TURNDOCK_HOME` 优先；否则 macOS 为 `~/Library/Application Support/TurnDock`，
/// 其他平台为 `$XDG_DATA_HOME/turndock` 或 `~/.local/share/turndock`。
pub fn data_dir() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("TURNDOCK_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(h));
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    if cfg!(target_os = "macos") {
        return Some(home.join("Library/Application Support/TurnDock"));
    }
    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(x) => Some(PathBuf::from(x).join("turndock")),
        None => Some(home.join(".local/share/turndock")),
    }
}

/// broker 监听的 Unix socket。`TURNDOCK_SOCKET` 可覆盖（测试用）。
pub fn socket_path() -> Option<PathBuf> {
    if let Some(s) = std::env::var_os("TURNDOCK_SOCKET").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(s));
    }
    Some(data_dir()?.join("run").join("broker.sock"))
}

pub fn database_path() -> Option<PathBuf> {
    Some(data_dir()?.join("turndock.db"))
}
