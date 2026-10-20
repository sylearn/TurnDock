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
## 与用户交接（TurnDock）

- 做了实际工作的一轮（改了文件、运行了命令、完成了任务）结束前，调用 `hand_off` 交接，在用户回复前不要结束：
  - title：一句话说明结果；summary：做了什么、关键决定、产出在哪里
  - checks：如实区分 passed / failed / not_run / manual，没跑就写 not_run
  - risks：风险、假设与待定事项
  - next_steps：2–4 个具体可执行的下一步，恰好一个推荐，并写明理由与代价
- 纯问答不需要交接。工作中途遇到多个可行方案或有影响的操作时，用 `ask_user` 询问并标出推荐。
- 工具返回 pending 时立即调用 `await_reply` 继续等待；pending、超时、取消、关闭都不是同意。
- 用户选定方向后按它执行；用户要求先停下时，说明当前状态并等待新的指示。
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
