use crate::error::AtlasResult;
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRecord {
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub payload: Value,
    pub created_at: String,
}

impl Store {
    pub fn insert_trace(
        &self,
        kind: &str,
        conversation_id: Option<&str>,
        model_id: Option<&str>,
        payload: &Value,
    ) -> AtlasResult<String> {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let payload_json = serde_json::to_string(payload)?;
        {
            let conn = self.conn.lock().expect("store lock");
            conn.execute(
                "INSERT INTO traces (id, kind, conversation_id, model_id, payload_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    kind,
                    conversation_id,
                    model_id,
                    payload_json,
                    created_at
                ],
            )?;
        }
        if let Some(pg) = &self.engines.postgres {
            if let Err(err) = crate::store::engines::pg_mirror::mirror_trace(
                pg,
                &id,
                kind,
                conversation_id,
                model_id,
                &payload_json,
                &created_at,
            ) {
                tracing::warn!("postgres mirror trace failed: {err}");
            }
        }
        Ok(id)
    }

    pub fn get_trace(&self, id: &str) -> AtlasResult<Option<TraceRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, kind, conversation_id, model_id, payload_json, created_at
             FROM traces WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(TraceRecord {
                id: row.get(0)?,
                kind: row.get(1)?,
                conversation_id: row.get(2)?,
                model_id: row.get(3)?,
                payload: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or(Value::Null),
                created_at: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_traces(
        &self,
        conversation_id: Option<&str>,
        limit: usize,
    ) -> AtlasResult<Vec<TraceRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 200) as i64;
        let mut out = Vec::new();
        if let Some(cid) = conversation_id {
            let mut stmt = conn.prepare(
                "SELECT id, kind, conversation_id, model_id, payload_json, created_at
                 FROM traces WHERE conversation_id = ?1
                 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![cid, limit], map_trace)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, kind, conversation_id, model_id, payload_json, created_at
                 FROM traces ORDER BY created_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], map_trace)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }
}

fn map_trace(row: &rusqlite::Row<'_>) -> rusqlite::Result<TraceRecord> {
    Ok(TraceRecord {
        id: row.get(0)?,
        kind: row.get(1)?,
        conversation_id: row.get(2)?,
        model_id: row.get(3)?,
        payload: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or(Value::Null),
        created_at: row.get(5)?,
    })
}
