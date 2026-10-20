//! 把调用结局格式化为给 AI 的 MCP 结果。文字直白：沉默与超时永远不是同意。

use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::json;
use turndock_core::{CallOutcome, CallResult, Outcome, ReplyPayload, RequestId, ToolKind};

pub fn outcome(id: &RequestId, o: CallOutcome) -> CallToolResult {
    match o.result {
        CallResult::Reply(p) => reply(id, o.tool, p),
        CallResult::Pending => pending(id, o.tool),
        CallResult::Cancelled => closed(
            id,
            o.tool,
            "cancelled",
            "这条请求已被取消，用户没有作出回复。不要把它当作同意。",
        ),
        CallResult::Expired => closed(
            id,
            o.tool,
            "expired",
            "这条请求已超过等待期限，用户没有作出回复。不要把它当作同意；停止相关操作，在对话中说明情况。",
        ),
        CallResult::Dismissed => closed(
            id,
            o.tool,
            "dismissed",
            "用户关闭了这条请求，没有给出回复。不要把它当作同意；停止相关操作，在对话中说明情况并等待新的指示。",
        ),
    }
}

fn reply(id: &RequestId, tool: ToolKind, p: ReplyPayload) -> CallToolResult {
    let mut content: Vec<ContentBlock> = p
        .images
        .iter()
        .map(|i| ContentBlock::image(i.data.clone(), i.mime.clone()))
        .collect();
    let text = match (tool, p.outcome) {
        (_, Outcome::Declined) => {
            let mut t = String::from(
                "用户拒绝了这个请求。不要执行相关操作；在对话中说明，并等待新的指示。",
            );
            if !p.text.trim().is_empty() {
                t.push_str("\n\n理由：");
                t.push_str(p.text.trim());
            }
            t
        }
        (ToolKind::Zhi, Outcome::Answered) => {
            // 与寸止的回复格式一致。
            let mut parts = Vec::new();
            if !p.selected_labels.is_empty() {
                parts.push(format!("选择的选项: {}", p.selected_labels.join(", ")));
            }
            if !p.text.trim().is_empty() {
                parts.push(p.text.trim().to_owned());
            }
            if parts.is_empty() {
                parts.push(format!("（用户发送了 {} 张图片）", p.images.len()));
            }
            parts.join("\n\n")
        }
        (ToolKind::HandOff, Outcome::Answered) => {
            let mut t = match (p.selected_labels.first(), p.selected_ids.first()) {
                (Some(l), Some(i)) => format!("用户选定了下一步：{l}（{i}）。按这个方向继续。"),
                _ => String::from("用户没有选用建议的下一步，给出了新的指示。按用户的指示继续。"),
            };
            if !p.text.trim().is_empty() {
                t.push_str("\n用户补充：");
                t.push_str(p.text.trim());
            }
            if !p.images.is_empty() {
                t.push_str(&format!("\n附图：{} 张", p.images.len()));
            }
            t.push_str("\n这一轮结束时，再用 hand_off 交接。");
            t
        }
        (ToolKind::AskUser, Outcome::Answered) => {
            let mut t = String::from("用户已回复。");
            if !p.selected_labels.is_empty() {
                let picks: Vec<_> = p
                    .selected_labels
                    .iter()
                    .zip(&p.selected_ids)
                    .map(|(l, i)| format!("{l}（{i}）"))
                    .collect();
                t.push_str(&format!("\n选择：{}", picks.join("、")));
            }
            if !p.text.trim().is_empty() {
                t.push_str("\n回复：");
                t.push_str(p.text.trim());
            }
            if !p.images.is_empty() {
                t.push_str(&format!("\n附图：{} 张", p.images.len()));
            }
            t
        }
    };
    content.push(ContentBlock::text(text));
    let mut result = CallToolResult::success(content);
    if tool != ToolKind::Zhi {
        result.structured_content = Some(json!({
            "outcome": match p.outcome { Outcome::Answered => "answered", Outcome::Declined => "declined" },
            "request_id": id.as_str(),
            "selected_ids": p.selected_ids,
            "selected_labels": p.selected_labels,
            "text": p.text,
            "image_count": p.images.len(),
        }));
    }
    result
}

fn pending(id: &RequestId, tool: ToolKind) -> CallToolResult {
    let text = format!(
        "用户还没有回复（request_id: {id}）。这不是同意，也不是拒绝。\n\
         请立即调用 await_reply，参数 request_id 为 \"{id}\"，继续等待用户回复。\n\
         不要结束当前回合，不要自行做决定，也不要重复提出同一个问题。"
    );
    with_status(id, tool, "pending", text, false)
}

fn closed(id: &RequestId, tool: ToolKind, status: &str, text: &str) -> CallToolResult {
    with_status(
        id,
        tool,
        status,
        format!("{text}（request_id: {id}）"),
        false,
    )
}

fn with_status(
    id: &RequestId,
    tool: ToolKind,
    status: &str,
    text: String,
    is_error: bool,
) -> CallToolResult {
    let content = vec![ContentBlock::text(text)];
    let mut r = if is_error {
        CallToolResult::error(content)
    } else {
        CallToolResult::success(content)
    };
    if tool != ToolKind::Zhi {
        r.structured_content = Some(json!({ "outcome": status, "request_id": id.as_str() }));
    }
    r
}

/// 运行错误（未连接、参数无效等）。与用户的拒绝严格区分。
pub fn error(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(format!(
        "TurnDock 错误：{}。这不是用户的回复，也不代表同意。",
        message.into()
    ))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use turndock_core::ImagePayload;

    fn text_of(r: &CallToolResult) -> String {
        r.content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn payload(outcome: Outcome) -> ReplyPayload {
        ReplyPayload {
            outcome,
            selected_ids: vec!["1".into(), "2".into()],
            selected_labels: vec!["继续".into(), "写测试".into()],
            text: "顺便更新文档".into(),
            images: vec![ImagePayload {
                mime: "image/jpeg".into(),
                data: "AAAA".into(),
            }],
        }
    }

    #[test]
    fn zhi_reply_matches_cunzhi_format() {
        let id = RequestId::new();
        let r = outcome(
            &id,
            CallOutcome {
                tool: ToolKind::Zhi,
                result: CallResult::Reply(payload(Outcome::Answered)),
            },
        );
        assert_eq!(text_of(&r), "选择的选项: 继续, 写测试\n\n顺便更新文档");
        assert_eq!(r.content.len(), 2, "图片在前，文字在后");
        assert!(r.structured_content.is_none());
    }

    #[test]
    fn pending_tells_ai_to_await_and_not_assume_consent() {
        let id = RequestId::new();
        let r = outcome(
            &id,
            CallOutcome {
                tool: ToolKind::AskUser,
                result: CallResult::Pending,
            },
        );
        let t = text_of(&r);
        assert!(t.contains("await_reply"));
        assert!(t.contains("不是同意"));
        assert!(t.contains(id.as_str()));
        assert_eq!(
            r.structured_content
                .as_ref()
                .and_then(|v| v["outcome"].as_str()),
            Some("pending")
        );
        assert_ne!(r.is_error, Some(true));
    }

    #[test]
    fn hand_off_reply_names_the_chosen_step() {
        let id = RequestId::new();
        let mut p = payload(Outcome::Answered);
        p.selected_ids = vec!["tests".into()];
        p.selected_labels = vec!["补测试".into()];
        let r = outcome(
            &id,
            CallOutcome {
                tool: ToolKind::HandOff,
                result: CallResult::Reply(p),
            },
        );
        let t = text_of(&r);
        assert!(t.contains("补测试（tests）"), "{t}");
        assert!(t.contains("hand_off"), "{t}");
        assert_eq!(
            r.structured_content
                .as_ref()
                .and_then(|v| v["selected_ids"][0].as_str()),
            Some("tests")
        );
    }

    #[test]
    fn decline_is_not_an_error_but_explicit() {
        let id = RequestId::new();
        let mut p = payload(Outcome::Declined);
        p.text = "太危险".into();
        let r = outcome(
            &id,
            CallOutcome {
                tool: ToolKind::AskUser,
                result: CallResult::Reply(p),
            },
        );
        assert!(text_of(&r).contains("拒绝"));
        assert!(text_of(&r).contains("太危险"));
        assert_ne!(r.is_error, Some(true));
    }
}
