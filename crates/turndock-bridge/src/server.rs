//! MCP 服务：`zhi`、`ask_user`、`await_reply`。

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, JsonObject,
    ListToolsResult, PaginatedRequestParams, ProgressNotificationParam, ServerCapabilities,
    ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::json;
use tracing::{debug, info};
use turndock_core::ipc::{BridgeFrame, BrokerFrame, ClientInfo};
use turndock_core::{
    AskSpec, AskUserParams, CallOutcome, CallResult, RequestId, ToolKind, ZhiParams,
};

use crate::format;
use crate::ipc::Ipc;

const PROGRESS_EVERY: Duration = Duration::from_secs(20);
const RECONNECT_EVERY: Duration = Duration::from_millis(500);
/// 剩余预算少于此值时直接返回 pending，不再发起新的等待。
const MIN_WAIT: Duration = Duration::from_millis(1500);

pub const INSTRUCTIONS: &str = "\
TurnDock（回合）让你在需要用户决定时停下来等待，而不是自作主张或提前结束。
- 需求不明确、有多个可行方案、即将做有影响的操作时，用 ask_user 询问，并给出选项。
- 一段工作完成、准备结束本轮回复之前，用 ask_user（或 zhi）请用户确认下一步；在得到明确答复前不要结束回合。
- 工具返回 pending 时，用户只是还没有回复：立即调用 await_reply(request_id) 继续等待。这不是同意。
- 用户拒绝、关闭、取消或超时，都表示没有获得同意：停止相关操作，在对话中说明。
- 只通过这些工具向用户提问，不要在普通回复里提问后直接结束。";

#[derive(Clone)]
pub struct Server {
    ipc: Arc<Ipc>,
    wait_override: Option<Duration>,
}

impl Server {
    pub fn new(ipc: Ipc, wait_override: Option<Duration>) -> Self {
        Self {
            ipc: Arc::new(ipc),
            wait_override,
        }
    }

    fn wait_for(&self, client: &ClientInfo) -> Duration {
        if let Some(w) = self.wait_override {
            return w;
        }
        wait_budget(&client.name)
    }

    /// 执行一次等待。连接中断时，若已知 request_id，就在等待预算内自动重连并续等，
    /// 使桌面应用重启对 AI 透明；预算用完则返回 pending。
    async fn run(
        &self,
        ctx: &RequestContext<RoleServer>,
        client: ClientInfo,
        tool: ToolKind,
        make: impl FnOnce(u64, u64) -> BridgeFrame,
        known_id: Option<RequestId>,
    ) -> CallToolResult {
        let deadline = Instant::now() + self.wait_for(&client);
        let heartbeat = ctx.meta.get_progress_token().map(|token| {
            let peer = ctx.peer.clone();
            tokio::spawn(async move {
                let mut n = 0u32;
                loop {
                    tokio::time::sleep(PROGRESS_EVERY).await;
                    n += 1;
                    let _ = peer
                        .notify_progress(
                            ProgressNotificationParam::new(token.clone(), f64::from(n))
                                .with_message("等待用户回复"),
                        )
                        .await;
                }
            })
        });

        let mut request_id = known_id;
        let mut first = Some(make);
        let pending = |id: &RequestId| {
            format::outcome(
                id,
                CallOutcome {
                    tool,
                    result: CallResult::Pending,
                },
            )
        };

        let result = 'outer: loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if first.is_none()
                && remaining < MIN_WAIT
                && let Some(id) = &request_id
            {
                break pending(id);
            }
            let conn = match self.ipc.connection(&client).await {
                Ok(c) => c,
                Err(e) => match &request_id {
                    Some(_) if first.is_none() => {
                        debug!(error = %e, "重连 TurnDock 失败，稍后重试");
                        if sleep_or_cancel(ctx, RECONNECT_EVERY).await {
                            break format::error("调用已被取消");
                        }
                        continue;
                    }
                    _ => break format::error(format!("{e:#}")),
                },
            };
            let (call, mut rx) = conn.register();
            let wait_ms = remaining.max(MIN_WAIT).as_millis() as u64;
            let frame = match (first.take(), &request_id) {
                (Some(make), _) => make(call, wait_ms),
                (None, Some(id)) => BridgeFrame::Await {
                    call,
                    request_id: id.clone(),
                    wait_ms,
                },
                (None, None) => break format::error("与 TurnDock 的连接中断"),
            };
            if let Err(e) = conn.send(frame) {
                conn.finish(call);
                if request_id.is_some() {
                    continue;
                }
                break format::error(format!("{e:#}"));
            }

            loop {
                tokio::select! {
                    frame = rx.recv() => match frame {
                        Some(BrokerFrame::Created { request_id: id, .. }) => {
                            debug!(request = %id, "请求已创建");
                            request_id = Some(id);
                        }
                        Some(BrokerFrame::Outcome { request_id: id, outcome, .. }) => {
                            let is_reply = matches!(outcome.result, CallResult::Reply(_));
                            let r = format::outcome(&id, outcome);
                            if is_reply {
                                // 结果即将交给 MCP 运行时写出。
                                let _ = conn.send(BridgeFrame::Ack { call });
                            }
                            conn.finish(call);
                            break 'outer r;
                        }
                        Some(BrokerFrame::Error { message, .. }) => {
                            conn.finish(call);
                            break 'outer format::error(message);
                        }
                        Some(_) => {}
                        None => {
                            conn.finish(call);
                            match &request_id {
                                Some(id) => {
                                    info!(request = %id, "与 TurnDock 的连接中断，尝试重连续等");
                                    if sleep_or_cancel(ctx, RECONNECT_EVERY).await {
                                        break 'outer format::error("调用已被取消");
                                    }
                                    continue 'outer;
                                }
                                None => break 'outer format::error("与 TurnDock 的连接中断"),
                            }
                        }
                    },
                    _ = ctx.ct.cancelled() => {
                        info!(call, "调用方取消");
                        let _ = conn.send(BridgeFrame::Cancel { call });
                        conn.finish(call);
                        break 'outer format::error("调用已被取消");
                    }
                }
            }
        };
        if let Some(h) = heartbeat {
            h.abort();
        }
        result
    }
}

/// 等待一段时间；期间调用被取消则返回 true。
async fn sleep_or_cancel(ctx: &RequestContext<RoleServer>, d: Duration) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(d) => false,
        _ = ctx.ct.cancelled() => true,
    }
}

/// 按宿主名称选择单次等待时长，需小于宿主的工具超时。见 docs/04 第 3 节。
pub fn wait_budget(client_name: &str) -> Duration {
    let n = client_name.to_ascii_lowercase();
    if n.contains("claude-code") || n == "claude code" {
        // stdio 空闲超时 30 分钟。
        Duration::from_secs(25 * 60)
    } else {
        // Codex 默认 60 秒；未知宿主取保守值。
        Duration::from_secs(50)
    }
}

fn parse<T: serde::de::DeserializeOwned>(args: Option<JsonObject>) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::Object(args.unwrap_or_default()))
        .map_err(|e| format!("参数无效：{e}"))
}

fn schema(v: serde_json::Value) -> Arc<JsonObject> {
    match v {
        serde_json::Value::Object(m) => Arc::new(m),
        _ => Arc::new(JsonObject::new()),
    }
}

fn tools() -> Vec<Tool> {
    let ask_user = Tool::new(
        "ask_user",
        "向用户提问并等待回复：确认下一步、在多个方案中选择、补充信息。\
         一段工作完成、准备结束本轮之前也应调用它。\
         返回 outcome：answered（已回复）/ declined（拒绝）/ pending（尚未回复，需调用 await_reply 继续等）/ \
         dismissed、cancelled、expired（未获同意）。",
        schema(json!({
            "type": "object",
            "properties": {
                "title": {"type": "string", "description": "一句话问题，显示在列表中（最多 160 字）"},
                "message": {"type": "string", "description": "完整说明与上下文，默认 Markdown"},
                "format": {"type": "string", "enum": ["markdown", "plain"], "default": "markdown"},
                "options": {
                    "type": "array",
                    "description": "可选项。id 在本次请求内唯一，回复中会返回所选 id",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {"type": "string"},
                            "label": {"type": "string"},
                            "description": {"type": "string"}
                        },
                        "required": ["id", "label"]
                    }
                },
                "selection": {
                    "type": "string", "enum": ["none", "single", "multiple"],
                    "description": "有选项时默认 single；无选项时必须为 none"
                },
                "allow_text": {"type": "boolean", "default": true},
                "allow_images": {"type": "boolean", "default": true}
            },
            "required": ["message"]
        })),
    );
    let await_reply = Tool::new(
        "await_reply",
        "继续等待之前某次提问的回复。当 ask_user 或 zhi 返回 pending 时立即调用，传入它给出的 request_id。",
        schema(json!({
            "type": "object",
            "properties": {
                "request_id": {"type": "string", "description": "pending 结果中的 request_id"}
            },
            "required": ["request_id"]
        })),
    );
    let zhi = Tool::new(
        "zhi",
        "（寸止兼容）向用户提问并等待回复，支持预定义选项（多选）、自由文本和图片。\
         返回 pending 时调用 await_reply 继续等待。",
        schema(json!({
            "type": "object",
            "properties": {
                "message": {"type": "string", "description": "要显示给用户的消息"},
                "predefined_options": {"type": "array", "items": {"type": "string"}, "description": "预定义的选项列表（可选）"},
                "is_markdown": {"type": "boolean", "description": "消息是否为 Markdown 格式，默认为 true"}
            },
            "required": ["message"]
        })),
    );
    [ask_user, await_reply, zhi]
        .into_iter()
        .map(|t| {
            t.with_annotations(
                ToolAnnotations::new()
                    .read_only(true)
                    .destructive(false)
                    .open_world(false),
            )
        })
        .collect()
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("turndock", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(tools()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tools().into_iter().find(|t| t.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let client = context
            .client_info()
            .map(|i| ClientInfo {
                name: i.name,
                version: i.version,
            })
            .unwrap_or_else(|| ClientInfo {
                name: "unknown".into(),
                version: String::new(),
            });
        let result = match request.name.as_ref() {
            "zhi" => match parse::<ZhiParams>(request.arguments)
                .and_then(|p| AskSpec::from_zhi(p).map_err(|e| e.to_string()))
            {
                Ok(spec) => {
                    let tool = spec.tool;
                    self.run(
                        &context,
                        client,
                        tool,
                        |call, wait_ms| BridgeFrame::Ask {
                            call,
                            spec,
                            wait_ms,
                        },
                        None,
                    )
                    .await
                }
                Err(e) => format::error(e),
            },
            "ask_user" => match parse::<AskUserParams>(request.arguments)
                .and_then(|p| AskSpec::from_ask_user(p).map_err(|e| e.to_string()))
            {
                Ok(spec) => {
                    let tool = spec.tool;
                    self.run(
                        &context,
                        client,
                        tool,
                        |call, wait_ms| BridgeFrame::Ask {
                            call,
                            spec,
                            wait_ms,
                        },
                        None,
                    )
                    .await
                }
                Err(e) => format::error(e),
            },
            "await_reply" => {
                #[derive(serde::Deserialize)]
                struct P {
                    request_id: String,
                }
                match parse::<P>(request.arguments).and_then(|p| {
                    RequestId::parse(&p.request_id).ok_or_else(|| "request_id 格式无效".to_owned())
                }) {
                    Ok(id) => {
                        let known = Some(id.clone());
                        self.run(
                            &context,
                            client,
                            ToolKind::AskUser,
                            move |call, wait_ms| BridgeFrame::Await {
                                call,
                                request_id: id,
                                wait_ms,
                            },
                            known,
                        )
                        .await
                    }
                    Err(e) => format::error(e),
                }
            }
            other => {
                return Err(ErrorData::invalid_params(
                    format!("未知工具：{other}"),
                    None,
                ));
            }
        };
        Ok(result.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_budget_stays_under_known_host_timeouts() {
        assert!(wait_budget("codex-mcp-client") < Duration::from_secs(60));
        assert!(wait_budget("claude-code") < Duration::from_secs(30 * 60));
        assert!(wait_budget("something-else") < Duration::from_secs(60));
    }

    #[test]
    fn tool_list_is_stable() {
        let names: Vec<_> = tools().into_iter().map(|t| t.name.to_string()).collect();
        assert_eq!(names, ["ask_user", "await_reply", "zhi"]);
    }
}
