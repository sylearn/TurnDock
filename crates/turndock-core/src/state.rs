//! 请求状态机。纯数据、无 IO；所有竞态由调用方（协调器）按顺序调用这些方法来裁决。
//!
//! 两个维度：阶段 [`Phase`] 与交付 [`Delivery`]，外加当前挂着的调用 `attached`。
//! 详见 docs/04-协议与状态.md 第 4 节。

use serde::{Deserialize, Serialize};

use crate::ids::{CallRef, ConnectionId, SubmissionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// 等用户回复。
    Waiting,
    /// 用户已回复或拒绝。
    Decided,
    /// 调用方取消。
    Cancelled,
    /// 超过截止时间。
    Expired,
    /// 用户关闭，未作答复。
    Dismissed,
}

impl Phase {
    pub fn is_closed(self) -> bool {
        matches!(self, Phase::Cancelled | Phase::Expired | Phase::Dismissed)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Waiting => "waiting",
            Phase::Decided => "decided",
            Phase::Cancelled => "cancelled",
            Phase::Expired => "expired",
            Phase::Dismissed => "dismissed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "waiting" => Phase::Waiting,
            "decided" => Phase::Decided,
            "cancelled" => Phase::Cancelled,
            "expired" => Phase::Expired,
            "dismissed" => Phase::Dismissed,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// 尚无回复可交付。
    None,
    /// 回复已保存，等待 AI 领取。
    Pending,
    /// 回复已发给挂着的调用，等待 bridge 确认。
    InFlight,
    /// bridge 已把结果交给 MCP 运行时。
    HandedOff,
    /// 发出后连接断开，无法确认是否送达。
    Unknown,
}

impl Delivery {
    pub fn as_str(self) -> &'static str {
        match self {
            Delivery::None => "none",
            Delivery::Pending => "pending",
            Delivery::InFlight => "in_flight",
            Delivery::HandedOff => "handed_off",
            Delivery::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Delivery::None,
            "pending" => Delivery::Pending,
            "in_flight" => Delivery::InFlight,
            "handed_off" => Delivery::HandedOff,
            "unknown" => Delivery::Unknown,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestState {
    pub phase: Phase,
    pub delivery: Delivery,
    pub attached: Option<CallRef>,
    pub submission: Option<SubmissionId>,
}

/// 调用挂到请求上之后应该发生什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachEffect {
    /// 开始等待用户。
    Wait,
    /// 已有回复，立即交付给这个调用。
    DeliverNow,
    /// 请求已结束，立即告知调用方。
    Closed(Phase),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecideEffect {
    /// 立即把回复交给挂着的调用。
    Deliver(CallRef),
    /// 没有调用挂着，保存为待领取。
    Stored,
    /// 同一个 submission 重复提交，返回已有结果，不做任何变化。
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelEffect {
    /// 请求被取消。
    Cancelled,
    /// 回复已在途，无法撤回；状态不变。
    TooLate,
    /// 该调用并未挂在此请求上，忽略。
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StateError {
    #[error("已有另一个调用在等待这条请求")]
    AlreadyAttached,
    #[error("请求已处于 {0:?}，不能再回复")]
    NotWaiting(Phase),
}

impl RequestState {
    pub fn new_attached(call: CallRef) -> Self {
        Self {
            phase: Phase::Waiting,
            delivery: Delivery::None,
            attached: Some(call),
            submission: None,
        }
    }

    /// 调用（ask 的首次等待或 await_reply）挂到请求上。
    pub fn attach(&mut self, call: CallRef) -> Result<AttachEffect, StateError> {
        if self.phase.is_closed() {
            return Ok(AttachEffect::Closed(self.phase));
        }
        if self.attached.is_some() {
            return Err(StateError::AlreadyAttached);
        }
        self.attached = Some(call);
        match self.phase {
            Phase::Waiting => Ok(AttachEffect::Wait),
            // AI 显式再次领取：无论之前是待领取、未知还是已交付，都可以（重新）交付。
            _ => {
                self.delivery = Delivery::InFlight;
                Ok(AttachEffect::DeliverNow)
            }
        }
    }

    /// 本次调用的等待预算用完。返回 true 表示应向该调用返回 pending。
    pub fn wait_elapsed(&mut self, call: CallRef) -> bool {
        if self.attached == Some(call) && self.phase == Phase::Waiting {
            self.attached = None;
            true
        } else {
            false
        }
    }

    /// 用户回复或拒绝。
    pub fn decide(&mut self, submission: SubmissionId) -> Result<DecideEffect, StateError> {
        if self.phase == Phase::Decided && self.submission.as_ref() == Some(&submission) {
            return Ok(DecideEffect::Duplicate);
        }
        if self.phase != Phase::Waiting {
            return Err(StateError::NotWaiting(self.phase));
        }
        self.phase = Phase::Decided;
        self.submission = Some(submission);
        match self.attached {
            Some(call) => {
                self.delivery = Delivery::InFlight;
                Ok(DecideEffect::Deliver(call))
            }
            None => {
                self.delivery = Delivery::Pending;
                Ok(DecideEffect::Stored)
            }
        }
    }

    /// 调用方取消了它的工具调用。
    pub fn cancel(&mut self, call: CallRef) -> CancelEffect {
        if self.attached != Some(call) {
            return CancelEffect::Ignored;
        }
        match self.phase {
            Phase::Waiting => {
                self.phase = Phase::Cancelled;
                self.attached = None;
                CancelEffect::Cancelled
            }
            Phase::Decided => CancelEffect::TooLate,
            _ => CancelEffect::Ignored,
        }
    }

    /// bridge 确认已把结果交给 MCP 运行时。
    pub fn ack(&mut self, call: CallRef) -> bool {
        if self.attached == Some(call) && self.delivery == Delivery::InFlight {
            self.delivery = Delivery::HandedOff;
            self.attached = None;
            true
        } else {
            false
        }
    }

    /// 某个连接断开。返回是否有变化。
    pub fn connection_lost(&mut self, connection: ConnectionId) -> bool {
        match self.attached {
            Some(c) if c.connection == connection => {
                self.attached = None;
                if self.delivery == Delivery::InFlight {
                    self.delivery = Delivery::Unknown;
                }
                true
            }
            _ => false,
        }
    }

    /// 到达截止时间。返回需要通知的挂着的调用（如有）；None 外层表示无变化。
    pub fn expire(&mut self) -> Option<Option<CallRef>> {
        if self.phase != Phase::Waiting {
            return None;
        }
        self.phase = Phase::Expired;
        Some(self.attached.take())
    }

    /// 用户关闭请求，不作答复。
    pub fn dismiss(&mut self) -> Result<Option<CallRef>, StateError> {
        if self.phase != Phase::Waiting {
            return Err(StateError::NotWaiting(self.phase));
        }
        self.phase = Phase::Dismissed;
        Ok(self.attached.take())
    }

    /// broker 重启后恢复：所有调用都已不存在。
    pub fn restore_after_restart(&mut self) {
        self.attached = None;
        if self.delivery == Delivery::InFlight {
            self.delivery = Delivery::Unknown;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(conn: u64, n: u64) -> CallRef {
        CallRef {
            connection: ConnectionId(conn),
            call: n,
        }
    }

    fn sub(s: &str) -> SubmissionId {
        SubmissionId(s.into())
    }

    #[test]
    fn reply_while_attached_delivers_then_acks() {
        let mut s = RequestState::new_attached(call(1, 1));
        assert_eq!(s.decide(sub("a")), Ok(DecideEffect::Deliver(call(1, 1))));
        assert_eq!(s.delivery, Delivery::InFlight);
        assert!(s.ack(call(1, 1)));
        assert_eq!(s.delivery, Delivery::HandedOff);
        assert_eq!(s.attached, None);
    }

    #[test]
    fn wait_elapsed_then_reply_is_stored_then_claimed() {
        let mut s = RequestState::new_attached(call(1, 1));
        assert!(s.wait_elapsed(call(1, 1)));
        assert_eq!(s.decide(sub("a")), Ok(DecideEffect::Stored));
        assert_eq!(s.delivery, Delivery::Pending);
        assert_eq!(s.attach(call(1, 2)), Ok(AttachEffect::DeliverNow));
        assert!(s.ack(call(1, 2)));
    }

    #[test]
    fn stale_timer_does_not_detach_new_call() {
        let mut s = RequestState::new_attached(call(1, 1));
        assert!(s.wait_elapsed(call(1, 1)));
        assert_eq!(s.attach(call(1, 2)), Ok(AttachEffect::Wait));
        assert!(!s.wait_elapsed(call(1, 1)));
        assert_eq!(s.attached, Some(call(1, 2)));
    }

    #[test]
    fn second_waiter_is_rejected() {
        let mut s = RequestState::new_attached(call(1, 1));
        assert_eq!(s.attach(call(2, 1)), Err(StateError::AlreadyAttached));
    }

    #[test]
    fn duplicate_submission_is_idempotent_and_other_submission_rejected() {
        let mut s = RequestState::new_attached(call(1, 1));
        assert!(s.wait_elapsed(call(1, 1)));
        assert_eq!(s.decide(sub("a")), Ok(DecideEffect::Stored));
        assert_eq!(s.decide(sub("a")), Ok(DecideEffect::Duplicate));
        assert_eq!(
            s.decide(sub("b")),
            Err(StateError::NotWaiting(Phase::Decided))
        );
    }

    #[test]
    fn cancel_only_by_attached_call() {
        let mut s = RequestState::new_attached(call(1, 1));
        assert_eq!(s.cancel(call(2, 1)), CancelEffect::Ignored);
        assert_eq!(s.cancel(call(1, 1)), CancelEffect::Cancelled);
        assert_eq!(
            s.decide(sub("a")),
            Err(StateError::NotWaiting(Phase::Cancelled))
        );
        assert_eq!(
            s.attach(call(1, 2)),
            Ok(AttachEffect::Closed(Phase::Cancelled))
        );
    }

    #[test]
    fn cancel_after_reply_in_flight_is_too_late() {
        let mut s = RequestState::new_attached(call(1, 1));
        s.decide(sub("a")).expect("decide");
        assert_eq!(s.cancel(call(1, 1)), CancelEffect::TooLate);
        assert_eq!(s.phase, Phase::Decided);
    }

    #[test]
    fn connection_lost_while_in_flight_becomes_unknown_and_can_be_reclaimed() {
        let mut s = RequestState::new_attached(call(1, 1));
        s.decide(sub("a")).expect("decide");
        assert!(s.connection_lost(ConnectionId(1)));
        assert_eq!(s.delivery, Delivery::Unknown);
        assert_eq!(s.attach(call(2, 1)), Ok(AttachEffect::DeliverNow));
        assert_eq!(s.delivery, Delivery::InFlight);
    }

    #[test]
    fn connection_lost_while_waiting_keeps_request_open() {
        let mut s = RequestState::new_attached(call(1, 1));
        assert!(s.connection_lost(ConnectionId(1)));
        assert_eq!(s.phase, Phase::Waiting);
        assert_eq!(s.attach(call(2, 1)), Ok(AttachEffect::Wait));
    }

    #[test]
    fn expire_and_dismiss_never_become_decided() {
        let mut s = RequestState::new_attached(call(1, 1));
        assert_eq!(s.expire(), Some(Some(call(1, 1))));
        assert!(s.decide(sub("a")).is_err());
        assert_eq!(s.expire(), None);

        let mut d = RequestState::new_attached(call(1, 1));
        assert_eq!(d.dismiss(), Ok(Some(call(1, 1))));
        assert!(d.decide(sub("a")).is_err());
    }

    #[test]
    fn restart_turns_in_flight_into_unknown() {
        let mut s = RequestState::new_attached(call(1, 1));
        s.decide(sub("a")).expect("decide");
        s.restore_after_restart();
        assert_eq!(s.delivery, Delivery::Unknown);
        assert_eq!(s.attached, None);
    }

    /// 随机交错所有事件 1000 轮：结局只有一个；关闭的请求永不变为 decided；
    /// 被取消的请求永不交付。
    #[test]
    fn randomized_races_have_single_outcome() {
        for seed in 0..1000u64 {
            let mut rng = fastrand::Rng::with_seed(seed);
            let first = call(1, 1);
            let mut s = RequestState::new_attached(first);
            let mut current = Some(first);
            let mut next_call = 2;
            let mut decided_by: Option<String> = None;
            let mut ever_closed = false;
            for _ in 0..12 {
                let before = s.phase;
                match rng.u8(0..8) {
                    0 | 1 => {
                        let id = format!("s{}", rng.u8(0..2));
                        if s.decide(sub(&id)).is_ok() && s.phase == Phase::Decided {
                            match &decided_by {
                                None => decided_by = Some(id),
                                Some(prev) => assert_eq!(prev, &id, "seed {seed}"),
                            }
                        }
                    }
                    2 => {
                        if let Some(c) = current {
                            s.cancel(c);
                        }
                    }
                    3 => {
                        if let Some(c) = current {
                            s.wait_elapsed(c);
                        }
                    }
                    4 => {
                        let c = call(rng.u64(1..3), next_call);
                        next_call += 1;
                        if s.attach(c).is_ok() && s.attached == Some(c) {
                            current = Some(c);
                        }
                    }
                    5 => {
                        if let Some(c) = current {
                            s.ack(c);
                        }
                    }
                    6 => {
                        s.connection_lost(ConnectionId(rng.u64(1..3)));
                    }
                    _ => {
                        let _ = s.expire();
                    }
                }
                if before.is_closed() {
                    assert_eq!(s.phase, before, "seed {seed}: closed phase changed");
                }
                if s.phase.is_closed() {
                    ever_closed = true;
                    assert_ne!(s.delivery, Delivery::InFlight, "seed {seed}");
                }
                if before == Phase::Decided {
                    assert_eq!(s.phase, Phase::Decided, "seed {seed}: decided changed");
                }
            }
            assert!(!(ever_closed && decided_by.is_some()), "seed {seed}");
        }
    }
}
