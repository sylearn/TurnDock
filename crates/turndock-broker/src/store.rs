//! SQLite 持久化。只被协调器 actor 使用（唯一写入者）。

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use turndock_core::{AskSpec, Delivery, Outcome, Phase, RequestId, RequestState, SubmissionId};

use crate::view::Draft;

const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE requests (
    id             TEXT PRIMARY KEY,
    client_name    TEXT NOT NULL,
    client_version TEXT NOT NULL,
    project_path   TEXT,
    spec_json      TEXT NOT NULL,
    phase          TEXT NOT NULL,
    delivery       TEXT NOT NULL,
    submission_id  TEXT,
    decision_json  TEXT,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    deadline_at    INTEGER NOT NULL,
    decided_at     INTEGER
);
CREATE INDEX requests_updated ON requests(updated_at);
CREATE TABLE drafts (
    request_id    TEXT PRIMARY KEY REFERENCES requests(id) ON DELETE CASCADE,
    text          TEXT NOT NULL,
    selected_json TEXT NOT NULL,
    updated_at    INTEGER NOT NULL
);
CREATE TABLE attachments (
    id         INTEGER PRIMARY KEY,
    request_id TEXT NOT NULL REFERENCES requests(id) ON DELETE CASCADE,
    position   INTEGER NOT NULL,
    mime       TEXT NOT NULL,
    data       BLOB NOT NULL
);
CREATE INDEX attachments_request ON attachments(request_id, position);
"#];

/// 保存到库里的决定（不含图片）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredDecision {
    pub outcome: Outcome,
    pub selected_ids: Vec<String>,
    pub text: String,
    pub image_count: u32,
    pub decided_at: i64,
}

#[derive(Debug, Clone)]
pub struct RequestRow {
    pub id: RequestId,
    pub client_name: String,
    pub client_version: String,
    pub project_path: Option<String>,
    pub spec: AskSpec,
    pub state: RequestState,
    pub decision: Option<StoredDecision>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deadline_at: i64,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("创建目录 {}", dir.display()))?;
            // 数据目录只允许本用户访问。
            std::fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
        }
        let conn =
            Connection::open(path).with_context(|| format!("打开数据库 {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let target = MIGRATIONS.len() as i64;
        if version > target {
            anyhow::bail!("数据库版本 {version} 高于本程序支持的 {target}，请升级 TurnDock");
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(Self { conn })
    }

    pub fn insert_request(&self, row: &RequestRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO requests (id, client_name, client_version, project_path, spec_json, phase,
                delivery, submission_id, decision_json, created_at, updated_at, deadline_at, decided_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, ?8, ?8, ?9, NULL)",
            params![
                row.id.as_str(),
                row.client_name,
                row.client_version,
                row.project_path,
                serde_json::to_string(&row.spec)?,
                row.state.phase.as_str(),
                row.state.delivery.as_str(),
                row.created_at,
                row.deadline_at,
            ],
        )?;
        Ok(())
    }

    /// 更新阶段与交付状态。
    pub fn update_state(&self, id: &RequestId, state: &RequestState, now: i64) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE requests SET phase = ?2, delivery = ?3, updated_at = ?4 WHERE id = ?1",
            params![
                id.as_str(),
                state.phase.as_str(),
                state.delivery.as_str(),
                now
            ],
        )?;
        anyhow::ensure!(n == 1, "请求 {id} 不存在");
        Ok(())
    }

    /// 在一个事务中写入决定、图片与新状态。
    pub fn record_decision(
        &self,
        id: &RequestId,
        state: &RequestState,
        submission: &SubmissionId,
        decision: &StoredDecision,
        images: &[(&str, &[u8])],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let n = tx.execute(
            "UPDATE requests SET phase = ?2, delivery = ?3, submission_id = ?4, decision_json = ?5,
                updated_at = ?6, decided_at = ?6 WHERE id = ?1 AND phase = 'waiting'",
            params![
                id.as_str(),
                state.phase.as_str(),
                state.delivery.as_str(),
                submission.0,
                serde_json::to_string(decision)?,
                decision.decided_at,
            ],
        )?;
        anyhow::ensure!(n == 1, "请求 {id} 不在等待状态");
        for (pos, (mime, data)) in images.iter().enumerate() {
            tx.execute(
                "INSERT INTO attachments (request_id, position, mime, data) VALUES (?1, ?2, ?3, ?4)",
                params![id.as_str(), pos as i64, mime, data],
            )?;
        }
        tx.execute(
            "DELETE FROM drafts WHERE request_id = ?1",
            params![id.as_str()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn load_images(&self, id: &RequestId) -> Result<Vec<(String, Vec<u8>)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT mime, data FROM attachments WHERE request_id = ?1 ORDER BY position",
        )?;
        let rows = stmt
            .query_map(params![id.as_str()], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn save_draft(&self, id: &RequestId, draft: &Draft, now: i64) -> Result<()> {
        if draft.text.is_empty() && draft.selected_ids.is_empty() {
            self.conn.execute(
                "DELETE FROM drafts WHERE request_id = ?1",
                params![id.as_str()],
            )?;
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO drafts (request_id, text, selected_json, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(request_id) DO UPDATE SET text = ?2, selected_json = ?3, updated_at = ?4",
            params![
                id.as_str(),
                draft.text,
                serde_json::to_string(&draft.selected_ids)?,
                now
            ],
        )?;
        Ok(())
    }

    pub fn load_draft(&self, id: &RequestId) -> Result<Option<Draft>> {
        let row = self
            .conn
            .query_row(
                "SELECT text, selected_json FROM drafts WHERE request_id = ?1",
                params![id.as_str()],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        Ok(match row {
            Some((text, sel)) => Some(Draft {
                text,
                selected_ids: serde_json::from_str(&sel)?,
            }),
            None => None,
        })
    }

    pub fn draft_ids(&self) -> Result<std::collections::HashSet<RequestId>> {
        let mut stmt = self.conn.prepare_cached("SELECT request_id FROM drafts")?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .map(|r| r.map(RequestId))
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// 读取保留期内的全部请求。
    pub fn load_recent(&self, since: i64) -> Result<Vec<RequestRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, client_name, client_version, project_path, spec_json, phase, delivery,
                submission_id, decision_json, created_at, updated_at, deadline_at
             FROM requests WHERE updated_at >= ?1 OR phase = 'waiting' ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![since], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, i64>(9)?,
                r.get::<_, i64>(10)?,
                r.get::<_, i64>(11)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, cn, cv, pp, spec, phase, delivery, sub, decision, created, updated, deadline) =
                row?;
            out.push(RequestRow {
                id: RequestId(id),
                client_name: cn,
                client_version: cv,
                project_path: pp,
                spec: serde_json::from_str(&spec)?,
                state: RequestState {
                    phase: Phase::parse(&phase).context("未知 phase")?,
                    delivery: Delivery::parse(&delivery).context("未知 delivery")?,
                    attached: None,
                    submission: sub.map(SubmissionId),
                },
                decision: decision.map(|d| serde_json::from_str(&d)).transpose()?,
                created_at: created,
                updated_at: updated,
                deadline_at: deadline,
            });
        }
        Ok(out)
    }

    /// 删除保留期之前已经结束的请求（含回复、草稿、图片）。
    pub fn purge_finished_before(&self, cutoff: i64) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM requests WHERE updated_at < ?1 AND phase != 'waiting'",
            params![cutoff],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use turndock_core::{AskUserParams, MessageFormat, Selection};

    fn row(id: &str) -> RequestRow {
        let spec = AskSpec::from_ask_user(AskUserParams {
            title: None,
            message: "问".into(),
            format: MessageFormat::Markdown,
            options: vec![],
            selection: Some(Selection::None),
            allow_text: true,
            allow_images: false,
        })
        .expect("spec");
        RequestRow {
            id: RequestId(id.into()),
            client_name: "codex".into(),
            client_version: "1".into(),
            project_path: Some("/p".into()),
            spec,
            state: RequestState {
                phase: Phase::Waiting,
                delivery: Delivery::None,
                attached: None,
                submission: None,
            },
            decision: None,
            created_at: 10,
            updated_at: 10,
            deadline_at: 100,
        }
    }

    #[test]
    fn decision_is_atomic_and_only_once() {
        let s = Store::open_in_memory().expect("open");
        let r = row("a");
        s.insert_request(&r).expect("insert");
        s.save_draft(
            &r.id,
            &Draft {
                text: "草稿".into(),
                selected_ids: vec![],
            },
            11,
        )
        .expect("draft");
        let mut st = r.state.clone();
        st.phase = Phase::Decided;
        st.delivery = Delivery::Pending;
        let d = StoredDecision {
            outcome: Outcome::Answered,
            selected_ids: vec![],
            text: "好".into(),
            image_count: 1,
            decided_at: 12,
        };
        let sub = SubmissionId("s".into());
        s.record_decision(&r.id, &st, &sub, &d, &[("image/png", b"x")])
            .expect("decide");
        assert!(s.record_decision(&r.id, &st, &sub, &d, &[]).is_err());
        assert_eq!(s.load_images(&r.id).expect("img").len(), 1);
        assert_eq!(s.load_draft(&r.id).expect("draft"), None);
        let loaded = s.load_recent(0).expect("load");
        assert_eq!(
            loaded[0].decision.as_ref().map(|d| d.text.as_str()),
            Some("好")
        );
    }

    #[test]
    fn purge_keeps_waiting_requests() {
        let s = Store::open_in_memory().expect("open");
        s.insert_request(&row("a")).expect("insert");
        let mut b = row("b");
        b.state.phase = Phase::Expired;
        s.insert_request(&b).expect("insert");
        assert_eq!(s.purge_finished_before(1000).expect("purge"), 1);
        assert_eq!(s.load_recent(0).expect("load").len(), 1);
    }

    #[test]
    fn reopening_file_keeps_data() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("t.db");
        Store::open(&path)
            .expect("open")
            .insert_request(&row("a"))
            .expect("insert");
        assert_eq!(
            Store::open(&path)
                .expect("reopen")
                .load_recent(0)
                .expect("load")
                .len(),
            1
        );
    }
}
