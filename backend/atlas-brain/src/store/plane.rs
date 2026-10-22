//! Store Plane M0 — schema hardening on SQLite.
//! messages · FTS5 · memory_edges · embedding_model_id · traces · outbox

use crate::error::AtlasResult;
use crate::store::ConversationRecord;
use rusqlite::{params, Connection};

use super::db::Store;

impl Store {
    pub fn migrate_store_plane_schema(&self) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                callout_json TEXT,
                created_at TEXT,
                trace_id TEXT,
                model_id TEXT,
                FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_messages_conversation
                ON messages(conversation_id, seq);

            CREATE TABLE IF NOT EXISTS memory_edges (
                id TEXT PRIMARY KEY,
                src_id TEXT NOT NULL,
                rel TEXT NOT NULL,
                dst_id TEXT NOT NULL,
                weight REAL NOT NULL DEFAULT 1.0,
                provenance_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(src_id, rel, dst_id)
            );
            CREATE INDEX IF NOT EXISTS idx_edges_src ON memory_edges(src_id);
            CREATE INDEX IF NOT EXISTS idx_edges_dst ON memory_edges(dst_id);

            CREATE TABLE IF NOT EXISTS traces (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                conversation_id TEXT,
                model_id TEXT,
                payload_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_traces_created ON traces(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_traces_conversation ON traces(conversation_id);

            CREATE TABLE IF NOT EXISTS outbox (
                id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                processed_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_outbox_pending
                ON outbox(processed_at, created_at);
            ",
        )?;

        ensure_column(&conn, "memory_embeddings", "embedding_model_id", "TEXT")?;
        ensure_column(
            &conn,
            "memory_embeddings",
            "stale",
            "INTEGER NOT NULL DEFAULT 0",
        )?;

        // Standalone FTS5 keyed by id (rebuild on write).
        conn.execute_batch(
            "
            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                message_id UNINDEXED,
                conversation_id UNINDEXED,
                role UNINDEXED,
                body,
                tokenize = 'unicode61'
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                node_id UNINDEXED,
                kind UNINDEXED,
                title,
                content,
                tags,
                tokenize = 'unicode61'
            );
            ",
        )?;

        Ok(())
    }

    /// One-shot backfill: blob conversations → normalized messages (+ FTS).
    pub fn migrate_messages_from_blobs(&self) -> AtlasResult<usize> {
        let ids: Vec<String> = {
            let conn = self.conn.lock().expect("store lock");
            let mut stmt = conn.prepare("SELECT id FROM conversations")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            out
        };

        let mut migrated = 0usize;
        for id in ids {
            let count: i64 = {
                let conn = self.conn.lock().expect("store lock");
                conn.query_row(
                    "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
                    params![id],
                    |row| row.get(0),
                )?
            };
            if count > 0 {
                continue;
            }
            if let Some(record) = self.load_conversation_blob(&id)? {
                if record.messages.is_empty() {
                    continue;
                }
                self.replace_messages(&record)?;
                migrated += 1;
            }
        }
        Ok(migrated)
    }

    pub(crate) fn load_conversation_blob(
        &self,
        id: &str,
    ) -> AtlasResult<Option<ConversationRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, title, messages_json, updated_at FROM conversations WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ConversationRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                messages: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                updated_at: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }
}

fn ensure_column(conn: &Connection, table: &str, column: &str, decl: &str) -> AtlasResult<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .collect();
    if !names.iter().any(|n| n == column) {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"),
            [],
        )?;
    }
    Ok(())
}
