//! 给界面的只读视图与界面动作参数。由 ts-rs 导出为前端类型。

use serde::{Deserialize, Serialize};
use turndock_core::{
    Delivery, MessageFormat, OptionSpec, Outcome, Phase, RequestId, Selection, SubmissionId,
    ToolKind,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RequestSummary {
    pub id: RequestId,
    pub client_name: String,
    pub client_version: String,
    /// 项目目录名（展示用）。
    pub project: Option<String>,
    /// 项目完整路径（展示用，不作为授权依据）。
    pub project_path: Option<String>,
    pub title: String,
    pub tool: ToolKind,
    pub phase: Phase,
    pub delivery: Delivery,
    /// 当前是否有调用挂着在等。
    pub attached: bool,
    pub outcome: Option<Outcome>,
    pub has_draft: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub created_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub updated_at: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub deadline_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Snapshot {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: u64,
    pub requests: Vec<RequestSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DecisionView {
    pub outcome: Outcome,
    pub selected_ids: Vec<String>,
    pub text: String,
    pub image_count: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub decided_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Draft {
    pub text: String,
    pub selected_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RequestDetail {
    pub summary: RequestSummary,
    /// 已净化的 HTML。
    pub message_html: String,
    /// 原文，用于复制。
    pub message_text: String,
    pub format: MessageFormat,
    pub options: Vec<OptionSpec>,
    pub selection: Selection,
    pub allow_text: bool,
    pub allow_images: bool,
    pub decision: Option<DecisionView>,
    pub draft: Option<Draft>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SubmitReply {
    pub request_id: RequestId,
    pub submission_id: SubmissionId,
    pub outcome: Outcome,
    pub selected_ids: Vec<String>,
    pub text: String,
    /// base64 图片数据；类型由 broker 按文件签名判断。
    pub images: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum SubmitStatus {
    /// 已交给正在等待的调用。
    Delivered,
    /// 没有调用在等，已保存为待领取。
    Stored,
    /// 重复提交，结果与之前相同。
    Duplicate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrokerEvent {
    Changed {
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        revision: u64,
    },
    NewRequest {
        id: RequestId,
        client_name: String,
        project: Option<String>,
        title: String,
    },
}
