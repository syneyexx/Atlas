use crate::error::AtlasResult;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub suite: String,
    pub name: String,
    pub query: String,
    #[serde(default)]
    pub expect: Value,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRun {
    pub id: String,
    pub suite: String,
    pub case_id: String,
    pub score: f64,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub detail: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackRecord {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// up | down | correction
    pub kind: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub correction: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingExample {
    pub id: String,
    pub source: String,
    pub input: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub meta: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticCacheEntry {
    pub cache_key: String,
    pub response: String,
    pub model_id: String,
    pub hit_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl Store {
    pub fn migrate_eval_schema(&self) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS eval_cases (
                id TEXT PRIMARY KEY,
                suite TEXT NOT NULL,
                name TEXT NOT NULL,
                query TEXT NOT NULL,
                expect_json TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_eval_suite ON eval_cases(suite);

            CREATE TABLE IF NOT EXISTS eval_runs (
                id TEXT PRIMARY KEY,
                suite TEXT NOT NULL,
                case_id TEXT NOT NULL,
                score REAL NOT NULL,
                passed INTEGER NOT NULL,
                trace_id TEXT,
                detail_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_eval_runs_suite ON eval_runs(suite, created_at DESC);

            CREATE TABLE IF NOT EXISTS feedback (
                id TEXT PRIMARY KEY,
                trace_id TEXT,
                conversation_id TEXT,
                kind TEXT NOT NULL,
                note TEXT NOT NULL,
                correction TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_feedback_trace ON feedback(trace_id);

            CREATE TABLE IF NOT EXISTS training_examples (
                id TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                input TEXT NOT NULL,
                output TEXT NOT NULL,
                trace_id TEXT,
                meta_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS semantic_cache (
                cache_key TEXT PRIMARY KEY,
                response TEXT NOT NULL,
                model_id TEXT NOT NULL,
                hit_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    pub fn upsert_eval_case(&self, case: &EvalCase) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO eval_cases (id, suite, name, query, expect_json, tags_json, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(id) DO UPDATE SET
               suite=excluded.suite,
               name=excluded.name,
               query=excluded.query,
               expect_json=excluded.expect_json,
               tags_json=excluded.tags_json",
            params![
                case.id,
                case.suite,
                case.name,
                case.query,
                serde_json::to_string(&case.expect)?,
                serde_json::to_string(&case.tags)?,
                case.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_eval_cases(&self, suite: Option<&str>) -> AtlasResult<Vec<EvalCase>> {
        let conn = self.conn.lock().expect("store lock");
        let mut out = Vec::new();
        if let Some(s) = suite {
            let mut stmt = conn.prepare(
                "SELECT id, suite, name, query, expect_json, tags_json, created_at
                 FROM eval_cases WHERE suite = ?1 ORDER BY name",
            )?;
            let rows = stmt.query_map(params![s], map_case)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, suite, name, query, expect_json, tags_json, created_at
                 FROM eval_cases ORDER BY suite, name",
            )?;
            let rows = stmt.query_map([], map_case)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn insert_eval_run(&self, run: &EvalRun) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO eval_runs (id, suite, case_id, score, passed, trace_id, detail_json, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                run.id,
                run.suite,
                run.case_id,
                run.score,
                if run.passed { 1 } else { 0 },
                run.trace_id,
                serde_json::to_string(&run.detail)?,
                run.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_eval_runs(&self, suite: Option<&str>, limit: usize) -> AtlasResult<Vec<EvalRun>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 200) as i64;
        let mut out = Vec::new();
        if let Some(s) = suite {
            let mut stmt = conn.prepare(
                "SELECT id, suite, case_id, score, passed, trace_id, detail_json, created_at
                 FROM eval_runs WHERE suite = ?1 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![s, limit], map_run)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, suite, case_id, score, passed, trace_id, detail_json, created_at
                 FROM eval_runs ORDER BY created_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], map_run)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn insert_feedback(&self, fb: &FeedbackRecord) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO feedback (id, trace_id, conversation_id, kind, note, correction, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                fb.id,
                fb.trace_id,
                fb.conversation_id,
                fb.kind,
                fb.note,
                fb.correction,
                fb.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_feedback(&self, limit: usize) -> AtlasResult<Vec<FeedbackRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 200) as i64;
        let mut stmt = conn.prepare(
            "SELECT id, trace_id, conversation_id, kind, note, correction, created_at
             FROM feedback ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(FeedbackRecord {
                id: row.get(0)?,
                trace_id: row.get(1)?,
                conversation_id: row.get(2)?,
                kind: row.get(3)?,
                note: row.get(4)?,
                correction: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn insert_training_example(&self, ex: &TrainingExample) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO training_examples (id, source, input, output, trace_id, meta_json, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                ex.id,
                ex.source,
                ex.input,
                ex.output,
                ex.trace_id,
                serde_json::to_string(&ex.meta)?,
                ex.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_training_examples(&self, limit: usize) -> AtlasResult<Vec<TrainingExample>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 500) as i64;
        let mut stmt = conn.prepare(
            "SELECT id, source, input, output, trace_id, meta_json, created_at
             FROM training_examples ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(TrainingExample {
                id: row.get(0)?,
                source: row.get(1)?,
                input: row.get(2)?,
                output: row.get(3)?,
                trace_id: row.get(4)?,
                meta: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or(Value::Null),
                created_at: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn cache_get(&self, key: &str) -> AtlasResult<Option<SemanticCacheEntry>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT cache_key, response, model_id, hit_count, created_at, updated_at
             FROM semantic_cache WHERE cache_key = ?1",
        )?;
        let row = stmt
            .query_row(params![key], |row| {
                Ok(SemanticCacheEntry {
                    cache_key: row.get(0)?,
                    response: row.get(1)?,
                    model_id: row.get(2)?,
                    hit_count: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .optional()?;
        if let Some(ref entry) = row {
            let _ = conn.execute(
                "UPDATE semantic_cache SET hit_count = hit_count + 1, updated_at = ?1 WHERE cache_key = ?2",
                params![Utc::now().to_rfc3339(), entry.cache_key],
            );
        }
        Ok(row)
    }

    pub fn cache_put(&self, key: &str, response: &str, model_id: &str) -> AtlasResult<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO semantic_cache (cache_key, response, model_id, hit_count, created_at, updated_at)
             VALUES (?1,?2,?3,0,?4,?4)
             ON CONFLICT(cache_key) DO UPDATE SET
               response=excluded.response,
               model_id=excluded.model_id,
               updated_at=excluded.updated_at",
            params![key, response, model_id, now],
        )?;
        Ok(())
    }

    pub fn cache_invalidate_all(&self) -> AtlasResult<usize> {
        let conn = self.conn.lock().expect("store lock");
        let n = conn.execute("DELETE FROM semantic_cache", [])?;
        Ok(n)
    }

    pub fn new_id() -> String {
        Uuid::new_v4().to_string()
    }
}

fn map_case(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvalCase> {
    Ok(EvalCase {
        id: row.get(0)?,
        suite: row.get(1)?,
        name: row.get(2)?,
        query: row.get(3)?,
        expect: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or(Value::Null),
        tags: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
        created_at: row.get(6)?,
    })
}

fn map_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvalRun> {
    Ok(EvalRun {
        id: row.get(0)?,
        suite: row.get(1)?,
        case_id: row.get(2)?,
        score: row.get(3)?,
        passed: row.get::<_, i64>(4)? != 0,
        trace_id: row.get(5)?,
        detail: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or(Value::Null),
        created_at: row.get(7)?,
    })
}
