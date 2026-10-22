use crate::error::AtlasResult;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchJob {
    pub id: String,
    pub title: String,
    pub mode: String,
    pub status: String,
    pub phase: String,
    pub question: String,
    pub clarifications: Value,
    pub plan: Value,
    pub report_md: String,
    pub budget: Value,
    pub paused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchEvent {
    pub id: String,
    pub job_id: String,
    pub seq: i64,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub lines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSource {
    pub id: String,
    pub job_id: String,
    pub url: String,
    pub title: String,
    pub fetched_at: String,
    pub quality_tier: String,
    pub fragment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub meta: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchClaim {
    pub id: String,
    pub job_id: String,
    pub claim: String,
    pub source_ids: Vec<String>,
    pub verified: bool,
    pub note: String,
    pub created_at: String,
}

impl Store {
    pub fn migrate_research_schema(&self) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS research_jobs (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                mode TEXT NOT NULL,
                status TEXT NOT NULL,
                phase TEXT NOT NULL,
                question TEXT NOT NULL,
                clarifications_json TEXT NOT NULL DEFAULT '[]',
                plan_json TEXT NOT NULL DEFAULT '{}',
                report_md TEXT NOT NULL DEFAULT '',
                budget_json TEXT NOT NULL DEFAULT '{}',
                paused INTEGER NOT NULL DEFAULT 0,
                conversation_id TEXT,
                task_id TEXT,
                attachment_json TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_research_jobs_updated ON research_jobs(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_research_jobs_status ON research_jobs(status);

            CREATE TABLE IF NOT EXISTS research_events (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                timestamp TEXT NOT NULL,
                event_type TEXT NOT NULL,
                lines_json TEXT NOT NULL,
                details TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY(job_id) REFERENCES research_jobs(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_research_events_job ON research_events(job_id, seq);

            CREATE TABLE IF NOT EXISTS research_sources (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                fetched_at TEXT NOT NULL,
                quality_tier TEXT NOT NULL,
                fragment TEXT NOT NULL,
                content_hash TEXT,
                meta_json TEXT NOT NULL DEFAULT '{}',
                FOREIGN KEY(job_id) REFERENCES research_jobs(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_research_sources_job ON research_sources(job_id);

            CREATE TABLE IF NOT EXISTS research_scratchpad (
                job_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value_json TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(job_id, key),
                FOREIGN KEY(job_id) REFERENCES research_jobs(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS research_claims (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                claim TEXT NOT NULL,
                source_ids_json TEXT NOT NULL DEFAULT '[]',
                verified INTEGER NOT NULL DEFAULT 0,
                note TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                FOREIGN KEY(job_id) REFERENCES research_jobs(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_research_claims_job ON research_claims(job_id);
            ",
        )?;
        Ok(())
    }

    pub fn create_research_job(&self, job: &ResearchJob) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO research_jobs (
                id, title, mode, status, phase, question, clarifications_json, plan_json,
                report_md, budget_json, paused, conversation_id, task_id, attachment_json,
                created_at, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![
                job.id,
                job.title,
                job.mode,
                job.status,
                job.phase,
                job.question,
                serde_json::to_string(&job.clarifications)?,
                serde_json::to_string(&job.plan)?,
                job.report_md,
                serde_json::to_string(&job.budget)?,
                if job.paused { 1 } else { 0 },
                job.conversation_id,
                job.task_id,
                job.attachment
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                job.created_at,
                job.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_research_job(&self, id: &str) -> AtlasResult<Option<ResearchJob>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, title, mode, status, phase, question, clarifications_json, plan_json,
                    report_md, budget_json, paused, conversation_id, task_id, attachment_json,
                    created_at, updated_at
             FROM research_jobs WHERE id = ?1",
        )?;
        Ok(stmt.query_row(params![id], map_job).optional()?)
    }

    pub fn list_research_jobs(&self, limit: usize) -> AtlasResult<Vec<ResearchJob>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 200) as i64;
        let mut stmt = conn.prepare(
            "SELECT id, title, mode, status, phase, question, clarifications_json, plan_json,
                    report_md, budget_json, paused, conversation_id, task_id, attachment_json,
                    created_at, updated_at
             FROM research_jobs ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], map_job)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn update_research_job(&self, job: &ResearchJob) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE research_jobs SET
                title=?2, mode=?3, status=?4, phase=?5, question=?6,
                clarifications_json=?7, plan_json=?8, report_md=?9, budget_json=?10,
                paused=?11, conversation_id=?12, task_id=?13, attachment_json=?14,
                updated_at=?15
             WHERE id=?1",
            params![
                job.id,
                job.title,
                job.mode,
                job.status,
                job.phase,
                job.question,
                serde_json::to_string(&job.clarifications)?,
                serde_json::to_string(&job.plan)?,
                job.report_md,
                serde_json::to_string(&job.budget)?,
                if job.paused { 1 } else { 0 },
                job.conversation_id,
                job.task_id,
                job.attachment
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                job.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn next_research_event_seq(&self, job_id: &str) -> AtlasResult<i64> {
        let conn = self.conn.lock().expect("store lock");
        let max: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM research_events WHERE job_id = ?1",
            params![job_id],
            |r| r.get(0),
        )?;
        Ok(max + 1)
    }

    pub fn append_research_event(
        &self,
        job_id: &str,
        event_type: &str,
        lines: &[String],
        details: Option<&str>,
    ) -> AtlasResult<ResearchEvent> {
        let seq = self.next_research_event_seq(job_id)?;
        let now = Utc::now();
        let event = ResearchEvent {
            id: Uuid::new_v4().to_string(),
            job_id: job_id.into(),
            seq,
            timestamp: now.format("%H:%M:%S").to_string(),
            event_type: event_type.into(),
            lines: lines.to_vec(),
            details: details.map(|s| s.to_string()),
            created_at: now.to_rfc3339(),
        };
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO research_events (id, job_id, seq, timestamp, event_type, lines_json, details, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                event.id,
                event.job_id,
                event.seq,
                event.timestamp,
                event.event_type,
                serde_json::to_string(&event.lines)?,
                event.details,
                event.created_at
            ],
        )?;
        Ok(event)
    }

    pub fn list_research_events(&self, job_id: &str) -> AtlasResult<Vec<ResearchEvent>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, job_id, seq, timestamp, event_type, lines_json, details, created_at
             FROM research_events WHERE job_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![job_id], map_event)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn upsert_research_source(&self, source: &ResearchSource) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO research_sources (
                id, job_id, url, title, fetched_at, quality_tier, fragment, content_hash, meta_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(id) DO UPDATE SET
                title=excluded.title, fragment=excluded.fragment,
                quality_tier=excluded.quality_tier, meta_json=excluded.meta_json,
                content_hash=excluded.content_hash, fetched_at=excluded.fetched_at",
            params![
                source.id,
                source.job_id,
                source.url,
                source.title,
                source.fetched_at,
                source.quality_tier,
                source.fragment,
                source.content_hash,
                serde_json::to_string(&source.meta)?
            ],
        )?;
        Ok(())
    }

    pub fn list_research_sources(&self, job_id: &str) -> AtlasResult<Vec<ResearchSource>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, job_id, url, title, fetched_at, quality_tier, fragment, content_hash, meta_json
             FROM research_sources WHERE job_id = ?1 ORDER BY fetched_at ASC",
        )?;
        let rows = stmt.query_map(params![job_id], map_source)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn scratchpad_set(&self, job_id: &str, key: &str, value: &Value) -> AtlasResult<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO research_scratchpad (job_id, key, value_json, updated_at)
             VALUES (?1,?2,?3,?4)
             ON CONFLICT(job_id, key) DO UPDATE SET value_json=excluded.value_json, updated_at=excluded.updated_at",
            params![job_id, key, serde_json::to_string(value)?, now],
        )?;
        Ok(())
    }

    pub fn scratchpad_get(&self, job_id: &str, key: &str) -> AtlasResult<Option<Value>> {
        let conn = self.conn.lock().expect("store lock");
        let raw: Option<String> = conn
            .query_row(
                "SELECT value_json FROM research_scratchpad WHERE job_id = ?1 AND key = ?2",
                params![job_id, key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    pub fn scratchpad_all(&self, job_id: &str) -> AtlasResult<Value> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT key, value_json FROM research_scratchpad WHERE job_id = ?1 ORDER BY key",
        )?;
        let rows = stmt.query_map(params![job_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut map = serde_json::Map::new();
        for row in rows {
            let (k, v) = row?;
            map.insert(k, serde_json::from_str(&v).unwrap_or(json!(null)));
        }
        Ok(Value::Object(map))
    }

    pub fn insert_research_claim(&self, claim: &ResearchClaim) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO research_claims (id, job_id, claim, source_ids_json, verified, note, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                claim.id,
                claim.job_id,
                claim.claim,
                serde_json::to_string(&claim.source_ids)?,
                if claim.verified { 1 } else { 0 },
                claim.note,
                claim.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_research_claims(&self, job_id: &str) -> AtlasResult<Vec<ResearchClaim>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, job_id, claim, source_ids_json, verified, note, created_at
             FROM research_claims WHERE job_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![job_id], map_claim)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn update_research_claim_verified(
        &self,
        id: &str,
        verified: bool,
        note: &str,
    ) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE research_claims SET verified = ?1, note = ?2 WHERE id = ?3",
            params![if verified { 1 } else { 0 }, note, id],
        )?;
        Ok(())
    }
}

fn map_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchJob> {
    let clarifications: String = row.get(6)?;
    let plan: String = row.get(7)?;
    let budget: String = row.get(9)?;
    let attachment: Option<String> = row.get(13)?;
    Ok(ResearchJob {
        id: row.get(0)?,
        title: row.get(1)?,
        mode: row.get(2)?,
        status: row.get(3)?,
        phase: row.get(4)?,
        question: row.get(5)?,
        clarifications: serde_json::from_str(&clarifications).unwrap_or(json!([])),
        plan: serde_json::from_str(&plan).unwrap_or(json!({})),
        report_md: row.get(8)?,
        budget: serde_json::from_str(&budget).unwrap_or(json!({})),
        paused: row.get::<_, i64>(10)? != 0,
        conversation_id: row.get(11)?,
        task_id: row.get(12)?,
        attachment: attachment.and_then(|s| serde_json::from_str(&s).ok()),
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn map_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchEvent> {
    let lines: String = row.get(5)?;
    Ok(ResearchEvent {
        id: row.get(0)?,
        job_id: row.get(1)?,
        seq: row.get(2)?,
        timestamp: row.get(3)?,
        event_type: row.get(4)?,
        lines: serde_json::from_str(&lines).unwrap_or_default(),
        details: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn map_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchSource> {
    let meta: String = row.get(8)?;
    Ok(ResearchSource {
        id: row.get(0)?,
        job_id: row.get(1)?,
        url: row.get(2)?,
        title: row.get(3)?,
        fetched_at: row.get(4)?,
        quality_tier: row.get(5)?,
        fragment: row.get(6)?,
        content_hash: row.get(7)?,
        meta: serde_json::from_str(&meta).unwrap_or(json!({})),
    })
}

fn map_claim(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchClaim> {
    let source_ids: String = row.get(3)?;
    Ok(ResearchClaim {
        id: row.get(0)?,
        job_id: row.get(1)?,
        claim: row.get(2)?,
        source_ids: serde_json::from_str(&source_ids).unwrap_or_default(),
        verified: row.get::<_, i64>(4)? != 0,
        note: row.get(5)?,
        created_at: row.get(6)?,
    })
}
