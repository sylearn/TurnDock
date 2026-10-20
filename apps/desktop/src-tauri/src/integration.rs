//! 接入说明：bridge 路径、各宿主的配置片段与规则片段。只生成文本，不修改任何外部配置。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Snippet {
    pub host: &'static str,
    pub file: &'static str,
    pub language: &'static str,
    pub content: String,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Integration {
    pub bridge_path: String,
    pub bridge_found: bool,
    pub snippets: Vec<Snippet>,
    pub rules: String,
}

pub const RULES: &str = "\
## 与用户确认（TurnDock）

- 需要用户决定时（需求不明确、有多个可行方案、即将做有影响的操作），调用 `ask_user` 并给出选项，不要自作主张。
- 每完成一段工作、准备结束本轮回复之前，调用 `ask_user` 请用户确认下一步；没有得到明确答复前不要结束回合。
- 工具返回 pending 时，立即调用 `await_reply` 并传入 request_id 继续等待。
- pending、超时、取消、关闭都不是同意；用户拒绝时停止相关操作并说明。
";

fn bridge_path() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent()
        .map(|d| d.join("turndock-mcp"))
        .unwrap_or_else(|| "turndock-mcp".into())
}

fn json_escape(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
}

pub fn build() -> Integration {
    let path = bridge_path();
    let p = path.display().to_string();
    let pj = json_escape(&p);
    let snippets = vec![
        Snippet {
            host: "Claude Code",
            file: "终端",
            language: "shell",
            content: format!("claude mcp add --scope user turndock -- '{p}'"),
            note: "Claude Code 的 stdio 空闲超时为 30 分钟，TurnDock 每 25 分钟返回一次 pending，AI 会自动续等。",
        },
        Snippet {
            host: "Codex",
            file: "~/.codex/config.toml",
            language: "toml",
            content: format!(
                "[mcp_servers.turndock]\ncommand = {pj}\ntool_timeout_sec = 3600\nenv = {{ TURNDOCK_WAIT_SECS = \"3300\" }}"
            ),
            note: "Codex 默认 60 秒超时。调大 tool_timeout_sec 并让 TURNDOCK_WAIT_SECS 略小于它，可减少续等次数。",
        },
        Snippet {
            host: "Cursor",
            file: "~/.cursor/mcp.json",
            language: "json",
            content: format!(
                "{{\n  \"mcpServers\": {{\n    \"turndock\": {{ \"command\": {pj} }}\n  }}\n}}"
            ),
            note: "Cursor 的超时尚未实测，默认每 50 秒续等一次。",
        },
    ];
    Integration {
        bridge_found: path.exists(),
        bridge_path: p,
        snippets,
        rules: RULES.to_owned(),
    }
}
