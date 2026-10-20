//! 协调器 actor：请求状态的唯一所有者。所有输入（IPC、界面、计时器）都变成 [`Cmd`]
//! 按顺序处理，竞态由处理顺序裁决。每个阶段/交付变化先写库，成功后才生效并通知外部。

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, info, warn};
use turndock_core::ipc::{BridgeFrame, BrokerFrame, ClientInfo, ErrorCode};
use turndock_core::state::{AttachEffect, CancelEffect, DecideEffect, StateError};
use turndock_core::{
    AskSpec, CallOutcome, CallRef, CallResult, ConnectionId, ImagePayload, MessageFormat, Outcome,
    Phase, ReplyPayload, RequestId, RequestState, Selection, limits,
};

use crate::store::{RequestRow, Store, StoredDecision};
use crate::view::{
    BrokerEvent, DecisionView, Draft, RequestDetail, RequestSummary, Snapshot, SubmitReply,
    SubmitStatus,
};
use crate::{images, markdown};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum ActionError {
    #[error("找不到这条请求")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    State(String),
    #[error("保存失败：{0}")]
    Storage(String),
    #[error("协调器已停止")]
    Stopped,
}

impl From<StateError> for ActionError {
    fn from(e: StateError) -> Self {
        ActionError::State(e.to_string())
    }
}

pub(crate) enum Cmd {
    Connect {
        client: ClientInfo,
        cwd: Option<String>,
        tx: mpsc::UnboundedSender<BrokerFrame>,
        reply: oneshot::Sender<ConnectionId>,
    },
    Frame {
        conn: ConnectionId,
        frame: BridgeFrame,
    },
    Disconnect {
        conn: ConnectionId,
    },
    WaitElapsed {
        call: CallRef,
        id: RequestId,
    },
    Expire {
        id: RequestId,
    },
    Snapshot {
        reply: oneshot::Sender<Snapshot>,
    },
    Detail {
        id: RequestId,
        reply: oneshot::Sender<Option<RequestDetail>>,
    },
    Submit {
        reply: SubmitReply,
        done: oneshot::Sender<Result<SubmitStatus, ActionError>>,
    },
    Dismiss {
        id: RequestId,
        done: oneshot::Sender<Result<(), ActionError>>,
    },
    SaveDraft {
        id: RequestId,
        draft: Draft,
        done: oneshot::Sender<Result<(), ActionError>>,
    },
}

struct Conn {
    client: ClientInfo,
    project_path: Option<String>,
    tx: mpsc::UnboundedSender<BrokerFrame>,
    calls: HashMap<u64, RequestId>,
}

pub(crate) struct Actor {
    store: Store,
    requests: HashMap<RequestId, RequestRow>,
    drafts: HashSet<RequestId>,
    conns: HashMap<ConnectionId, Conn>,
    next_conn: u64,
    revision: u64,
    deadline_ms: i64,
    events: broadcast::Sender<BrokerEvent>,
    self_tx: mpsc::UnboundedSender<Cmd>,
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Actor {
    pub(crate) fn new(
        store: Store,
        retention_ms: i64,
        deadline_ms: i64,
        events: broadcast::Sender<BrokerEvent>,
        self_tx: mpsc::UnboundedSender<Cmd>,
    ) -> anyhow::Result<Self> {
        let now = now_ms();
        let purged = store.purge_finished_before(now - retention_ms)?;
        if purged > 0 {
            info!(purged, "清理保留期外的请求");
        }
        let mut requests = HashMap::new();
        for mut row in store.load_recent(now - retention_ms)? {
            let before = row.state.clone();
            row.state.restore_after_restart();
            if row.state != before {
                store.update_state(&row.id, &row.state, now)?;
            }
            requests.insert(row.id.clone(), row);
        }
        let drafts = store.draft_ids()?;
        let actor = Self {
            store,
            requests,
            drafts,
            conns: HashMap::new(),
            next_conn: 1,
            revision: 1,
            deadline_ms,
            events,
            self_tx,
        };
        let waiting: Vec<_> = actor
            .requests
            .values()
            .filter(|r| r.state.phase == Phase::Waiting)
            .map(|r| (r.id.clone(), r.deadline_at))
            .collect();
        for (id, deadline) in waiting {
            actor.schedule_expiry(id, deadline);
        }
        Ok(actor)
    }

    pub(crate) async fn run(
        mut self,
        mut rx: mpsc::UnboundedReceiver<Cmd>,
        stop: tokio_util::sync::CancellationToken,
    ) {
        loop {
            tokio::select! {
                cmd = rx.recv() => match cmd {
                    Some(cmd) => self.handle(cmd),
                    None => break,
                },
                _ = stop.cancelled() => break,
            }
        }
        debug!("协调器退出");
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Connect {
                client,
                cwd,
                tx,
                reply,
            } => {
                let id = ConnectionId(self.next_conn);
                self.next_conn += 1;
                info!(conn = id.0, client = %client.name, "连接建立");
                self.conns.insert(
                    id,
                    Conn {
                        client,
                        project_path: cwd,
                        tx,
                        calls: HashMap::new(),
                    },
                );
                let _ = reply.send(id);
            }
            Cmd::Frame { conn, frame } => self.on_frame(conn, frame),
            Cmd::Disconnect { conn } => self.on_disconnect(conn),
            Cmd::WaitElapsed { call, id } => self.on_wait_elapsed(call, id),
            Cmd::Expire { id } => self.on_expire(id),
            Cmd::Snapshot { reply } => {
                let _ = reply.send(self.snapshot());
            }
            Cmd::Detail { id, reply } => {
                let _ = reply.send(self.detail(&id));
            }
            Cmd::Submit { reply, done } => {
                let _ = done.send(self.submit(reply));
            }
            Cmd::Dismiss { id, done } => {
                let _ = done.send(self.dismiss(&id));
            }
            Cmd::SaveDraft { id, draft, done } => {
                let _ = done.send(self.save_draft(&id, draft));
            }
        }
    }

    // ---------- IPC ----------

    fn on_frame(&mut self, conn: ConnectionId, frame: BridgeFrame) {
        match frame {
            BridgeFrame::Hello { .. } => {
                self.send_error(conn, 0, ErrorCode::Protocol, "重复的 hello".into());
            }
            BridgeFrame::Ask {
                call,
                spec,
                wait_ms,
            } => self.on_ask(conn, call, spec, wait_ms),
            BridgeFrame::Await {
                call,
                request_id,
                wait_ms,
            } => self.on_await(conn, call, request_id, wait_ms),
            BridgeFrame::Cancel { call } => self.on_cancel(conn, call),
            BridgeFrame::Ack { call } => self.on_ack(conn, call),
        }
    }

    fn on_ask(&mut self, conn: ConnectionId, call: u64, spec: AskSpec, wait_ms: u64) {
        if let Err(e) = spec.validate() {
            return self.send_error(conn, call, ErrorCode::InvalidRequest, e.to_string());
        }
        let Some(c) = self.conns.get(&conn) else {
            return;
        };
        if c.calls.contains_key(&call) {
            return self.send_error(conn, call, ErrorCode::Protocol, "call 编号重复".into());
        }
        if c.calls.len() >= limits::PER_CONNECTION_OPEN {
            return self.send_error(
                conn,
                call,
                ErrorCode::Capacity,
                format!("此连接已有 {} 条请求在等待", limits::PER_CONNECTION_OPEN),
            );
        }
        let open = self
            .requests
            .values()
            .filter(|r| r.state.phase == Phase::Waiting)
            .count();
        if open >= limits::GLOBAL_OPEN {
            return self.send_error(conn, call, ErrorCode::Capacity, "待回复请求已满".into());
        }

        let now = now_ms();
        let cref = CallRef {
            connection: conn,
            call,
        };
        let row = RequestRow {
            id: RequestId::new(),
            client_name: c.client.name.clone(),
            client_version: c.client.version.clone(),
            project_path: c.project_path.clone(),
            spec,
            state: RequestState::new_attached(cref),
            decision: None,
            created_at: now,
            updated_at: now,
            deadline_at: now + self.deadline_ms,
        };
        if let Err(e) = self.store.insert_request(&row) {
            warn!(error = %e, "写入请求失败");
            return self.send_error(conn, call, ErrorCode::Storage, "无法保存请求".into());
        }
        let id = row.id.clone();
        info!(request = %id, conn = conn.0, call, "新请求");
        let event = BrokerEvent::NewRequest {
            id: id.clone(),
            client_name: row.client_name.clone(),
            project: project_name(row.project_path.as_deref()),
            title: title_of(&row.spec),
        };
        let deadline = row.deadline_at;
        self.requests.insert(id.clone(), row);
        if let Some(c) = self.conns.get_mut(&conn) {
            c.calls.insert(call, id.clone());
        }
        self.send(
            conn,
            BrokerFrame::Created {
                call,
                request_id: id.clone(),
            },
        );
        self.schedule_wait(cref, id.clone(), wait_ms);
        self.schedule_expiry(id, deadline);
        let _ = self.events.send(event);
        self.bump();
    }

    fn on_await(&mut self, conn: ConnectionId, call: u64, id: RequestId, wait_ms: u64) {
        let Some(c) = self.conns.get(&conn) else {
            return;
        };
        if c.calls.contains_key(&call) {
            return self.send_error(conn, call, ErrorCode::Protocol, "call 编号重复".into());
        }
        let cref = CallRef {
            connection: conn,
            call,
        };
        let Some(row) = self.requests.get(&id) else {
            return self.send_error(
                conn,
                call,
                ErrorCode::NotFound,
                format!("找不到请求 {id}（可能已超过保留期）"),
            );
        };
        let mut next = row.state.clone();
        match next.attach(cref) {
            Err(StateError::AlreadyAttached) => self.send_error(
                conn,
                call,
                ErrorCode::AlreadyWaiting,
                "已有另一个调用在等待这条请求".into(),
            ),
            Err(e) => self.send_error(conn, call, ErrorCode::InvalidRequest, e.to_string()),
            Ok(AttachEffect::Closed(phase)) => {
                let tool = row.spec.tool;
                self.send_outcome(conn, call, &id, tool, closed_result(phase));
            }
            Ok(AttachEffect::Wait) => {
                if self.commit(&id, next).is_ok() {
                    if let Some(c) = self.conns.get_mut(&conn) {
                        c.calls.insert(call, id.clone());
                    }
                    self.schedule_wait(cref, id, wait_ms);
                    self.bump();
                } else {
                    self.send_error(conn, call, ErrorCode::Storage, "无法更新请求".into());
                }
            }
            Ok(AttachEffect::DeliverNow) => {
                if self.commit(&id, next).is_ok() {
                    if let Some(c) = self.conns.get_mut(&conn) {
                        c.calls.insert(call, id.clone());
                    }
                    self.deliver(cref, &id);
                    self.bump();
                } else {
                    self.send_error(conn, call, ErrorCode::Storage, "无法更新请求".into());
                }
            }
        }
    }

    fn on_cancel(&mut self, conn: ConnectionId, call: u64) {
        let Some(id) = self
            .conns
            .get(&conn)
            .and_then(|c| c.calls.get(&call).cloned())
        else {
            return;
        };
        let cref = CallRef {
            connection: conn,
            call,
        };
        let Some(row) = self.requests.get(&id) else {
            return;
        };
        let mut next = row.state.clone();
        match next.cancel(cref) {
            CancelEffect::Cancelled => {
                if self.commit(&id, next).is_ok() {
                    info!(request = %id, "调用方取消");
                    self.forget_call(cref);
                    self.bump();
                }
            }
            CancelEffect::TooLate => info!(request = %id, "取消到达时回复已在途"),
            CancelEffect::Ignored => {}
        }
    }

    fn on_ack(&mut self, conn: ConnectionId, call: u64) {
        let Some(id) = self
            .conns
            .get(&conn)
            .and_then(|c| c.calls.get(&call).cloned())
        else {
            return;
        };
        let cref = CallRef {
            connection: conn,
            call,
        };
        if let Some(row) = self.requests.get(&id) {
            let mut next = row.state.clone();
            if next.ack(cref) && self.commit(&id, next).is_ok() {
                self.bump();
            }
        }
        self.forget_call(cref);
    }

    fn on_wait_elapsed(&mut self, call: CallRef, id: RequestId) {
        let Some(row) = self.requests.get(&id) else {
            return;
        };
        let tool = row.spec.tool;
        let mut next = row.state.clone();
        if next.wait_elapsed(call) {
            // 只解除挂接，阶段与交付不变，无需写库。
            let _ = self.commit(&id, next);
            self.forget_call(call);
            self.send_outcome(call.connection, call.call, &id, tool, CallResult::Pending);
            self.bump();
        }
    }

    fn on_disconnect(&mut self, conn: ConnectionId) {
        let Some(c) = self.conns.remove(&conn) else {
            return;
        };
        info!(conn = conn.0, "连接断开");
        let mut changed = false;
        for id in c.calls.values() {
            if let Some(row) = self.requests.get(id) {
                let mut next = row.state.clone();
                if next.connection_lost(conn) {
                    changed |= self.commit(id, next).is_ok();
                }
            }
        }
        if changed {
            self.bump();
        }
    }

    fn on_expire(&mut self, id: RequestId) {
        let Some(row) = self.requests.get(&id) else {
            return;
        };
        if row.deadline_at > now_ms() {
            // 计时器提前触发（例如系统时间被调整），重新安排。
            return self.schedule_expiry(id, row.deadline_at);
        }
        let tool = row.spec.tool;
        let mut next = row.state.clone();
        if let Some(attached) = next.expire()
            && self.commit(&id, next).is_ok()
        {
            info!(request = %id, "请求过期");
            if let Some(call) = attached {
                self.forget_call(call);
                self.send_outcome(call.connection, call.call, &id, tool, CallResult::Expired);
            }
            self.bump();
        }
    }

    // ---------- 界面动作 ----------

    fn submit(&mut self, r: SubmitReply) -> Result<SubmitStatus, ActionError> {
        let row = self
            .requests
            .get(&r.request_id)
            .ok_or(ActionError::NotFound)?;
        validate_reply(&row.spec, &r)?;
        let valid_images =
            images::validate(&r.images).map_err(|e| ActionError::Invalid(e.to_string()))?;

        let mut next = row.state.clone();
        let effect = next.decide(r.submission_id.clone())?;
        if effect == DecideEffect::Duplicate {
            return Ok(SubmitStatus::Duplicate);
        }
        let decision = StoredDecision {
            outcome: r.outcome,
            selected_ids: r.selected_ids.clone(),
            text: r.text.trim_end().to_owned(),
            image_count: valid_images.len() as u32,
            decided_at: now_ms(),
        };
        let image_refs: Vec<(&str, &[u8])> = valid_images
            .iter()
            .map(|i| (i.mime, i.bytes.as_slice()))
            .collect();
        self.store
            .record_decision(
                &r.request_id,
                &next,
                &r.submission_id,
                &decision,
                &image_refs,
            )
            .map_err(|e| ActionError::Storage(e.to_string()))?;

        let id = r.request_id;
        if let Some(row) = self.requests.get_mut(&id) {
            row.state = next;
            row.decision = Some(decision);
            row.updated_at = now_ms();
        }
        self.drafts.remove(&id);
        info!(request = %id, ?effect, "用户已决定");
        let status = match effect {
            DecideEffect::Deliver(call) => {
                self.deliver(call, &id);
                SubmitStatus::Delivered
            }
            _ => SubmitStatus::Stored,
        };
        self.bump();
        Ok(status)
    }

    fn dismiss(&mut self, id: &RequestId) -> Result<(), ActionError> {
        let row = self.requests.get(id).ok_or(ActionError::NotFound)?;
        let tool = row.spec.tool;
        let mut next = row.state.clone();
        let attached = next.dismiss()?;
        self.commit(id, next)
            .map_err(|e| ActionError::Storage(e.to_string()))?;
        if let Some(call) = attached {
            self.forget_call(call);
            self.send_outcome(call.connection, call.call, id, tool, CallResult::Dismissed);
        }
        self.bump();
        Ok(())
    }

    fn save_draft(&mut self, id: &RequestId, draft: Draft) -> Result<(), ActionError> {
        let row = self.requests.get(id).ok_or(ActionError::NotFound)?;
        if row.state.phase != Phase::Waiting {
            return Err(ActionError::State("请求已结束，草稿不再保存".into()));
        }
        if draft.text.len() > limits::REPLY_TEXT_BYTES {
            return Err(ActionError::Invalid("草稿过长".into()));
        }
        let empty = draft.text.is_empty() && draft.selected_ids.is_empty();
        self.store
            .save_draft(id, &draft, now_ms())
            .map_err(|e| ActionError::Storage(e.to_string()))?;
        let changed = if empty {
            self.drafts.remove(id)
        } else {
            self.drafts.insert(id.clone())
        };
        if changed {
            self.bump();
        }
        Ok(())
    }

    fn snapshot(&self) -> Snapshot {
        let mut requests: Vec<_> = self.requests.values().map(|r| self.summary(r)).collect();
        requests.sort_by_key(|r| std::cmp::Reverse(r.created_at));
        Snapshot {
            revision: self.revision,
            requests,
        }
    }

    fn detail(&self, id: &RequestId) -> Option<RequestDetail> {
        let row = self.requests.get(id)?;
        let draft = if self.drafts.contains(id) {
            self.store.load_draft(id).unwrap_or_else(|e| {
                warn!(error = %e, "读取草稿失败");
                None
            })
        } else {
            None
        };
        let (heading, body) = row.spec.heading_and_body();
        let markdown_heading =
            row.spec.title.is_none() && row.spec.format == MessageFormat::Markdown;
        // 标题按 Markdown 渲染后取纯文本；若只剩空白（例如整行是 HTML），则不另起标题。
        let (heading, body) = match heading {
            Some(h) if markdown_heading => {
                let plain = markdown::plain_text(&h);
                if plain.is_empty() {
                    (None, row.spec.message.clone())
                } else {
                    (Some(plain), body)
                }
            }
            other => (other, body),
        };
        Some(RequestDetail {
            summary: self.summary(row),
            heading,
            message_html: markdown::render(&body, row.spec.format),
            message_text: row.spec.message.clone(),
            format: row.spec.format,
            options: row.spec.options.clone(),
            selection: row.spec.selection,
            allow_text: row.spec.allow_text,
            allow_images: row.spec.allow_images,
            decision: row.decision.as_ref().map(|d| DecisionView {
                outcome: d.outcome,
                selected_ids: d.selected_ids.clone(),
                text: d.text.clone(),
                image_count: d.image_count,
                decided_at: d.decided_at,
            }),
            draft,
        })
    }

    fn summary(&self, r: &RequestRow) -> RequestSummary {
        RequestSummary {
            id: r.id.clone(),
            client_name: r.client_name.clone(),
            client_version: r.client_version.clone(),
            project: project_name(r.project_path.as_deref()),
            project_path: r.project_path.clone(),
            title: title_of(&r.spec),
            tool: r.spec.tool,
            phase: r.state.phase,
            delivery: r.state.delivery,
            attached: r.state.attached.is_some(),
            outcome: r.decision.as_ref().map(|d| d.outcome),
            has_draft: self.drafts.contains(&r.id),
            created_at: r.created_at,
            updated_at: r.updated_at,
            deadline_at: r.deadline_at,
        }
    }

    // ---------- 内部 ----------

    /// 把新状态写库（仅当阶段或交付变化时）并更新内存。
    fn commit(&mut self, id: &RequestId, next: RequestState) -> anyhow::Result<()> {
        let row = self
            .requests
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("请求不存在"))?;
        if row.state.phase != next.phase || row.state.delivery != next.delivery {
            let now = now_ms();
            if let Err(e) = self.store.update_state(id, &next, now) {
                warn!(request = %id, error = %e, "状态写入失败");
                return Err(e);
            }
            row.updated_at = now;
        }
        row.state = next;
        Ok(())
    }

    fn deliver(&mut self, call: CallRef, id: &RequestId) {
        let Some(row) = self.requests.get(id) else {
            return;
        };
        let Some(d) = &row.decision else {
            warn!(request = %id, "交付时缺少决定");
            return;
        };
        let images = match self.store.load_images(id) {
            Ok(list) => list
                .into_iter()
                .map(|(mime, data)| ImagePayload {
                    mime,
                    data: images::encode(&data),
                })
                .collect(),
            Err(e) => {
                warn!(request = %id, error = %e, "读取图片失败");
                Vec::new()
            }
        };
        let labels = d
            .selected_ids
            .iter()
            .filter_map(|sid| row.spec.options.iter().find(|o| &o.id == sid))
            .map(|o| o.label.clone())
            .collect();
        let payload = ReplyPayload {
            outcome: d.outcome,
            selected_ids: d.selected_ids.clone(),
            selected_labels: labels,
            text: d.text.clone(),
            images,
        };
        let tool = row.spec.tool;
        self.send_outcome(
            call.connection,
            call.call,
            id,
            tool,
            CallResult::Reply(payload),
        );
    }

    fn forget_call(&mut self, call: CallRef) {
        if let Some(c) = self.conns.get_mut(&call.connection) {
            c.calls.remove(&call.call);
        }
    }

    fn send(&self, conn: ConnectionId, frame: BrokerFrame) {
        if let Some(c) = self.conns.get(&conn) {
            let _ = c.tx.send(frame);
        }
    }

    fn send_outcome(
        &self,
        conn: ConnectionId,
        call: u64,
        id: &RequestId,
        tool: turndock_core::ToolKind,
        result: CallResult,
    ) {
        self.send(
            conn,
            BrokerFrame::Outcome {
                call,
                request_id: id.clone(),
                outcome: CallOutcome { tool, result },
            },
        );
    }

    fn send_error(&self, conn: ConnectionId, call: u64, code: ErrorCode, message: String) {
        self.send(
            conn,
            BrokerFrame::Error {
                call,
                code,
                message,
            },
        );
    }

    fn schedule_wait(&self, call: CallRef, id: RequestId, wait_ms: u64) {
        let wait = Duration::from_millis(wait_ms.clamp(1_000, limits::DEFAULT_DEADLINE_MS as u64));
        let tx = self.self_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            let _ = tx.send(Cmd::WaitElapsed { call, id });
        });
    }

    fn schedule_expiry(&self, id: RequestId, deadline_at: i64) {
        let left = Duration::from_millis((deadline_at - now_ms()).max(0) as u64);
        let tx = self.self_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(left).await;
            let _ = tx.send(Cmd::Expire { id });
        });
    }

    fn bump(&mut self) {
        self.revision += 1;
        let _ = self.events.send(BrokerEvent::Changed {
            revision: self.revision,
        });
    }
}

/// 列表标题：Markdown 消息去掉格式与 HTML 后的纯文本。
fn title_of(spec: &AskSpec) -> String {
    let raw = spec.display_title();
    if spec.title.is_some() || spec.format != MessageFormat::Markdown {
        return raw;
    }
    let plain = markdown::plain_text(&raw);
    if plain.is_empty() {
        raw
    } else {
        turndock_core::spec::truncate_chars(&plain, 80)
    }
}

fn closed_result(phase: Phase) -> CallResult {
    match phase {
        Phase::Cancelled => CallResult::Cancelled,
        Phase::Expired => CallResult::Expired,
        _ => CallResult::Dismissed,
    }
}

fn project_name(path: Option<&str>) -> Option<String> {
    let p = std::path::Path::new(path?);
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .or_else(|| Some(p.display().to_string()))
}

fn validate_reply(spec: &AskSpec, r: &SubmitReply) -> Result<(), ActionError> {
    let invalid = |m: &str| Err(ActionError::Invalid(m.to_owned()));
    if r.text.len() > limits::REPLY_TEXT_BYTES {
        return invalid("回复文字过长");
    }
    match r.outcome {
        Outcome::Declined => {
            if !r.selected_ids.is_empty() || !r.images.is_empty() {
                return invalid("拒绝时只能附带文字理由");
            }
        }
        Outcome::Answered => {
            let unique: HashSet<_> = r.selected_ids.iter().collect();
            if unique.len() != r.selected_ids.len() {
                return invalid("选项重复");
            }
            if r.selected_ids
                .iter()
                .any(|id| !spec.options.iter().any(|o| &o.id == id))
            {
                return invalid("选择了不存在的选项");
            }
            match spec.selection {
                Selection::None if !r.selected_ids.is_empty() => {
                    return invalid("这条请求没有选项");
                }
                Selection::Single if r.selected_ids.len() > 1 => return invalid("只能选择一项"),
                _ => {}
            }
            if !spec.allow_text && !r.text.trim().is_empty() {
                return invalid("这条请求不接受文字回复");
            }
            if !spec.allow_images && !r.images.is_empty() {
                return invalid("这条请求不接受图片");
            }
            if r.selected_ids.is_empty() && r.text.trim().is_empty() && r.images.is_empty() {
                return invalid("回复不能为空");
            }
        }
    }
    Ok(())
}
