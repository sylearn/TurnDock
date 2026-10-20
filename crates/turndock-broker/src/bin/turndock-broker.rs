//! 无界面的开发版协调器：在终端里查看请求并回复。桌面应用完成前用于真实宿主联调。
//!
//! 命令：
//!   l                 列出请求
//!   s <n>             查看第 n 条
//!   r <n> <文字>      回复文字
//!   o <n> <id,id> [文字]  选择选项（可附文字）
//!   d <n> [理由]      拒绝
//!   x <n>             关闭（不回复）

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, BufReader};
use turndock_broker::view::{RequestSummary, SubmitReply};
use turndock_broker::{BrokerConfig, BrokerHandle};
use turndock_core::{Outcome, Phase, SubmissionId};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("TURNDOCK_LOG")
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    let config = BrokerConfig::from_env()?;
    println!("TurnDock 开发版协调器");
    println!("  socket：{}", config.socket_path.display());
    if let Some(db) = &config.db_path {
        println!("  数据库：{}", db.display());
    }
    let broker = turndock_broker::spawn(config).await?;

    let mut events = broker.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = events.recv().await {
            if let turndock_broker::view::BrokerEvent::NewRequest {
                client_name,
                project,
                title,
                ..
            } = ev
            {
                println!(
                    "\n● 新请求 · {client_name} · {} · {title}\n  输入 l 查看",
                    project.unwrap_or_default()
                );
            }
        }
    });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        if let Err(e) = command(&broker, line.trim()).await {
            println!("  ✗ {e}");
        }
    }
    Ok(())
}

async fn waiting(broker: &BrokerHandle) -> Result<Vec<RequestSummary>> {
    let mut list: Vec<_> = broker
        .snapshot()
        .await?
        .requests
        .into_iter()
        .filter(|r| r.phase == Phase::Waiting)
        .collect();
    list.sort_by_key(|r| r.created_at);
    Ok(list)
}

async fn command(broker: &BrokerHandle, line: &str) -> Result<()> {
    let mut parts = line.splitn(3, ' ');
    let cmd = parts.next().unwrap_or("");
    let list = waiting(broker).await?;
    let pick = |s: Option<&str>| -> Result<RequestSummary> {
        let n: usize = s
            .unwrap_or("")
            .parse()
            .map_err(|_| anyhow::anyhow!("需要序号"))?;
        list.get(n.wrapping_sub(1))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("没有第 {n} 条"))
    };
    let submit = |r: &RequestSummary, outcome, ids: Vec<String>, text: &str| SubmitReply {
        request_id: r.id.clone(),
        submission_id: SubmissionId(uuid::Uuid::new_v4().to_string()),
        outcome,
        selected_ids: ids,
        text: text.to_owned(),
        images: vec![],
    };
    match cmd {
        "" => {}
        "l" => {
            if list.is_empty() {
                println!("  暂时没有待回复的请求");
            }
            for (i, r) in list.iter().enumerate() {
                println!(
                    "  {}. [{}] {} · {} {}",
                    i + 1,
                    r.client_name,
                    r.project.clone().unwrap_or_default(),
                    r.title,
                    if r.attached { "" } else { "（未在等待）" }
                );
            }
        }
        "s" => {
            let r = pick(parts.next())?;
            let d = broker
                .detail(r.id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("已不存在"))?;
            println!("\n{}\n", d.message_text);
            for o in &d.options {
                println!("  [{}] {}", o.id, o.label);
            }
        }
        "r" => {
            let r = pick(parts.next())?;
            let text = parts.next().unwrap_or("");
            let s = broker
                .submit(submit(&r, Outcome::Answered, vec![], text))
                .await?;
            println!("  ✓ {s:?}");
        }
        "o" => {
            let r = pick(parts.next())?;
            let rest = parts.next().unwrap_or("");
            let (ids, text) = rest.split_once(' ').unwrap_or((rest, ""));
            let ids = ids
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            let s = broker
                .submit(submit(&r, Outcome::Answered, ids, text))
                .await?;
            println!("  ✓ {s:?}");
        }
        "d" => {
            let r = pick(parts.next())?;
            let text = parts.next().unwrap_or("");
            let s = broker
                .submit(submit(&r, Outcome::Declined, vec![], text))
                .await?;
            println!("  ✓ 已拒绝 {s:?}");
        }
        "x" => {
            let r = pick(parts.next())?;
            broker.dismiss(r.id).await?;
            println!("  ✓ 已关闭");
        }
        _ => println!("  命令：l / s n / r n 文字 / o n id,id [文字] / d n [理由] / x n"),
    }
    Ok(())
}
