//! 本机 IPC 服务：Unix socket，目录 0700、socket 0600，accept 后校验对端 UID。

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};
use tokio_util::codec::{Framed, LinesCodec};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use turndock_core::ipc::{BridgeFrame, BrokerFrame, PROTOCOL_VERSION};
use turndock_core::limits;

use crate::actor::Cmd;

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// 绑定 socket。已有 broker 在监听时返回错误；残留的 socket 文件会被清理。
pub(crate) async fn bind(path: &Path) -> Result<(UnixListener, u32)> {
    let dir = path.parent().context("socket 路径没有父目录")?;
    std::fs::create_dir_all(dir).with_context(|| format!("创建 {}", dir.display()))?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let owner = std::fs::metadata(dir)?.uid();

    if path.exists() {
        if UnixStream::connect(path).await.is_ok() {
            bail!("已有 TurnDock 协调器在运行（{}）", path.display());
        }
        std::fs::remove_file(path)
            .with_context(|| format!("清理残留 socket {}", path.display()))?;
    }
    let listener = UnixListener::bind(path).with_context(|| format!("绑定 {}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok((listener, owner))
}

pub(crate) async fn serve(
    listener: UnixListener,
    owner_uid: u32,
    socket_path: std::path::PathBuf,
    actor: mpsc::UnboundedSender<Cmd>,
    stop: CancellationToken,
) {
    loop {
        let accepted = tokio::select! {
            a = listener.accept() => a,
            _ = stop.cancelled() => {
                let _ = std::fs::remove_file(&socket_path);
                return;
            }
        };
        let stream = match accepted {
            Ok((s, _)) => s,
            Err(e) => {
                warn!(error = %e, "accept 失败");
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        };
        match stream.peer_cred() {
            Ok(cred) if cred.uid() == owner_uid => {}
            Ok(cred) => {
                warn!(uid = cred.uid(), "拒绝其他用户的连接");
                continue;
            }
            Err(e) => {
                warn!(error = %e, "无法读取对端身份，拒绝连接");
                continue;
            }
        }
        let actor = actor.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, actor, stop).await {
                debug!(error = %e, "连接结束");
            }
        });
    }
}

async fn handle_connection(
    stream: UnixStream,
    actor: mpsc::UnboundedSender<Cmd>,
    stop: CancellationToken,
) -> Result<()> {
    let mut framed = Framed::new(stream, LinesCodec::new_with_max_length(limits::FRAME_BYTES));

    let hello = tokio::time::timeout(HELLO_TIMEOUT, framed.next())
        .await
        .context("等待 hello 超时")?
        .context("连接在 hello 前关闭")??;
    let (client, cwd) = match serde_json::from_str::<BridgeFrame>(&hello)? {
        BridgeFrame::Hello {
            protocol,
            client,
            cwd,
            ..
        } if protocol == PROTOCOL_VERSION => (client, cwd),
        BridgeFrame::Hello { protocol, .. } => {
            let reason = format!("协议版本不兼容：bridge {protocol}，broker {PROTOCOL_VERSION}");
            framed
                .send(serde_json::to_string(&BrokerFrame::Reject {
                    reason: reason.clone(),
                })?)
                .await?;
            bail!(reason);
        }
        _ => bail!("第一帧必须是 hello"),
    };

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<BrokerFrame>();
    let (reply, conn_rx) = oneshot::channel();
    actor
        .send(Cmd::Connect {
            client,
            cwd,
            tx: out_tx,
            reply,
        })
        .ok()
        .context("协调器已停止")?;
    let conn = conn_rx.await?;
    framed
        .send(serde_json::to_string(&BrokerFrame::Welcome {
            connection_id: conn.0,
            broker_version: env!("CARGO_PKG_VERSION").into(),
        })?)
        .await?;

    let (mut sink, mut stream) = framed.split();
    let writer = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            let line = match serde_json::to_string(&frame) {
                Ok(l) => l,
                Err(e) => {
                    warn!(error = %e, "序列化帧失败");
                    continue;
                }
            };
            if sink.send(line).await.is_err() {
                break;
            }
        }
    });

    let result = async {
        loop {
            let line = tokio::select! {
                l = stream.next() => match l {
                    Some(l) => l,
                    None => break,
                },
                _ = stop.cancelled() => break,
            };
            let frame: BridgeFrame = serde_json::from_str(&line?).context("无效帧")?;
            actor
                .send(Cmd::Frame { conn, frame })
                .ok()
                .context("协调器已停止")?;
        }
        anyhow::Ok(())
    }
    .await;

    let _ = actor.send(Cmd::Disconnect { conn });
    writer.abort();
    result
}
