//! 用户偏好：保存在数据目录的 prefs.json。不含任何敏感数据。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Prefs {
    /// 快捷回复，⌥1–⌥9 对应前九条。发出的就是这里的文字。
    pub quick_replies: Vec<String>,
    /// 唤出快速回复面板的全局快捷键；空字符串表示关闭。
    pub shortcut: String,
    /// 新请求时发送系统通知。
    pub notify: bool,
    /// 新请求时自动弹出快速回复面板。
    pub popup: bool,
    /// system / light / dark
    pub theme: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            quick_replies: vec![
                "继续".into(),
                "先补测试，再继续".into(),
                "先停下，用 hand_off 交接目前的进展".into(),
            ],
            shortcut: "Control+Alt+Space".into(),
            notify: true,
            popup: false,
            theme: "system".into(),
        }
    }
}

impl Prefs {
    pub fn path() -> Option<PathBuf> {
        Some(turndock_core::paths::data_dir()?.join("prefs.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path().ok_or_else(|| anyhow::anyhow!("无法确定数据目录"))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn sanitized(mut self) -> Self {
        self.quick_replies = self
            .quick_replies
            .into_iter()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .map(|s| turndock_core::spec::truncate_chars(&s, 2000))
            .take(9)
            .collect();
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = "system".into();
        }
        self
    }
}
