//! 回合（TurnDock）协调器。桌面应用把它嵌入进程运行；`turndock-broker` 可执行文件用于无界面开发。

mod actor;
pub mod images;
mod ipc;
pub mod markdown;
mod store;
pub mod view;

use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use turndock_core::RequestId;

pub use actor::ActionError;
use actor::{Actor, Cmd};
use store::Store;
use view::{BrokerEvent, Draft, RequestDetail, Snapshot, SubmitReply, SubmitStatus};

#[derive(Debug, Clone)]
pub struct BrokerConfig {
    /// None 表示使用内存数据库（测试）。
    pub db_path: Option<PathBuf>,
    pub socket_path: PathBuf,
    /// 已结束请求的保留时长。
    pub retention_ms: i64,
    /// 新请求的截止时长。
    pub deadline_ms: i64,
}

impl BrokerConfig {
    /// 默认路径（见 `turndock_core::paths`）。
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            db_path: Some(turndock_core::paths::database_path().context("无法确定数据目录")?),
            socket_path: turndock_core::paths::socket_path().context("无法确定 socket 路径")?,
            retention_ms: 7 * 24 * 60 * 60 * 1000,
            deadline_ms: turndock_core::limits::DEFAULT_DEADLINE_MS,
        })
    }
}

/// 与协调器通信的句柄，可随意克隆。
#[derive(Clone)]
pub struct BrokerHandle {
    tx: mpsc::UnboundedSender<Cmd>,
    events: broadcast::Sender<BrokerEvent>,
    stop: CancellationToken,
}

/// 在当前 tokio 运行时中启动协调器与 IPC 服务。
pub async fn spawn(config: BrokerConfig) -> Result<BrokerHandle> {
    let store = match &config.db_path {
        Some(p) => Store::open(p)?,
        None => Store::open_in_memory()?,
    };
    let (listener, owner) = ipc::bind(&config.socket_path).await?;
    let (tx, rx) = mpsc::unbounded_channel();
    let (events, _) = broadcast::channel(256);
    let actor = Actor::new(
        store,
        config.retention_ms,
        config.deadline_ms,
        events.clone(),
        tx.clone(),
    )?;
    let stop = CancellationToken::new();
    tokio::spawn(actor.run(rx, stop.clone()));
    tokio::spawn(ipc::serve(
        listener,
        owner,
        config.socket_path.clone(),
        tx.clone(),
        stop.clone(),
    ));
    Ok(BrokerHandle { tx, events, stop })
}

/// 在独立线程（单线程运行时）上启动协调器，使数据库写入不占用调用方的运行时。
pub fn start_thread(config: BrokerConfig) -> Result<BrokerHandle> {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("turndock-broker".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(anyhow::Error::from(e)));
                    return;
                }
            };
            rt.block_on(async move {
                let result = spawn(config).await;
                let ok = result.is_ok();
                let _ = ready_tx.send(result);
                if ok {
                    std::future::pending::<()>().await;
                }
            });
        })?;
    ready_rx.recv().context("协调器线程意外退出")?
}

impl BrokerHandle {
    /// 停止接受连接并断开所有 bridge。已持久化的请求在下次启动时恢复。
    pub fn shutdown(&self) {
        self.stop.cancel();
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BrokerEvent> {
        self.events.subscribe()
    }

    async fn ask<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> Cmd) -> Result<T, ActionError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(make(tx)).map_err(|_| ActionError::Stopped)?;
        rx.await.map_err(|_| ActionError::Stopped)
    }

    pub async fn snapshot(&self) -> Result<Snapshot, ActionError> {
        self.ask(|reply| Cmd::Snapshot { reply }).await
    }

    pub async fn detail(&self, id: RequestId) -> Result<Option<RequestDetail>, ActionError> {
        self.ask(|reply| Cmd::Detail { id, reply }).await
    }

    pub async fn submit(&self, reply: SubmitReply) -> Result<SubmitStatus, ActionError> {
        self.ask(|done| Cmd::Submit { reply, done }).await?
    }

    pub async fn dismiss(&self, id: RequestId) -> Result<(), ActionError> {
        self.ask(|done| Cmd::Dismiss { id, done }).await?
    }

    pub async fn save_draft(&self, id: RequestId, draft: Draft) -> Result<(), ActionError> {
        self.ask(|done| Cmd::SaveDraft { id, draft, done }).await?
    }
}
