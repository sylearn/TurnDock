//! 标识符。

use serde::{Deserialize, Serialize};

/// 请求 ID：随机 UUID，同时作为 AI 领取回复的能力令牌。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(transparent)]
pub struct RequestId(pub String);

impl RequestId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// 只接受规范的 UUID 文本，避免任意字符串进入存储与日志。
    pub fn parse(s: &str) -> Option<Self> {
        uuid::Uuid::parse_str(s.trim())
            .ok()
            .map(|u| Self(u.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// broker 进程内的连接编号，只在本次运行有效。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectionId(pub u64);

/// 某个连接上的某次工具调用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallRef {
    pub connection: ConnectionId,
    pub call: u64,
}

/// UI 一次提交的幂等键。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(transparent)]
pub struct SubmissionId(pub String);
