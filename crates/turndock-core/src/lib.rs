//! 回合（TurnDock）领域核心：请求规格、状态机、回复与 IPC 帧。不做任何 IO。

pub mod ids;
pub mod ipc;
pub mod limits;
pub mod paths;
pub mod reply;
pub mod spec;
pub mod state;

pub use ids::{CallRef, ConnectionId, RequestId, SubmissionId};
pub use reply::{CallOutcome, CallResult, ImagePayload, Outcome, ReplyPayload};
pub use spec::{
    AskSpec, AskUserParams, Check, CheckStatus, HandOffParams, MessageFormat, OptionSpec,
    Selection, ToolKind, ZhiParams,
};
pub use state::{Delivery, Phase, RequestState};
