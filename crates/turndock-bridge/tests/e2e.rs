//! 端到端：真实的 turndock-mcp 子进程 + 进程内 broker + rmcp 客户端（扮演 AI 宿主）。

use std::time::Duration;

use rmcp::ServiceExt;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation,
};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use serde_json::json;
use turndock_broker::view::SubmitReply;
use turndock_broker::{BrokerConfig, BrokerHandle};
use turndock_core::{Delivery, Outcome, Phase, RequestId, SubmissionId};

struct Env {
    dir: tempfile::TempDir,
    broker: BrokerHandle,
}

async fn start() -> Env {
    let dir = tempfile::tempdir().expect("tmp");
    let broker = turndock_broker::spawn(BrokerConfig {
        db_path: None,
        socket_path: dir.path().join("run/b.sock"),
        retention_ms: 60_000,
        deadline_ms: 3_600_000,
    })
    .await
    .expect("broker");
    Env { dir, broker }
}

async fn client(env: &Env, name: &str, wait_secs: u64) -> RunningService<RoleClient, ClientConfig> {
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_turndock-mcp"));
    cmd.env("TURNDOCK_SOCKET", env.dir.path().join("run/b.sock"))
        .env("TURNDOCK_NO_LAUNCH", "1")
        .env("TURNDOCK_WAIT_SECS", wait_secs.to_string());
    let transport = TokioChildProcess::new(cmd).expect("spawn bridge");
    ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::new(name, "1.0"),
    )
    .serve(transport)
    .await
    .expect("initialize")
}

fn args(v: serde_json::Value) -> rmcp::model::JsonObject {
    v.as_object().cloned().expect("object")
}

fn text(r: &CallToolResult) -> String {
    r.content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 等到 broker 里出现一条处于等待中的请求。
async fn wait_for_request(env: &Env) -> RequestId {
    for _ in 0..100 {
        let snap = env.broker.snapshot().await.expect("snapshot");
        if let Some(r) = snap
            .requests
            .iter()
            .find(|r| r.phase == Phase::Waiting && r.attached)
        {
            return r.id.clone();
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("没有出现请求");
}

async fn state(env: &Env, id: &RequestId) -> (Phase, Delivery) {
    let snap = env.broker.snapshot().await.expect("snapshot");
    let r = snap.requests.iter().find(|r| &r.id == id).expect("存在");
    (r.phase, r.delivery)
}

fn answer(id: &RequestId, selected: &[&str], t: &str) -> SubmitReply {
    SubmitReply {
        request_id: id.clone(),
        submission_id: SubmissionId(uuid_like()),
        outcome: Outcome::Answered,
        selected_ids: selected.iter().map(|s| s.to_string()).collect(),
        text: t.into(),
        images: vec![],
    }
}

fn uuid_like() -> String {
    format!("{:?}", std::time::Instant::now())
}

#[tokio::test]
async fn lists_tools_with_instructions() {
    let env = start().await;
    let c = client(&env, "claude-code", 30).await;
    let info = c.peer_info().expect("server info");
    assert!(
        info.instructions
            .as_deref()
            .unwrap_or("")
            .contains("await_reply")
    );
    let tools = c.list_all_tools().await.expect("tools");
    let names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
    assert_eq!(names, ["ask_user", "await_reply", "zhi"]);
    c.cancel().await.expect("close");
}

#[tokio::test]
async fn ask_user_round_trip_with_structured_result() {
    let env = start().await;
    let c = client(&env, "claude-code", 30).await;
    let call = c.call_tool(CallToolRequestParams::new("ask_user").with_arguments(args(json!({
        "title": "导出范围",
        "message": "报告已生成，选择导出范围。",
        "options": [{"id": "summary", "label": "只导出摘要"}, {"id": "full", "label": "完整报告"}]
    }))));
    let responder = async {
        let id = wait_for_request(&env).await;
        env.broker
            .submit(answer(&id, &["full"], "带上附录"))
            .await
            .expect("submit");
        id
    };
    let (result, id) = tokio::join!(call, responder);
    let result = result.expect("call");
    let s = result.structured_content.clone().expect("structured");
    assert_eq!(s["outcome"], "answered");
    assert_eq!(s["selected_ids"], json!(["full"]));
    assert_eq!(s["request_id"], id.as_str());
    assert!(text(&result).contains("完整报告"));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        state(&env, &id).await,
        (Phase::Decided, Delivery::HandedOff)
    );
    c.cancel().await.expect("close");
}

#[tokio::test]
async fn zhi_pending_then_await_reply_gets_stored_answer() {
    let env = start().await;
    // 模拟 Codex：每次只等 1 秒。
    let c = client(&env, "codex-mcp-client", 1).await;
    let r = c
        .call_tool(
            CallToolRequestParams::new("zhi").with_arguments(args(json!({
                "message": "这一步做完了，接下来？",
                "predefined_options": ["继续", "先写测试"]
            }))),
        )
        .await
        .expect("call");
    let t = text(&r);
    assert!(t.contains("await_reply"), "{t}");
    let id = {
        let snap = env.broker.snapshot().await.expect("snap");
        snap.requests[0].id.clone()
    };
    assert!(t.contains(id.as_str()));

    // AI 不在等的时候用户回复：保存为待领取。
    env.broker
        .submit(answer(&id, &["1", "2"], "顺便更新文档"))
        .await
        .expect("submit");
    assert_eq!(state(&env, &id).await, (Phase::Decided, Delivery::Pending));

    let r = c
        .call_tool(
            CallToolRequestParams::new("await_reply")
                .with_arguments(args(json!({ "request_id": id.as_str() }))),
        )
        .await
        .expect("await");
    assert_eq!(text(&r), "选择的选项: 继续, 先写测试\n\n顺便更新文档");
    c.cancel().await.expect("close");
}

#[tokio::test]
async fn host_cancellation_cancels_request() {
    let env = start().await;
    let c = client(&env, "claude-code", 30).await;
    let call = c.call_tool(
        CallToolRequestParams::new("ask_user")
            .with_arguments(args(json!({ "message": "要删除旧数据吗？" }))),
    );
    // 宿主放弃等待（例如用户在终端按 Esc），丢弃调用会发出 notifications/cancelled。
    let _ = tokio::time::timeout(Duration::from_millis(600), call).await;
    let id = env.broker.snapshot().await.expect("snap").requests[0]
        .id
        .clone();
    let mut phase = Phase::Waiting;
    for _ in 0..50 {
        phase = state(&env, &id).await.0;
        if phase == Phase::Cancelled {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert_eq!(phase, Phase::Cancelled);
    assert!(env.broker.submit(answer(&id, &[], "好")).await.is_err());
    c.cancel().await.expect("close");
}

#[tokio::test]
async fn bridge_restart_can_claim_by_request_id() {
    let env = start().await;
    let c1 = client(&env, "codex", 1).await;
    let r = c1
        .call_tool(
            CallToolRequestParams::new("ask_user")
                .with_arguments(args(json!({ "message": "先确认范围" }))),
        )
        .await
        .expect("call");
    let id = r.structured_content.expect("s")["request_id"]
        .as_str()
        .expect("id")
        .to_owned();
    c1.cancel().await.expect("close");

    let c2 = client(&env, "codex", 30).await;
    let call = c2.call_tool(
        CallToolRequestParams::new("await_reply").with_arguments(args(json!({ "request_id": id }))),
    );
    let responder = async {
        let id = wait_for_request(&env).await;
        env.broker
            .submit(answer(&id, &[], "新会话里回复"))
            .await
            .expect("submit");
    };
    let (r, _) = tokio::join!(call, responder);
    assert!(text(&r.expect("await")).contains("新会话里回复"));
    c2.cancel().await.expect("close");
}

#[tokio::test]
async fn missing_broker_fails_fast_with_clear_error() {
    let dir = tempfile::tempdir().expect("tmp");
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_turndock-mcp"));
    cmd.env("TURNDOCK_SOCKET", dir.path().join("none.sock"))
        .env("TURNDOCK_NO_LAUNCH", "1");
    let c = ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::new("codex", "1"),
    )
    .serve(TokioChildProcess::new(cmd).expect("spawn"))
    .await
    .expect("init");
    let started = std::time::Instant::now();
    let r = c
        .call_tool(
            CallToolRequestParams::new("zhi").with_arguments(args(json!({ "message": "在吗" }))),
        )
        .await
        .expect("call");
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(r.is_error, Some(true));
    let t = text(&r);
    assert!(t.contains("未运行") && t.contains("不代表同意"), "{t}");
    c.cancel().await.expect("close");
}

#[tokio::test]
async fn invalid_arguments_are_reported_not_treated_as_answers() {
    let env = start().await;
    let c = client(&env, "codex", 30).await;
    let r = c
        .call_tool(
            CallToolRequestParams::new("ask_user").with_arguments(args(json!({
                "message": "x",
                "options": [{"id": "a", "label": "A"}, {"id": "a", "label": "B"}]
            }))),
        )
        .await
        .expect("call");
    assert_eq!(r.is_error, Some(true));
    assert!(text(&r).contains("重复"));
    assert!(
        env.broker
            .snapshot()
            .await
            .expect("snap")
            .requests
            .is_empty()
    );
    c.cancel().await.expect("close");
}

#[tokio::test]
async fn broker_restart_is_transparent_to_waiting_call() {
    let dir = tempfile::tempdir().expect("tmp");
    let config = BrokerConfig {
        db_path: Some(dir.path().join("t.db")),
        socket_path: dir.path().join("run/b.sock"),
        retention_ms: 60_000,
        deadline_ms: 3_600_000,
    };
    let first = turndock_broker::spawn(config.clone())
        .await
        .expect("broker 1");
    let env = Env {
        dir,
        broker: first.clone(),
    };
    let c = client(&env, "claude-code", 20).await;
    let call = c.call_tool(
        CallToolRequestParams::new("ask_user")
            .with_arguments(args(json!({ "message": "重启前提的问题" }))),
    );
    let restart_and_reply = async {
        let id = wait_for_request(&env).await;
        // 模拟桌面应用重启（例如升级）。
        first.shutdown();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let second = turndock_broker::spawn(config).await.expect("broker 2");
        let env2 = Env {
            dir: tempfile::tempdir().expect("tmp"),
            broker: second.clone(),
        };
        // bridge 自动重连并重新挂上同一条请求。
        let again = wait_for_request(&env2).await;
        assert_eq!(again, id);
        second
            .submit(answer(&id, &[], "重启后也能收到"))
            .await
            .expect("submit");
    };
    let (r, _) = tokio::join!(call, restart_and_reply);
    let r = r.expect("call");
    assert_ne!(r.is_error, Some(true), "{}", text(&r));
    assert!(text(&r).contains("重启后也能收到"));
    c.cancel().await.expect("close");
}
