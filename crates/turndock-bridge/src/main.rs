//! turndock-mcp：每个 MCP 客户端启动一个，通过 stdio 提供工具，并转发到 TurnDock。
//!
//! 环境变量：
//! - `TURNDOCK_WAIT_SECS`：单次调用最长等待秒数（需小于宿主的工具超时）。
//! - `TURNDOCK_SOCKET` / `TURNDOCK_HOME`：覆盖 socket 与数据目录。
//! - `TURNDOCK_NO_LAUNCH=1`：TurnDock 未运行时不尝试启动它。

mod format;
mod ipc;
mod server;

use std::time::Duration;

use anyhow::Context;
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // stdout 专用于 MCP 协议，日志只能写到 stderr。
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("TURNDOCK_LOG")
                .unwrap_or_else(|_| "warn".into()),
        )
        .init();

    if std::env::args().any(|a| a == "--version" || a == "-V") {
        eprintln!("turndock-mcp {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let socket = turndock_core::paths::socket_path().context("无法确定 TurnDock socket 路径")?;
    let launch = std::env::var_os("TURNDOCK_NO_LAUNCH").is_none();
    let wait = std::env::var("TURNDOCK_WAIT_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&s| s >= 1)
        .map(Duration::from_secs);

    let server = server::Server::new(ipc::Ipc::new(socket, launch), wait);
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .context("MCP 初始化失败")?;
    service.waiting().await?;
    Ok(())
}
