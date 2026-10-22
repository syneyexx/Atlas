use crate::error::AtlasResult;
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEvent {
    pub id: String,
    pub event_type: String,
    pub payload: Value,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_at: Option<String>,
}

impl Store {
    pub fn enqueue_outbox(&self, event_type: &str, payload: &Value) -> AtlasResult<String> {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let payload_json = serde_json::to_string(payload)?;
        {
            let conn = self.conn.lock().expect("store lock");
            conn.execute(
                "INSERT INTO outbox (id, event_type, payload_json, created_at, processed_at)
                 VALUES (?1, ?2, ?3, ?4, NULL)",
                params![id, event_type, payload_json, created_at],
            )?;
        }
        if let Some(pg) = &self.engines.postgres {
            if let Err(err) = crate::store::engines::pg_mirror::mirror_outbox(
                pg,
                &id,
                event_type,
                &payload_json,
                &created_at,
            ) {
                tracing::warn!("postgres mirror outbox failed: {err}");
            }
        }
        Ok(id)
    }

    pub fn list_pending_outbox(&self, limit: usize) -> AtlasResult<Vec<OutboxEvent>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 500) as i64;
        let mut stmt = conn.prepare(
            "SELECT id, event_type, payload_json, created_at, processed_at
             FROM outbox WHERE processed_at IS NULL
             ORDER BY created_at ASC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], map_outbox)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn mark_outbox_processed(&self, id: &str) -> AtlasResult<bool> {
        let processed_at = Utc::now().to_rfc3339();
        let n = {
            let conn = self.conn.lock().expect("store lock");
            conn.execute(
                "UPDATE outbox SET processed_at = ?1 WHERE id = ?2 AND processed_at IS NULL",
                params![processed_at, id],
            )?
        };
        if n > 0 {
            if let Some(pg) = &self.engines.postgres {
                if let Err(err) =
                    crate::store::engines::pg_mirror::mirror_outbox_processed(pg, id, &processed_at)
                {
                    tracing::warn!("postgres mirror outbox processed failed: {err}");
                }
            }
        }
        Ok(n > 0)
    }

    /// Request-driven drain: mark pending events processed (durable ack).
    /// Event payloads remain queryable via traces/outbox APIs for replay.
    pub fn drain_outbox(&self, limit: usize) -> AtlasResult<DrainResult> {
        let pending = self.list_pending_outbox(limit)?;
        let mut processed = 0usize;
        let mut by_type = std::collections::BTreeMap::<String, usize>::new();
        for ev in &pending {
            if self.mark_outbox_processed(&ev.id)? {
                processed += 1;
                *by_type.entry(ev.event_type.clone()).or_insert(0) += 1;
            }
        }
        Ok(DrainResult {
            seen: pending.len(),
            processed,
            by_type,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DrainResult {
    pub seen: usize,
    pub processed: usize,
    pub by_type: std::collections::BTreeMap<String, usize>,
}

fn map_outbox(row: &rusqlite::Row<'_>) -> rusqlite::Result<OutboxEvent> {
    Ok(OutboxEvent {
        id: row.get(0)?,
        event_type: row.get(1)?,
        payload: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or(Value::Null),
        created_at: row.get(3)?,
        processed_at: row.get(4)?,
    })
}
