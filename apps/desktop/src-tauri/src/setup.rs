//! 接入：检查 `turndock` 命令、检测各 AI 客户端是否已接入、在用户点击后执行接入。
//!
//! 接入命令对所有用户、所有电脑都相同（只引用命令 `turndock mcp`，不含个人路径）。
//! 只有用户在界面上明确点击时才会执行修改；检测只读取配置，不写入。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// 命令行入口：`turndock` 打开窗口，`turndock mcp` 是 MCP 服务。
pub const COMMAND: &str = "turndock";
pub const MCP_ARG: &str = "mcp";
pub const SERVER: &str = "turndock";
/// 命令的默认安装位置：所有 Mac 上都相同，并且在系统默认 PATH（/etc/paths）里。
const SYSTEM_BIN: &str = "/usr/local/bin";

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandState {
    /// 已安装，并指向当前这份回合。
    Ready,
    /// 没有安装。
    Missing,
    /// 已安装，但指向别处（旧版本或开发构建）。
    Elsewhere,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandStatus {
    pub state: CommandState,
    /// 命令所在位置（已安装时）或将要安装的位置。
    pub path: String,
    /// 已安装时实际指向的文件。
    pub target: Option<String>,
    /// 本应用自带的 turndock 命令。
    pub bundled: String,
    pub bundled_found: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostId {
    ClaudeCode,
    Codex,
    /// Cursor 及其他使用 mcpServers JSON 配置的客户端。
    Json,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// 回合替你运行接入命令。
    Run,
    /// 打开客户端自己的安装确认（deeplink）。
    Open,
    /// 只能复制配置。
    Copy,
}

#[derive(Debug, Clone, Serialize)]
pub struct Host {
    pub id: HostId,
    pub name: &'static str,
    /// 本机是否装了这个客户端；未知时为 None。
    pub installed: Option<bool>,
    /// 是否已接入回合；无法判断时为 None。
    pub connected: Option<bool>,
    pub action: Action,
    /// 可直接复制的接入命令或配置。
    pub snippet: String,
    pub language: &'static str,
    /// 经同一套渲染与净化管线生成的高亮 HTML。
    pub html: String,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Setup {
    pub version: &'static str,
    pub command: CommandStatus,
    pub hosts: Vec<Host>,
    pub rules: &'static str,
    pub rules_html: String,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn render(md: &str) -> String {
    turndock_broker::markdown::render(md, turndock_core::MessageFormat::Markdown)
}

/// 应用自带的 bridge：与主程序在同一目录（.app 的 Contents/MacOS，或开发时的 target/debug）。
pub fn bundled_bridge() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(COMMAND)))
        .unwrap_or_else(|| COMMAND.into())
}

/// 安装目录：`TURNDOCK_BIN_DIR` 可覆盖（测试与自定义安装用）。
fn bin_dirs() -> Vec<PathBuf> {
    if let Some(dir) = std::env::var_os("TURNDOCK_BIN_DIR") {
        return vec![PathBuf::from(dir)];
    }
    vec![PathBuf::from(SYSTEM_BIN), home().join(".local/bin")]
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

pub fn command_status() -> CommandStatus {
    let bundled = bundled_bridge();
    let dirs = bin_dirs();
    let installed = dirs
        .iter()
        .map(|d| d.join(COMMAND))
        .find(|p| p.symlink_metadata().is_ok());
    let (state, path, target) = match installed {
        Some(p) => {
            let target = p.canonicalize().ok();
            let state = if same_file(&p, &bundled) {
                CommandState::Ready
            } else {
                CommandState::Elsewhere
            };
            (state, p, target)
        }
        None => (CommandState::Missing, dirs[0].join(COMMAND), None),
    };
    CommandStatus {
        state,
        path: path.display().to_string(),
        target: target.map(|t| t.display().to_string()),
        bundled_found: bundled.is_file(),
        bundled: bundled.display().to_string(),
    }
}

/// 把命令链接到应用自带的 bridge。应用更新时 bridge 随之更新，链接不用变。
/// 目录不可写时弹出系统的管理员授权（只执行 mkdir 与 ln 两条命令）。
fn install_command() -> Result<CommandStatus, String> {
    let bundled = bundled_bridge();
    if !bundled.is_file() {
        return Err(format!("找不到应用自带的 {COMMAND}：{}", bundled.display()));
    }
    let dir = bin_dirs().remove(0);
    let link = dir.join(COMMAND);
    match link_into(&dir, &link, &bundled) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            link_as_admin(&dir, &link, &bundled)?;
        }
        Err(e) => return Err(format!("无法安装到 {}：{e}", dir.display())),
    }
    Ok(command_status())
}

/// 确保 `turndock` 命令可用。`interactive` 为假时只在目录可写时静默创建，不弹授权。
pub fn ensure_command(interactive: bool) -> Result<(), String> {
    // 开发构建不改动系统目录，除非显式指定了 TURNDOCK_BIN_DIR。
    if cfg!(debug_assertions) && std::env::var_os("TURNDOCK_BIN_DIR").is_none() {
        return Ok(());
    }
    let status = command_status();
    if matches!(status.state, CommandState::Ready) || !status.bundled_found {
        return Ok(());
    }
    if interactive {
        return install_command().map(|_| ());
    }
    let dir = bin_dirs().remove(0);
    link_into(&dir, &dir.join(COMMAND), &bundled_bridge()).map_err(|e| e.to_string())
}

fn link_into(dir: &Path, link: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    if let Ok(meta) = link.symlink_metadata() {
        if meta.is_dir() {
            return Err(std::io::Error::other(format!(
                "{} 是一个目录",
                link.display()
            )));
        }
        std::fs::remove_file(link)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn link_as_admin(dir: &Path, link: &Path, target: &Path) -> Result<(), String> {
    let script = format!(
        "do shell script \"/bin/mkdir -p \" & quoted form of {dir} & \" && /bin/ln -sfn \" & quoted form of {target} & \" \" & quoted form of {link} with prompt \"回合要把 {COMMAND} 命令安装到 {shown}。\" with administrator privileges",
        dir = applescript_string(&dir.display().to_string()),
        target = applescript_string(&target.display().to_string()),
        link = applescript_string(&link.display().to_string()),
        shown = dir.display(),
    );
    let out = run(
        Path::new("/usr/bin/osascript"),
        &["-e", &script],
        None,
        Duration::from_secs(120),
    )?;
    if out.ok {
        Ok(())
    } else if out.stderr.contains("-128") {
        Err("已取消授权，命令没有安装".into())
    } else {
        Err(format!("安装失败：{}", out.stderr.trim()))
    }
}

// ---------- 查找客户端 ----------

/// 图形应用从 launchd 启动，PATH 很短。取登录 shell 的 PATH，再补上常见安装位置。
fn search_path() -> &'static [PathBuf] {
    static PATHS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    PATHS.get_or_init(|| {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        if let Ok(out) = run(
            Path::new(&shell),
            &["-lc", "printf %s \"$PATH\""],
            None,
            Duration::from_secs(3),
        ) && out.ok
        {
            dirs.extend(std::env::split_paths(out.stdout.trim()));
        }
        if let Some(p) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&p));
        }
        let h = home();
        dirs.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            h.join(".local/bin"),
            h.join(".claude/local"),
            h.join(".npm-global/bin"),
            h.join(".bun/bin"),
            h.join(".volta/bin"),
        ]);
        let mut seen = std::collections::HashSet::new();
        dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));
        dirs
    })
}

fn find_exe(name: &str) -> Option<PathBuf> {
    search_path()
        .iter()
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

fn joined_path() -> String {
    std::env::join_paths(search_path())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

// ---------- 检测是否已接入（只读） ----------

fn json_has_server(path: &Path) -> Option<bool> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(false),
        Err(_) => return None,
    };
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    Some(v.get("mcpServers").and_then(|s| s.get(SERVER)).is_some())
}

fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".codex"))
}

fn toml_has_server(path: &Path) -> Option<bool> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(false),
        Err(_) => return None,
    };
    let headers = [
        format!("[mcp_servers.{SERVER}]"),
        format!("[mcp_servers.\"{SERVER}\"]"),
    ];
    Some(text.lines().any(|l| headers.iter().any(|h| l.trim() == h)))
}

fn cursor_installed() -> bool {
    Path::new("/Applications/Cursor.app").exists()
        || home().join("Applications/Cursor.app").exists()
        || find_exe("cursor").is_some()
}

/// Cursor 的一键安装链接：由 Cursor 自己弹出确认。
pub fn cursor_deeplink() -> String {
    let config = format!("{{\"command\":\"{COMMAND}\",\"args\":[\"{MCP_ARG}\"]}}");
    format!(
        "cursor://anysphere.cursor-deeplink/mcp/install?name={SERVER}&config={}",
        base64(config.as_bytes())
    )
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn claude_command() -> String {
    format!("claude mcp add --scope user {SERVER} -- {COMMAND} {MCP_ARG}")
}

pub fn codex_command() -> String {
    format!("codex mcp add {SERVER} -- {COMMAND} {MCP_ARG}")
}

pub fn json_config() -> String {
    format!(
        "{{\n  \"mcpServers\": {{\n    \"{SERVER}\": {{\n      \"command\": \"{COMMAND}\",\n      \"args\": [\"{MCP_ARG}\"]\n    }}\n  }}\n}}"
    )
}

#[allow(clippy::too_many_arguments)]
fn host(
    id: HostId,
    name: &'static str,
    installed: Option<bool>,
    connected: Option<bool>,
    action: Action,
    snippet: String,
    language: &'static str,
    note: &'static str,
) -> Host {
    let html = render(&format!("```{language}\n{snippet}\n```"));
    Host {
        id,
        name,
        installed,
        connected,
        action,
        snippet,
        language,
        html,
        note,
    }
}

pub fn hosts() -> Vec<Host> {
    let h = home();
    vec![
        host(
            HostId::ClaudeCode,
            "Claude Code",
            Some(find_exe("claude").is_some()),
            json_has_server(&h.join(".claude.json")),
            Action::Run,
            claude_command(),
            "bash",
            "接入到用户范围，所有项目可用。重启 Claude Code 会话后生效。",
        ),
        host(
            HostId::Codex,
            "Codex",
            Some(find_exe("codex").is_some()),
            toml_has_server(&codex_home().join("config.toml")),
            Action::Run,
            codex_command(),
            "bash",
            "Codex 默认 60 秒工具超时：回合每 50 秒返回一次 pending，AI 会自动续等，无需改配置。",
        ),
        {
            let cursor = cursor_installed();
            host(
                HostId::Json,
                "Cursor 及其他客户端",
                None,
                json_has_server(&h.join(".cursor/mcp.json")),
                if cursor { Action::Open } else { Action::Copy },
                json_config(),
                "json",
                if cursor {
                    "「在 Cursor 中接入」由 Cursor 弹出确认。其他支持 stdio MCP 的客户端，把这段配置加进它的 MCP 设置。"
                } else {
                    "把这段配置加进客户端的 MCP 设置（例如 ~/.cursor/mcp.json）。"
                },
            )
        },
    ]
}

pub fn build() -> Setup {
    Setup {
        version: env!("CARGO_PKG_VERSION"),
        command: command_status(),
        hosts: hosts(),
        rules: RULES,
        rules_html: render(RULES),
    }
}

// ---------- 接入（用户点击后执行） ----------

pub fn connect(id: HostId) -> Result<(), String> {
    // 接入的配置引用 `turndock` 命令；拖拽安装的用户此时才需要授权一次。
    ensure_command(true)?;
    let (exe, args): (&str, Vec<&str>) = match id {
        HostId::ClaudeCode => (
            "claude",
            vec![
                "mcp", "add", "--scope", "user", SERVER, "--", COMMAND, MCP_ARG,
            ],
        ),
        HostId::Codex => ("codex", vec!["mcp", "add", SERVER, "--", COMMAND, MCP_ARG]),
        HostId::Json => return Err("这个客户端需要手动接入".into()),
    };
    let path = find_exe(exe).ok_or_else(|| format!("没有找到 {exe} 命令"))?;
    let out = run(&path, &args, Some(&joined_path()), Duration::from_secs(30))?;
    if out.ok {
        Ok(())
    } else {
        let msg = if out.stderr.trim().is_empty() {
            out.stdout
        } else {
            out.stderr
        };
        Err(format!("{exe} 返回错误：{}", msg.trim()))
    }
}

struct Output {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// 运行外部命令，超时即结束它。输出只用于界面提示，不写日志。
fn run(
    exe: &Path,
    args: &[&str],
    path_env: Option<&str>,
    timeout: Duration,
) -> Result<Output, String> {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(p) = path_env {
        cmd.env("PATH", p);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("无法运行 {}：{e}", exe.display()))?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} 超过 {} 秒没有结束",
                    exe.display(),
                    timeout.as_secs()
                ));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = s.read_to_string(&mut stdout);
    }
    if let Some(mut s) = child.stderr.take() {
        let _ = s.read_to_string(&mut stderr);
    }
    Ok(Output {
        ok: status.success(),
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(
            base64(b"{\"command\":\"turndock-mcp\"}"),
            "eyJjb21tYW5kIjoidHVybmRvY2stbWNwIn0="
        );
    }

    #[test]
    fn snippets_are_the_same_for_everyone() {
        for h in hosts() {
            assert!(!h.snippet.contains("/Users/"), "{}", h.snippet);
            assert!(h.snippet.contains(COMMAND));
            assert!(h.html.contains("hl-"), "未高亮：{}", h.html);
        }
        assert!(cursor_deeplink().contains("config=eyJ"));
    }

    #[test]
    fn detects_servers_in_configs() {
        let dir = tempfile::tempdir().expect("测试");
        let json = dir.path().join("mcp.json");
        assert_eq!(json_has_server(&json), Some(false));
        std::fs::write(
            &json,
            r#"{"mcpServers":{"turndock":{"command":"turndock-mcp"}}}"#,
        )
        .expect("测试");
        assert_eq!(json_has_server(&json), Some(true));
        std::fs::write(&json, "not json").expect("测试");
        assert_eq!(json_has_server(&json), None);

        let toml = dir.path().join("config.toml");
        std::fs::write(&toml, "[mcp_servers.other]\ncommand = \"x\"\n").expect("测试");
        assert_eq!(toml_has_server(&toml), Some(false));
        std::fs::write(
            &toml,
            "model = \"o3\"\n\n[mcp_servers.turndock]\ncommand = \"turndock-mcp\"\n",
        )
        .expect("测试");
        assert_eq!(toml_has_server(&toml), Some(true));
    }

    #[test]
    fn links_and_relinks_the_command() {
        let dir = tempfile::tempdir().expect("测试");
        let bin = dir.path().join("bin");
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::write(&a, "a").expect("测试");
        std::fs::write(&b, "b").expect("测试");
        let link = bin.join(COMMAND);
        link_into(&bin, &link, &a).expect("测试");
        assert!(same_file(&link, &a));
        // 指向旧位置时直接替换。
        link_into(&bin, &link, &b).expect("测试");
        assert!(same_file(&link, &b));
    }

    /// 为浏览器预览生成示例数据：`TURNDOCK_DUMP_SETUP=路径 cargo test -p turndock-desktop dump_setup -- --ignored`
    #[test]
    #[ignore]
    fn dump_setup() {
        if let Some(path) = std::env::var_os("TURNDOCK_DUMP_SETUP") {
            let json = serde_json::to_string_pretty(&build()).expect("序列化");
            std::fs::write(path, json).expect("写入");
        }
    }
}
