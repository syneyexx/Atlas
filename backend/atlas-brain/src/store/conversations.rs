use crate::error::AtlasResult;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub role: String,
    pub content: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "createdAt"
    )]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callout: Option<Callout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Callout {
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRecord {
    pub id: String,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub updated_at: String,
}

impl Store {
    /// Replace normalized message rows + FTS for a conversation.
    /// Also keeps `messages_json` as denormalized mirror for older readers.
    pub fn replace_messages(&self, record: &ConversationRecord) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        let tx = conn.unchecked_transaction()?;

        // Parent row first — messages FK conversations(id).
        let messages_json = serde_json::to_string(&record.messages)?;
        tx.execute(
            "INSERT INTO conversations (id, title, messages_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
               title=excluded.title,
               messages_json=excluded.messages_json,
               updated_at=excluded.updated_at",
            params![record.id, record.title, messages_json, record.updated_at],
        )?;

        // Clear old FTS rows for this conversation.
        let old_ids: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT id FROM messages WHERE conversation_id = ?1",
            )?;
            let rows = stmt.query_map(params![record.id], |row| row.get(0))?;
            let mut ids = Vec::new();
            for row in rows {
                ids.push(row?);
            }
            ids
        };
        for mid in &old_ids {
            tx.execute(
                "DELETE FROM messages_fts WHERE message_id = ?1",
                params![mid],
            )?;
        }
        tx.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![record.id],
        )?;

        for (seq, msg) in record.messages.iter().enumerate() {
            let mid = msg
                .id
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let callout_json = msg
                .callout
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            tx.execute(
                "INSERT INTO messages (
                    id, conversation_id, seq, role, content, callout_json,
                    created_at, trace_id, model_id
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    mid.clone(),
                    record.id,
                    seq as i64,
                    msg.role,
                    msg.content,
                    callout_json,
                    msg.created_at,
                    msg.trace_id,
                    msg.model_id,
                ],
            )?;
            tx.execute(
                "INSERT INTO messages_fts (message_id, conversation_id, role, body)
                 VALUES (?1, ?2, ?3, ?4)",
                params![mid, record.id, msg.role, msg.content],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn load_messages(&self, conversation_id: &str) -> AtlasResult<Vec<ChatMessage>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, role, content, callout_json, created_at, trace_id, model_id
             FROM messages WHERE conversation_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            let callout_raw: Option<String> = row.get(3)?;
            Ok(ChatMessage {
                id: Some(row.get(0)?),
                role: row.get(1)?,
                content: row.get(2)?,
                callout: callout_raw
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok()),
                created_at: row.get(4)?,
                trace_id: row.get(5)?,
                model_id: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    #[allow(dead_code)]
    pub fn search_messages_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> AtlasResult<Vec<(String, String, f64)>> {
        let q = sanitize_fts_query(query);
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 100) as i64;
        let mut stmt = conn.prepare(
            "SELECT message_id, conversation_id, bm25(messages_fts)
             FROM messages_fts WHERE messages_fts MATCH ?1
             ORDER BY bm25(messages_fts) LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![q, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                // bm25 is lower=better; convert to positive relevance
                (-row.get::<_, f64>(2)?).max(0.0),
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

/// Turn free text into a safe FTS5 OR-query of tokens.
pub(crate) fn sanitize_fts_query(query: &str) -> String {
    let tokens: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| {
            let clean: String = t
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            format!("\"{clean}\"")
        })
        .filter(|t| t.len() > 2)
        .take(12)
        .collect();
    tokens.join(" OR ")
}
