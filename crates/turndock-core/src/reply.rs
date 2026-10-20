//! 用户的决定，以及交给调用方的结果。

use serde::{Deserialize, Serialize};

use crate::spec::ToolKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Answered,
    Declined,
}

/// 已校验的图片，`data` 为 base64。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePayload {
    pub mime: String,
    pub data: String,
}

/// 交给 AI 的回复内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyPayload {
    pub outcome: Outcome,
    pub selected_ids: Vec<String>,
    pub selected_labels: Vec<String>,
    pub text: String,
    #[serde(default)]
    pub images: Vec<ImagePayload>,
}

/// 一次调用的结局。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallResult {
    Reply(ReplyPayload),
    /// 本次等待预算用完，用户尚未回复。
    Pending,
    Cancelled,
    Expired,
    Dismissed,
}

/// 结局附带的上下文，bridge 用它决定文本格式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallOutcome {
    pub tool: ToolKind,
    pub result: CallResult,
}
