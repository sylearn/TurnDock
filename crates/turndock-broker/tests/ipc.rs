//! 通过真实 Unix socket 驱动协调器，覆盖 docs/05 测试清单中的 P 系列场景。

use std::path::PathBuf;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::UnixStream;
use tokio_util::codec::{Framed, LinesCodec};
use turndock_broker::view::{SubmitReply, SubmitStatus};
use turndock_broker::{ActionError, BrokerConfig, BrokerHandle};
use turndock_core::ipc::{BridgeFrame, BrokerFrame, ClientInfo, ErrorCode};
use turndock_core::{
    AskSpec, AskUserParams, CallResult, Delivery, MessageFormat, OptionSpec, Outcome, Phase,
    RequestId, Selection, SubmissionId, ZhiParams,
};

struct Env {
    _dir: tempfile::TempDir,
    socket: PathBuf,
    broker: BrokerHandle,
}

async fn start(deadline_ms: i64) -> Env {
    let dir = tempfile::tempdir().expect("tmp");
    let socket = dir.path().join("run/b.sock");
    let broker = turndock_broker::spawn(BrokerConfig {
        db_path: Some(dir.path().join("t.db")),
        socket_path: socket.clone(),
        retention_ms: 60_000,
        deadline_ms,
    })
    .await
    .expect("spawn");
    Env {
        _dir: dir,
        socket,
        broker,
    }
}

struct Bridge {
    io: Framed<UnixStream, LinesCodec>,
}

impl Bridge {
    async fn connect(env: &Env, name: &str) -> Self {
        let mut b = Self::raw(env).await;
        b.send(BridgeFrame::Hello {
            protocol: 1,
            bridge_version: "test".into(),
            client: ClientInfo {
                name: name.into(),
                version: "1".into(),
            },
            cwd: Some(format!("/work/{name}")),
            pid: None,
        })
        .await;
        assert!(matches!(b.recv().await, BrokerFrame::Welcome { .. }));
        b
    }

    async fn raw(env: &Env) -> Self {
        let s = UnixStream::connect(&env.socket).await.expect("connect");
        Self {
            io: Framed::new(s, LinesCodec::new()),
        }
    }

    async fn send(&mut self, f: BridgeFrame) {
        self.io
            .send(serde_json::to_string(&f).expect("ser"))
            .await
            .expect("send");
    }

    async fn recv(&mut self) -> BrokerFrame {
        let line = tokio::time::timeout(Duration::from_secs(5), self.io.next())
            .await
            .expect("等待帧超时")
            .expect("连接关闭")
            .expect("读取");
        serde_json::from_str(&line).expect("de")
    }

    async fn assert_silent(&mut self, ms: u64) {
        if let Ok(Some(line)) =
            tokio::time::timeout(Duration::from_millis(ms), self.io.next()).await
        {
            panic!("不应收到帧：{line:?}");
        }
    }

    async fn ask(&mut self, call: u64, spec: AskSpec, wait_ms: u64) -> RequestId {
        self.send(BridgeFrame::Ask {
            call,
            spec,
            wait_ms,
        })
        .await;
        match self.recv().await {
            BrokerFrame::Created {
                call: c,
                request_id,
            } => {
                assert_eq!(c, call);
                request_id
            }
            other => panic!("期望 created，收到 {other:?}"),
        }
    }

    async fn outcome(&mut self, call: u64) -> CallResult {
        match self.recv().await {
            BrokerFrame::Outcome {
                call: c, outcome, ..
            } => {
                assert_eq!(c, call);
                outcome.result
            }
            other => panic!("期望 outcome，收到 {other:?}"),
        }
    }
}

fn choose() -> AskSpec {
    AskSpec::from_ask_user(AskUserParams {
        title: Some("选范围".into()),
        message: "选一个".into(),
        format: MessageFormat::Markdown,
        options: vec![
            OptionSpec {
                id: "a".into(),
                label: "方案 A".into(),
                description: None,
            },
            OptionSpec {
                id: "b".into(),
                label: "方案 B".into(),
                description: None,
            },
        ],
        selection: Some(Selection::Single),
        allow_text: true,
        allow_images: true,
    })
    .expect("spec")
}

fn zhi() -> AskSpec {
    AskSpec::from_zhi(ZhiParams {
        message: "继续吗？".into(),
        predefined_options: vec![],
        is_markdown: true,
    })
    .expect("spec")
}

fn reply(id: &RequestId, sub: &str, selected: &[&str], text: &str) -> SubmitReply {
    SubmitReply {
        request_id: id.clone(),
        submission_id: SubmissionId(sub.into()),
        outcome: Outcome::Answered,
        selected_ids: selected.iter().map(|s| s.to_string()).collect(),
        text: text.into(),
        images: vec![],
    }
}

async fn summary(env: &Env, id: &RequestId) -> turndock_broker::view::RequestSummary {
    env.broker
        .snapshot()
        .await
        .expect("snapshot")
        .requests
        .into_iter()
        .find(|r| &r.id == id)
        .expect("存在")
}

const LONG: u64 = 60_000;

#[tokio::test]
async fn p01_reply_within_wait_is_delivered_and_acked() {
    let env = start(LONG as i64).await;
    let mut b = Bridge::connect(&env, "codex").await;
    let id = b.ask(1, choose(), LONG).await;
    let s = summary(&env, &id).await;
    assert!(s.attached);
    assert_eq!(s.project.as_deref(), Some("codex"));

    let status = env.broker.submit(reply(&id, "s1", &["b"], "就这样")).await;
    assert_eq!(status, Ok(SubmitStatus::Delivered));
    match b.outcome(1).await {
        CallResult::Reply(p) => {
            assert_eq!(p.selected_labels, vec!["方案 B"]);
            assert_eq!(p.text, "就这样");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(summary(&env, &id).await.delivery, Delivery::InFlight);
    b.send(BridgeFrame::Ack { call: 1 }).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let s = summary(&env, &id).await;
    assert_eq!(s.delivery, Delivery::HandedOff);
    assert!(!s.attached);
}

#[tokio::test]
async fn p02_p03_pending_then_stored_then_claimed() {
    let env = start(LONG as i64).await;
    let mut b = Bridge::connect(&env, "codex").await;
    let id = b.ask(1, zhi(), 1_000).await;
    assert_eq!(b.outcome(1).await, CallResult::Pending);
    assert!(!summary(&env, &id).await.attached);

    let status = env.broker.submit(reply(&id, "s1", &[], "继续")).await;
    assert_eq!(status, Ok(SubmitStatus::Stored));
    assert_eq!(summary(&env, &id).await.delivery, Delivery::Pending);

    b.send(BridgeFrame::Await {
        call: 2,
        request_id: id.clone(),
        wait_ms: LONG,
    })
    .await;
    match b.outcome(2).await {
        CallResult::Reply(p) => assert_eq!(p.text, "继续"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn p04_cancel_closes_request_and_rejects_late_reply() {
    let env = start(LONG as i64).await;
    let mut b = Bridge::connect(&env, "claude-code").await;
    let id = b.ask(1, choose(), LONG).await;
    b.send(BridgeFrame::Cancel { call: 1 }).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(summary(&env, &id).await.phase, Phase::Cancelled);
    assert!(matches!(
        env.broker.submit(reply(&id, "s1", &["a"], "")).await,
        Err(ActionError::State(_))
    ));
    b.assert_silent(200).await;

    // 已取消的请求再被领取，明确告知已取消。
    b.send(BridgeFrame::Await {
        call: 2,
        request_id: id,
        wait_ms: LONG,
    })
    .await;
    assert_eq!(b.outcome(2).await, CallResult::Cancelled);
}

#[tokio::test]
async fn p06_disconnect_in_flight_becomes_unknown_and_reclaimable() {
    let env = start(LONG as i64).await;
    let mut b = Bridge::connect(&env, "codex").await;
    let id = b.ask(1, zhi(), LONG).await;
    env.broker
        .submit(reply(&id, "s1", &[], "好"))
        .await
        .expect("submit");
    let _ = b.outcome(1).await;
    drop(b); // 未 ack 即断开
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(summary(&env, &id).await.delivery, Delivery::Unknown);

    let mut b2 = Bridge::connect(&env, "codex").await;
    b2.send(BridgeFrame::Await {
        call: 1,
        request_id: id.clone(),
        wait_ms: LONG,
    })
    .await;
    match b2.outcome(1).await {
        CallResult::Reply(p) => assert_eq!(p.text, "好"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn p07_two_connections_are_isolated() {
    let env = start(LONG as i64).await;
    let mut a = Bridge::connect(&env, "codex").await;
    let mut b = Bridge::connect(&env, "cursor").await;
    // 相同的 call 编号，不同连接。
    let id_a = a.ask(1, zhi(), LONG).await;
    let id_b = b.ask(1, zhi(), LONG).await;
    assert_ne!(id_a, id_b);
    env.broker
        .submit(reply(&id_b, "s1", &[], "给 B"))
        .await
        .expect("submit");
    match b.outcome(1).await {
        CallResult::Reply(p) => assert_eq!(p.text, "给 B"),
        other => panic!("{other:?}"),
    }
    a.assert_silent(200).await;
}

#[tokio::test]
async fn p08_second_waiter_is_rejected() {
    let env = start(LONG as i64).await;
    let mut a = Bridge::connect(&env, "codex").await;
    let mut b = Bridge::connect(&env, "codex").await;
    let id = a.ask(1, zhi(), LONG).await;
    b.send(BridgeFrame::Await {
        call: 1,
        request_id: id,
        wait_ms: LONG,
    })
    .await;
    match b.recv().await {
        BrokerFrame::Error { code, .. } => assert_eq!(code, ErrorCode::AlreadyWaiting),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn p09_duplicate_submission_is_idempotent() {
    let env = start(LONG as i64).await;
    let mut b = Bridge::connect(&env, "codex").await;
    let id = b.ask(1, zhi(), LONG).await;
    let r = reply(&id, "same", &[], "一次");
    assert_eq!(
        env.broker.submit(r.clone()).await,
        Ok(SubmitStatus::Delivered)
    );
    assert_eq!(env.broker.submit(r).await, Ok(SubmitStatus::Duplicate));
    let _ = b.outcome(1).await;
    b.assert_silent(200).await;
    assert!(matches!(
        env.broker.submit(reply(&id, "other", &[], "两次")).await,
        Err(ActionError::State(_))
    ));
}

#[tokio::test]
async fn expiry_tells_waiting_call() {
    let env = start(1_200).await;
    let mut b = Bridge::connect(&env, "codex").await;
    let id = b.ask(1, zhi(), LONG).await;
    assert_eq!(b.outcome(1).await, CallResult::Expired);
    assert_eq!(summary(&env, &id).await.phase, Phase::Expired);
}

#[tokio::test]
async fn dismiss_tells_waiting_call() {
    let env = start(LONG as i64).await;
    let mut b = Bridge::connect(&env, "codex").await;
    let id = b.ask(1, zhi(), LONG).await;
    env.broker.dismiss(id.clone()).await.expect("dismiss");
    assert_eq!(b.outcome(1).await, CallResult::Dismissed);
    assert_eq!(summary(&env, &id).await.phase, Phase::Dismissed);
}

#[tokio::test]
async fn invalid_replies_are_rejected() {
    let env = start(LONG as i64).await;
    let mut b = Bridge::connect(&env, "codex").await;
    let id = b.ask(1, choose(), LONG).await;
    for bad in [
        reply(&id, "1", &["zzz"], ""),
        reply(&id, "2", &["a", "b"], ""),
        reply(&id, "3", &[], "  "),
    ] {
        assert!(matches!(
            env.broker.submit(bad).await,
            Err(ActionError::Invalid(_))
        ));
    }
    let mut svg = reply(&id, "4", &["a"], "");
    svg.images = vec!["PHN2Zz48L3N2Zz4=".into()];
    assert!(matches!(
        env.broker.submit(svg).await,
        Err(ActionError::Invalid(_))
    ));
}

#[tokio::test]
async fn image_mime_comes_from_bytes() {
    let env = start(LONG as i64).await;
    let mut b = Bridge::connect(&env, "codex").await;
    let id = b.ask(1, zhi(), LONG).await;
    let jpeg = turndock_broker::images::encode(&[
        0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0,
    ]);
    let mut r = reply(&id, "s", &[], "");
    r.images = vec![jpeg];
    env.broker.submit(r).await.expect("submit");
    match b.outcome(1).await {
        CallResult::Reply(p) => assert_eq!(p.images[0].mime, "image/jpeg"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn drafts_survive_and_clear_on_decision() {
    let env = start(LONG as i64).await;
    let mut b = Bridge::connect(&env, "codex").await;
    let id = b.ask(1, choose(), LONG).await;
    let draft = turndock_broker::view::Draft {
        text: "写到一半".into(),
        selected_ids: vec!["a".into()],
    };
    env.broker
        .save_draft(id.clone(), draft.clone())
        .await
        .expect("draft");
    assert!(summary(&env, &id).await.has_draft);
    let d = env
        .broker
        .detail(id.clone())
        .await
        .expect("detail")
        .expect("some");
    assert_eq!(d.draft, Some(draft));
    env.broker
        .submit(reply(&id, "s", &["a"], ""))
        .await
        .expect("submit");
    assert!(!summary(&env, &id).await.has_draft);
}

#[tokio::test]
async fn protocol_mismatch_is_rejected() {
    let env = start(LONG as i64).await;
    let mut b = Bridge::raw(&env).await;
    b.send(BridgeFrame::Hello {
        protocol: 99,
        bridge_version: "x".into(),
        client: ClientInfo::default(),
        cwd: None,
        pid: None,
    })
    .await;
    assert!(matches!(b.recv().await, BrokerFrame::Reject { .. }));
}

#[tokio::test]
async fn second_broker_on_same_socket_refuses_to_start() {
    let env = start(LONG as i64).await;
    let err = turndock_broker::spawn(BrokerConfig {
        db_path: None,
        socket_path: env.socket.clone(),
        retention_ms: 1,
        deadline_ms: 1,
    })
    .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn p10_waiting_request_survives_broker_restart() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = dir.path().join("t.db");
    let id = {
        let rt_dir = dir.path().join("a");
        let env_socket = rt_dir.join("b.sock");
        let broker = turndock_broker::spawn(BrokerConfig {
            db_path: Some(db.clone()),
            socket_path: env_socket.clone(),
            retention_ms: 60_000,
            deadline_ms: LONG as i64,
        })
        .await
        .expect("spawn");
        let env = Env {
            _dir: tempfile::tempdir().expect("tmp"),
            socket: env_socket,
            broker,
        };
        let mut b = Bridge::connect(&env, "codex").await;
        let id = b.ask(1, zhi(), 1_000).await;
        assert_eq!(b.outcome(1).await, CallResult::Pending);
        id
    };
    // 第二个协调器打开同一个数据库，模拟重启后的加载。
    let socket = dir.path().join("b/b.sock");
    let broker = turndock_broker::spawn(BrokerConfig {
        db_path: Some(db),
        socket_path: socket.clone(),
        retention_ms: 60_000,
        deadline_ms: LONG as i64,
    })
    .await
    .expect("spawn 2");
    let env = Env {
        _dir: tempfile::tempdir().expect("tmp"),
        socket,
        broker,
    };
    assert_eq!(summary(&env, &id).await.phase, Phase::Waiting);
    let mut b = Bridge::connect(&env, "codex").await;
    b.send(BridgeFrame::Await {
        call: 1,
        request_id: id.clone(),
        wait_ms: LONG,
    })
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    env.broker
        .submit(reply(&id, "s", &[], "重启后回复"))
        .await
        .expect("submit");
    match b.outcome(1).await {
        CallResult::Reply(p) => assert_eq!(p.text, "重启后回复"),
        other => panic!("{other:?}"),
    }
}
