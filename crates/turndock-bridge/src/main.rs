//! turndock：回合的命令行入口。
//!
//! - `turndock`：在终端里打开回合窗口。
//! - `turndock mcp`：MCP stdio 服务，由 AI 客户端启动；通过 Unix socket 转发到回合应用。
//!
//! 环境变量：
//! - `TURNDOCK_WAIT_SECS`：单次调用最长等待秒数（需小于宿主的工具超时）。
//! - `TURNDOCK_SOCKET` / `TURNDOCK_HOME`：覆盖 socket 与数据目录。
//! - `TURNDOCK_NO_LAUNCH=1`：回合未运行时不尝试启动它。

mod format;
mod ipc;
mod server;

use std::io::IsTerminal;
use std::time::Duration;

use anyhow::Context;
use rmcp::ServiceExt;

const HELP: &str = concat!(
    "turndock ",
    env!("CARGO_PKG_VERSION"),
    "  你和 AI 之间的交接点\n\n",
    "用法：\n",
    "  turndock            打开回合窗口\n",
    "  turndock mcp        MCP 服务（由 AI 客户端启动，不需要手动运行）\n",
    "  turndock --version  显示版本\n\n",
    "接入 AI 客户端：\n",
    "  Claude Code  claude mcp add --scope user turndock -- turndock mcp\n",
    "  Codex        codex mcp add turndock -- turndock mcp\n",
    "  其他客户端   {\"mcpServers\": {\"turndock\": {\"command\": \"turndock\", \"args\": [\"mcp\"]}}}\n",
);

const BUNDLE_ID: &str = "app.turndock.desktop";

enum Mode {
    Version,
    Help,
    Open,
    Mcp,
}

fn mode(args: &[String]) -> Result<Mode, String> {
    match args.first().map(String::as_str) {
        Some("-V" | "--version" | "version") => Ok(Mode::Version),
        Some("-h" | "--help" | "help") => Ok(Mode::Help),
        Some("mcp") => Ok(Mode::Mcp),
        Some("open") => Ok(Mode::Open),
        // 旧配置可能不带参数：被客户端通过管道启动时仍当作 MCP 服务。
        None if std::io::stdin().is_terminal() => Ok(Mode::Open),
        None => Ok(Mode::Mcp),
        Some(other) => Err(format!("不认识的命令：{other}\n\n{HELP}")),
    }
}

/// 交给 macOS 打开回合；已在运行时只是把窗口带到前面。
fn open_app() -> anyhow::Result<()> {
    let status = std::process::Command::new("/usr/bin/open")
        .args(["-b", BUNDLE_ID])
        .status()
        .context("无法运行 open")?;
    if !status.success() {
        anyhow::bail!("没有找到回合应用，请重新运行安装命令");
    }
    Ok(())
}

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

    let args: Vec<String> = std::env::args().skip(1).collect();
    match mode(&args) {
        Ok(Mode::Version) => {
            println!("turndock {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Ok(Mode::Help) => {
            print!("{HELP}");
            return Ok(());
        }
        Ok(Mode::Open) => return open_app(),
        Ok(Mode::Mcp) => {}
        Err(msg) => {
            eprint!("{msg}");
            std::process::exit(2);
        }
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
