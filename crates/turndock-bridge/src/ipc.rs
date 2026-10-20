//! 到 broker 的 IPC 客户端。懒连接：第一次工具调用时才连接，断开后下次调用重连。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use tokio::net::UnixStream;
use tokio::sync::{Mutex, mpsc};
use tokio_util::codec::{Framed, LinesCodec};
use tracing::{debug, info};
use turndock_core::ipc::{BridgeFrame, BrokerFrame, ClientInfo, PROTOCOL_VERSION};
use turndock_core::limits;

/// macOS 上 TurnDock.app 的 bundle ID。
pub const BUNDLE_ID: &str = "app.turndock.desktop";
const LAUNCH_WAIT: Duration = Duration::from_secs(8);

type Pending = Arc<std::sync::Mutex<HashMap<u64, mpsc::UnboundedSender<BrokerFrame>>>>;

pub struct Connection {
    out: mpsc::UnboundedSender<BridgeFrame>,
    pending: Pending,
    next_call: AtomicU64,
    closed: tokio_util::sync::CancellationToken,
}

impl Connection {
    /// 登记一次调用，返回调用编号与接收该调用帧的通道。
    pub fn register(&self) -> (u64, mpsc::UnboundedReceiver<BrokerFrame>) {
        let call = self.next_call.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(mut p) = self.pending.lock() {
            p.insert(call, tx);
        }
        (call, rx)
    }

    pub fn finish(&self, call: u64) {
        if let Ok(mut p) = self.pending.lock() {
            p.remove(&call);
        }
    }

    pub fn send(&self, frame: BridgeFrame) -> Result<()> {
        self.out
            .send(frame)
            .ok()
            .context("与 TurnDock 的连接已断开")
    }

    fn is_closed(&self) -> bool {
        self.closed.is_cancelled()
    }
}

pub struct Ipc {
    socket: PathBuf,
    launch: bool,
    current: Mutex<Option<Arc<Connection>>>,
}

impl Ipc {
    pub fn new(socket: PathBuf, launch: bool) -> Self {
        Self {
            socket,
            launch,
            current: Mutex::new(None),
        }
    }

    pub async fn connection(&self, client: &ClientInfo) -> Result<Arc<Connection>> {
        let mut cur = self.current.lock().await;
        if let Some(c) = cur.as_ref()
            && !c.is_closed()
        {
            return Ok(c.clone());
        }
        let conn = Arc::new(self.connect(client).await?);
        *cur = Some(conn.clone());
        Ok(conn)
    }

    async fn connect(&self, client: &ClientInfo) -> Result<Connection> {
        let stream = match UnixStream::connect(&self.socket).await {
            Ok(s) => s,
            Err(first) if self.launch => {
                debug!(error = %first, "broker 未运行，尝试启动 TurnDock");
                launch_app()?;
                self.connect_with_retry().await?
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("TurnDock 未运行（{}）", self.socket.display()));
            }
        };
        let mut framed = Framed::new(stream, LinesCodec::new_with_max_length(limits::FRAME_BYTES));
        let hello = BridgeFrame::Hello {
            protocol: PROTOCOL_VERSION,
            bridge_version: env!("CARGO_PKG_VERSION").into(),
            client: client.clone(),
            cwd: std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string()),
            pid: Some(std::process::id()),
        };
        framed.send(serde_json::to_string(&hello)?).await?;
        let line = tokio::time::timeout(Duration::from_secs(5), framed.next())
            .await
            .context("TurnDock 握手超时")?
            .context("TurnDock 在握手时断开")??;
        match serde_json::from_str::<BrokerFrame>(&line)? {
            BrokerFrame::Welcome { connection_id, .. } => {
                info!(connection_id, "已连接 TurnDock");
            }
            BrokerFrame::Reject { reason } => bail!("TurnDock 拒绝连接：{reason}"),
            other => bail!("握手收到意外帧：{other:?}"),
        }

        let (mut sink, mut stream) = framed.split();
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<BridgeFrame>();
        let pending: Pending = Arc::default();
        let closed = tokio_util::sync::CancellationToken::new();

        let writer_closed = closed.clone();
        tokio::spawn(async move {
            while let Some(f) = out_rx.recv().await {
                let Ok(line) = serde_json::to_string(&f) else {
                    continue;
                };
                if sink.send(line).await.is_err() {
                    break;
                }
            }
            writer_closed.cancel();
        });

        let reader_pending = pending.clone();
        let reader_closed = closed.clone();
        tokio::spawn(async move {
            while let Some(Ok(line)) = stream.next().await {
                let Ok(frame) = serde_json::from_str::<BrokerFrame>(&line) else {
                    continue;
                };
                let call = match &frame {
                    BrokerFrame::Created { call, .. }
                    | BrokerFrame::Outcome { call, .. }
                    | BrokerFrame::Error { call, .. } => *call,
                    _ => continue,
                };
                let tx = reader_pending
                    .lock()
                    .ok()
                    .and_then(|p| p.get(&call).cloned());
                if let Some(tx) = tx {
                    let _ = tx.send(frame);
                }
            }
            reader_closed.cancel();
            // 关闭所有等待中的调用通道，使它们得知连接已断开。
            if let Ok(mut p) = reader_pending.lock() {
                p.clear();
            }
        });

        Ok(Connection {
            out: out_tx,
            pending,
            next_call: AtomicU64::new(1),
            closed,
        })
    }

    async fn connect_with_retry(&self) -> Result<UnixStream> {
        let deadline = tokio::time::Instant::now() + LAUNCH_WAIT;
        loop {
            match UnixStream::connect(&self.socket).await {
                Ok(s) => return Ok(s),
                Err(e) if tokio::time::Instant::now() >= deadline => {
                    return Err(e).context(
                        "已尝试启动 TurnDock，但 8 秒内没有就绪。请确认已安装并打开 TurnDock",
                    );
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
            }
        }
    }
}

/// 通过 bundle ID 让系统启动已安装的 TurnDock。不执行任何调用方提供的路径。
fn launch_app() -> Result<()> {
    if cfg!(target_os = "macos") {
        let status = std::process::Command::new("/usr/bin/open")
            .args(["-g", "-b", BUNDLE_ID])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("无法调用 open")?;
        if !status.success() {
            bail!("没有找到已安装的 TurnDock（{BUNDLE_ID}）");
        }
        Ok(())
    } else {
        bail!("TurnDock 未运行，请先打开 TurnDock")
    }
}
