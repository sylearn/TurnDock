//! 请求规格：AI 通过工具提交的问题，以及对它的校验。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::limits;

/// 产生请求的工具。决定回复给 AI 时的文本格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// 寸止兼容入口。
    Zhi,
    /// 中途需要用户做一个决定。
    AskUser,
    /// 一轮工作结束时的交接：成果、验证、风险与建议的下一步。
    HandOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum MessageFormat {
    #[default]
    Markdown,
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    None,
    Single,
    Multiple,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct OptionSpec {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub description: Option<String>,
    /// AI 推荐的选项；每个请求最多一个。
    #[serde(default)]
    pub recommended: bool,
}

/// 交接中的一项验证。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// 已运行并通过。
    Passed,
    /// 已运行但失败。
    Failed,
    /// 没有运行。
    NotRun,
    /// 需要人来看。
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Check {
    pub label: String,
    pub status: CheckStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub detail: Option<String>,
}

/// 建议的下一步（`hand_off` 的参数形式）。
#[derive(Debug, Clone, Deserialize)]
pub struct NextStep {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub recommended: bool,
}

/// `hand_off` 工具的原始参数。
#[derive(Debug, Clone, Deserialize)]
pub struct HandOffParams {
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub checks: Vec<Check>,
    #[serde(default)]
    pub risks: Vec<String>,
    pub next_steps: Vec<NextStep>,
}

/// 经过校验的请求规格。只能通过 [`AskSpec::from_zhi`] / [`AskSpec::from_ask_user`] 构造。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskSpec {
    pub tool: ToolKind,
    pub title: Option<String>,
    pub message: String,
    pub format: MessageFormat,
    pub options: Vec<OptionSpec>,
    pub selection: Selection,
    pub allow_text: bool,
    pub allow_images: bool,
    /// 交接：验证情况。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Check>,
    /// 交接：风险与待定事项。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub risks: Vec<String>,
}

/// `zhi` 工具的原始参数（与寸止一致）。
#[derive(Debug, Clone, Deserialize)]
pub struct ZhiParams {
    pub message: String,
    #[serde(default)]
    pub predefined_options: Vec<String>,
    #[serde(default = "default_true")]
    pub is_markdown: bool,
}

/// `ask_user` 工具的原始参数。
#[derive(Debug, Clone, Deserialize)]
pub struct AskUserParams {
    #[serde(default)]
    pub title: Option<String>,
    pub message: String,
    #[serde(default)]
    pub format: MessageFormat,
    #[serde(default)]
    pub options: Vec<OptionSpec>,
    #[serde(default)]
    pub selection: Option<Selection>,
    #[serde(default = "default_true")]
    pub allow_text: bool,
    #[serde(default = "default_true")]
    pub allow_images: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpecError {
    #[error("message 不能为空")]
    EmptyMessage,
    #[error("message 超过 {} KiB", limits::MESSAGE_BYTES / 1024)]
    MessageTooLarge,
    #[error("title 超过 {} 个字符", limits::TITLE_CHARS)]
    TitleTooLong,
    #[error("选项超过 {} 个", limits::OPTIONS)]
    TooManyOptions,
    #[error("第 {0} 个选项为空或超过 {max} 个字符", max = limits::OPTION_CHARS)]
    BadOption(usize),
    #[error("选项 ID 重复：{0}")]
    DuplicateOptionId(String),
    #[error("selection 为 none 时不能提供选项；有选项时 selection 不能为 none")]
    SelectionMismatch,
    #[error("没有选项时必须允许文字或图片回复，否则用户无法回答")]
    NoWayToAnswer,
    #[error("最多只能有一个推荐选项")]
    MultipleRecommended,
    #[error("交接必须有标题")]
    MissingTitle,
    #[error("交接需要 1–{} 个建议的下一步", limits::NEXT_STEPS)]
    BadNextSteps,
    #[error("验证最多 {} 项，风险最多 {} 项，每项不超过 {max} 个字符", limits::CHECKS, limits::RISKS, max = limits::OPTION_CHARS)]
    BadHandOffDetail,
}

impl AskSpec {
    pub fn from_zhi(p: ZhiParams) -> Result<Self, SpecError> {
        let options = p
            .predefined_options
            .into_iter()
            .enumerate()
            .map(|(i, label)| OptionSpec {
                id: format!("{}", i + 1),
                label,
                description: None,
                recommended: false,
            })
            .collect::<Vec<_>>();
        let selection = if options.is_empty() {
            Selection::None
        } else {
            // 寸止的预设选项是多选语义。
            Selection::Multiple
        };
        let spec = Self {
            tool: ToolKind::Zhi,
            title: None,
            message: p.message,
            format: if p.is_markdown {
                MessageFormat::Markdown
            } else {
                MessageFormat::Plain
            },
            options,
            selection,
            allow_text: true,
            allow_images: true,
            checks: Vec::new(),
            risks: Vec::new(),
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn from_hand_off(p: HandOffParams) -> Result<Self, SpecError> {
        let title = p.title.trim().to_owned();
        if title.is_empty() {
            return Err(SpecError::MissingTitle);
        }
        if p.next_steps.is_empty() || p.next_steps.len() > limits::NEXT_STEPS {
            return Err(SpecError::BadNextSteps);
        }
        let spec = Self {
            tool: ToolKind::HandOff,
            title: Some(title),
            message: p.summary,
            format: MessageFormat::Markdown,
            options: p
                .next_steps
                .into_iter()
                .map(|n| OptionSpec {
                    id: n.id,
                    label: n.label,
                    description: n
                        .reason
                        .map(|r| r.trim().to_owned())
                        .filter(|r| !r.is_empty()),
                    recommended: n.recommended,
                })
                .collect(),
            selection: Selection::Single,
            allow_text: true,
            allow_images: true,
            checks: p.checks,
            risks: p
                .risks
                .into_iter()
                .map(|r| r.trim().to_owned())
                .filter(|r| !r.is_empty())
                .collect(),
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn recommended(&self) -> Option<&OptionSpec> {
        self.options.iter().find(|o| o.recommended)
    }

    pub fn from_ask_user(p: AskUserParams) -> Result<Self, SpecError> {
        let selection = match (p.selection, p.options.is_empty()) {
            (Some(s), _) => s,
            (None, true) => Selection::None,
            (None, false) => Selection::Single,
        };
        let spec = Self {
            tool: ToolKind::AskUser,
            title: p
                .title
                .map(|t| t.trim().to_owned())
                .filter(|t| !t.is_empty()),
            message: p.message,
            format: p.format,
            options: p.options,
            selection,
            allow_text: p.allow_text,
            allow_images: p.allow_images,
            checks: Vec::new(),
            risks: Vec::new(),
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), SpecError> {
        if self.message.trim().is_empty() {
            return Err(SpecError::EmptyMessage);
        }
        if self.message.len() > limits::MESSAGE_BYTES {
            return Err(SpecError::MessageTooLarge);
        }
        if let Some(t) = &self.title
            && t.chars().count() > limits::TITLE_CHARS
        {
            return Err(SpecError::TitleTooLong);
        }
        if self.options.len() > limits::OPTIONS {
            return Err(SpecError::TooManyOptions);
        }
        let mut seen = HashSet::new();
        for (i, o) in self.options.iter().enumerate() {
            let label_len = o.label.chars().count();
            if o.id.trim().is_empty()
                || o.label.trim().is_empty()
                || label_len > limits::OPTION_CHARS
                || o.id.len() > 128
            {
                return Err(SpecError::BadOption(i + 1));
            }
            if !seen.insert(o.id.as_str()) {
                return Err(SpecError::DuplicateOptionId(o.id.clone()));
            }
        }
        if self.options.iter().filter(|o| o.recommended).count() > 1 {
            return Err(SpecError::MultipleRecommended);
        }
        if self.checks.len() > limits::CHECKS
            || self.risks.len() > limits::RISKS
            || self.checks.iter().any(|c| {
                c.label.trim().is_empty()
                    || c.label.chars().count() > limits::OPTION_CHARS
                    || c.detail
                        .as_ref()
                        .is_some_and(|d| d.chars().count() > limits::OPTION_CHARS)
            })
            || self
                .risks
                .iter()
                .any(|r| r.chars().count() > limits::OPTION_CHARS)
        {
            return Err(SpecError::BadHandOffDetail);
        }
        if (self.selection == Selection::None) != self.options.is_empty() {
            return Err(SpecError::SelectionMismatch);
        }
        if self.options.is_empty() && !self.allow_text && !self.allow_images {
            return Err(SpecError::NoWayToAnswer);
        }
        Ok(())
    }

    /// 详情页的标题与正文：显式 title 原样使用；否则当正文首行是普通句子或 Markdown 标题、
    /// 且不超过 80 字时，把它提出来作为标题，正文去掉这一行，避免重复显示。
    pub fn heading_and_body(&self) -> (Option<String>, String) {
        if let Some(t) = &self.title {
            return (Some(t.clone()), self.message.clone());
        }
        let mut lines = self.message.lines();
        let mut skipped = 0usize;
        let first = loop {
            match lines.next() {
                Some(l) if l.trim().is_empty() => skipped += 1,
                Some(l) => break l.trim(),
                None => return (None, self.message.clone()),
            }
        };
        let is_block = first.starts_with(['-', '*', '+', '>', '|', '`'])
            || first
                .split_once(". ")
                .is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()));
        let text = first.trim_start_matches('#').trim().replace(['*', '`'], "");
        if is_block || text.is_empty() || text.chars().count() > 80 {
            return (None, self.message.clone());
        }
        let rest: Vec<&str> = self.message.lines().skip(skipped + 1).collect();
        (
            Some(text),
            rest.join("\n").trim_start_matches('\n').to_owned(),
        )
    }

    /// 列表中展示的标题：显式 title，否则取正文第一行有内容的文字。
    pub fn display_title(&self) -> String {
        if let Some(t) = &self.title {
            return t.clone();
        }
        let line = self
            .message
            .lines()
            .map(|l| {
                l.trim()
                    .trim_start_matches(['#', '>', '-', '*', ' '])
                    .trim()
            })
            .find(|l| !l.is_empty() && !l.starts_with("```"))
            .unwrap_or("");
        truncate_chars(&line.replace(['*', '`'], ""), 80)
    }
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(options: Vec<(&str, &str)>, selection: Option<Selection>) -> AskUserParams {
        AskUserParams {
            title: None,
            message: "选哪个？".into(),
            format: MessageFormat::Markdown,
            options: options
                .into_iter()
                .map(|(id, label)| OptionSpec {
                    id: id.into(),
                    label: label.into(),
                    description: None,
                    recommended: false,
                })
                .collect(),
            selection,
            allow_text: true,
            allow_images: true,
        }
    }

    #[test]
    fn zhi_options_are_multiple_with_positional_ids() {
        let spec = AskSpec::from_zhi(ZhiParams {
            message: "继续吗".into(),
            predefined_options: vec!["继续".into(), "停下".into()],
            is_markdown: true,
        })
        .expect("valid");
        assert_eq!(spec.selection, Selection::Multiple);
        assert_eq!(spec.options[1].id, "2");
    }

    #[test]
    fn ask_user_defaults_to_single_when_options_present() {
        let spec = AskSpec::from_ask_user(ask(vec![("a", "A"), ("b", "B")], None)).expect("valid");
        assert_eq!(spec.selection, Selection::Single);
    }

    #[test]
    fn rejects_duplicate_ids_and_mismatched_selection() {
        assert_eq!(
            AskSpec::from_ask_user(ask(vec![("a", "A"), ("a", "B")], None)),
            Err(SpecError::DuplicateOptionId("a".into()))
        );
        assert_eq!(
            AskSpec::from_ask_user(ask(vec![], Some(Selection::Single))),
            Err(SpecError::SelectionMismatch)
        );
        assert_eq!(
            AskSpec::from_ask_user(ask(vec![("a", "A")], Some(Selection::None))),
            Err(SpecError::SelectionMismatch)
        );
    }

    fn hand_off(steps: Vec<(&str, bool)>) -> HandOffParams {
        HandOffParams {
            title: "导出脚本完成".into(),
            summary: "写完了导出脚本。".into(),
            checks: vec![Check {
                label: "单元测试".into(),
                status: CheckStatus::Passed,
                detail: None,
            }],
            risks: vec!["  ".into(), "大文件未测".into()],
            next_steps: steps
                .into_iter()
                .map(|(id, rec)| NextStep {
                    id: id.into(),
                    label: format!("做 {id}"),
                    reason: Some(" 因为 ".into()),
                    recommended: rec,
                })
                .collect(),
        }
    }

    #[test]
    fn hand_off_builds_single_choice_with_recommendation() {
        let spec =
            AskSpec::from_hand_off(hand_off(vec![("a", false), ("b", true)])).expect("valid");
        assert_eq!(spec.tool, ToolKind::HandOff);
        assert_eq!(spec.selection, Selection::Single);
        assert_eq!(spec.recommended().map(|o| o.id.as_str()), Some("b"));
        assert_eq!(spec.options[0].description.as_deref(), Some("因为"));
        assert_eq!(spec.risks, vec!["大文件未测".to_owned()]);
    }

    #[test]
    fn hand_off_requires_steps_title_and_one_recommendation() {
        assert_eq!(
            AskSpec::from_hand_off(hand_off(vec![])),
            Err(SpecError::BadNextSteps)
        );
        assert_eq!(
            AskSpec::from_hand_off(hand_off(vec![("a", true), ("b", true)])),
            Err(SpecError::MultipleRecommended)
        );
        let mut p = hand_off(vec![("a", false)]);
        p.title = " ".into();
        assert_eq!(AskSpec::from_hand_off(p), Err(SpecError::MissingTitle));
    }

    #[test]
    fn old_specs_without_new_fields_still_deserialize() {
        let old = r#"{"tool":"ask_user","title":null,"message":"m","format":"markdown","options":[{"id":"a","label":"A"}],"selection":"single","allow_text":true,"allow_images":true}"#;
        let spec: AskSpec = serde_json::from_str(old).expect("compatible");
        assert!(spec.checks.is_empty() && !spec.options[0].recommended);
    }

    #[test]
    fn rejects_unanswerable_request() {
        let mut p = ask(vec![], None);
        p.allow_text = false;
        p.allow_images = false;
        assert_eq!(AskSpec::from_ask_user(p), Err(SpecError::NoWayToAnswer));
    }

    #[test]
    fn rejects_oversized_message() {
        let mut p = ask(vec![], None);
        p.message = "字".repeat(limits::MESSAGE_BYTES);
        assert_eq!(AskSpec::from_ask_user(p), Err(SpecError::MessageTooLarge));
    }

    #[test]
    fn heading_is_lifted_from_plain_first_line_only() {
        let mk = |m: &str| {
            let mut p = ask(vec![], None);
            p.message = m.into();
            AskSpec::from_ask_user(p).expect("valid")
        };
        assert_eq!(
            mk("要合并吗？").heading_and_body(),
            (Some("要合并吗？".into()), String::new())
        );
        assert_eq!(
            mk("## 先确认范围\n\n正文").heading_and_body(),
            (Some("先确认范围".into()), "正文".into())
        );
        assert_eq!(mk("- 列表\n- 项").heading_and_body().0, None);
        assert_eq!(mk("1. 第一步").heading_and_body().0, None);
        assert_eq!(mk(&"长".repeat(81)).heading_and_body().0, None);
    }

    #[test]
    fn display_title_skips_markdown_noise() {
        let mut p = ask(vec![], None);
        p.message = "\n## **先确认** 第一版范围\n正文".into();
        let spec = AskSpec::from_ask_user(p).expect("valid");
        assert_eq!(spec.display_title(), "先确认 第一版范围");
    }
}
