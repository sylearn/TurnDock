//! bridge ↔ broker 的 IPC 帧：换行分隔的 JSON。见 docs/04-协议与状态.md 第 5 节。

use serde::{Deserialize, Serialize};

use crate::ids::RequestId;
use crate::reply::CallOutcome;
use crate::spec::AskSpec;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ClientInfo {
    pub name: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeFrame {
    Hello {
        protocol: u32,
        bridge_version: String,
        client: ClientInfo,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        pid: Option<u32>,
    },
    Ask {
        call: u64,
        spec: AskSpec,
        wait_ms: u64,
    },
    Await {
        call: u64,
        request_id: RequestId,
        wait_ms: u64,
    },
    Cancel {
        call: u64,
    },
    Ack {
        call: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    NotFound,
    AlreadyWaiting,
    Capacity,
    Storage,
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrokerFrame {
    Welcome {
        connection_id: u64,
        broker_version: String,
    },
    Reject {
        reason: String,
    },
    Created {
        call: u64,
        request_id: RequestId,
    },
    Outcome {
        call: u64,
        request_id: RequestId,
        outcome: CallOutcome,
    },
    Error {
        call: u64,
        code: ErrorCode,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_use_snake_case_tags() {
        let f = BridgeFrame::Cancel { call: 3 };
        assert_eq!(
            serde_json::to_string(&f).expect("ser"),
            r#"{"type":"cancel","call":3}"#
        );
        let back: BridgeFrame = serde_json::from_str(r#"{"type":"ack","call":9}"#).expect("de");
        assert_eq!(back, BridgeFrame::Ack { call: 9 });
    }
}
