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
    AskUser,
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
        };
        spec.validate()?;
        Ok(spec)
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
        if (self.selection == Selection::None) != self.options.is_empty() {
            return Err(SpecError::SelectionMismatch);
        }
        if self.options.is_empty() && !self.allow_text && !self.allow_images {
            return Err(SpecError::NoWayToAnswer);
        }
        Ok(())
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
    fn display_title_skips_markdown_noise() {
        let mut p = ask(vec![], None);
        p.message = "\n## **先确认** 第一版范围\n正文".into();
        let spec = AskSpec::from_ask_user(p).expect("valid");
        assert_eq!(spec.display_title(), "先确认 第一版范围");
    }
}
