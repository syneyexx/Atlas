use crate::error::AtlasResult;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRecord {
    pub id: String,
    pub title: String,
    pub source: String,
    pub media_type: String,
    pub object_hash: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunk_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: String,
    pub document_id: String,
    pub seq: i64,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_node_id: Option<String>,
    pub created_at: String,
}

impl Store {
    pub fn migrate_documents_schema(&self) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                source TEXT NOT NULL,
                media_type TEXT NOT NULL,
                object_hash TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_documents_updated ON documents(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_documents_hash ON documents(object_hash);

            CREATE TABLE IF NOT EXISTS document_chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                content TEXT NOT NULL,
                memory_node_id TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_chunks_document ON document_chunks(document_id, seq);

            CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
                document_id UNINDEXED,
                chunk_id UNINDEXED,
                title,
                content,
                tokenize = 'unicode61'
            );
            ",
        )?;
        Ok(())
    }

    pub fn upsert_document(&self, doc: &DocumentRecord) -> AtlasResult<()> {
        {
            let conn = self.conn.lock().expect("store lock");
            conn.execute(
                "INSERT INTO documents (id, title, source, media_type, object_hash, metadata_json, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                 ON CONFLICT(id) DO UPDATE SET
                   title=excluded.title,
                   source=excluded.source,
                   media_type=excluded.media_type,
                   object_hash=excluded.object_hash,
                   metadata_json=excluded.metadata_json,
                   updated_at=excluded.updated_at",
                params![
                    doc.id,
                    doc.title,
                    doc.source,
                    doc.media_type,
                    doc.object_hash,
                    serde_json::to_string(&doc.metadata)?,
                    doc.created_at,
                    doc.updated_at
                ],
            )?;
        }
        if let Some(pg) = &self.engines.postgres {
            if let Err(err) = crate::store::engines::pg_mirror::mirror_document(pg, doc) {
                tracing::warn!("postgres mirror document failed: {err}");
            }
        }
        Ok(())
    }

    pub fn insert_document_chunk(&self, chunk: &DocumentChunk, title: &str) -> AtlasResult<()> {
        {
            let conn = self.conn.lock().expect("store lock");
            conn.execute(
                "INSERT INTO document_chunks (id, document_id, seq, content, memory_node_id, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    chunk.id,
                    chunk.document_id,
                    chunk.seq,
                    chunk.content,
                    chunk.memory_node_id,
                    chunk.created_at
                ],
            )?;
            conn.execute(
                "INSERT INTO documents_fts (document_id, chunk_id, title, content)
                 VALUES (?1, ?2, ?3, ?4)",
                params![chunk.document_id, chunk.id, title, chunk.content],
            )?;
        }
        if let Some(pg) = &self.engines.postgres {
            if let Err(err) = crate::store::engines::pg_mirror::mirror_document_chunk(pg, chunk) {
                tracing::warn!("postgres mirror document_chunk failed: {err}");
            }
        }
        Ok(())
    }

    pub fn list_documents(&self) -> AtlasResult<Vec<DocumentRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, title, source, media_type, object_hash, metadata_json, created_at, updated_at
             FROM documents ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], map_doc)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_document(&self, id: &str) -> AtlasResult<Option<DocumentRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, title, source, media_type, object_hash, metadata_json, created_at, updated_at
             FROM documents WHERE id = ?1",
        )?;
        let doc = stmt.query_row(params![id], map_doc).optional()?;
        Ok(doc)
    }

    pub fn list_document_chunks(&self, document_id: &str) -> AtlasResult<Vec<DocumentChunk>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, document_id, seq, content, memory_node_id, created_at
             FROM document_chunks WHERE document_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![document_id], map_chunk)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn delete_document(&self, id: &str) -> AtlasResult<bool> {
        let chunk_ids: Vec<String> = {
            let conn = self.conn.lock().expect("store lock");
            let mut stmt =
                conn.prepare("SELECT id FROM document_chunks WHERE document_id = ?1")?;
            let rows = stmt.query_map(params![id], |row| row.get(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            out
        };
        let conn = self.conn.lock().expect("store lock");
        for cid in &chunk_ids {
            let _ = conn.execute(
                "DELETE FROM documents_fts WHERE chunk_id = ?1",
                params![cid],
            );
        }
        let _ = conn.execute(
            "DELETE FROM document_chunks WHERE document_id = ?1",
            params![id],
        );
        let n = conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

    pub fn search_documents_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> AtlasResult<Vec<(String, String, f64)>> {
        let q = super::conversations::sanitize_fts_query(query);
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 100) as i64;
        let mut stmt = conn.prepare(
            "SELECT document_id, chunk_id, bm25(documents_fts)
             FROM documents_fts WHERE documents_fts MATCH ?1
             ORDER BY bm25(documents_fts) LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![q, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                (-row.get::<_, f64>(2)?).max(0.0),
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn new_document_id() -> String {
        Uuid::new_v4().to_string()
    }

    pub fn now_rfc3339() -> String {
        Utc::now().to_rfc3339()
    }
}

fn map_doc(row: &rusqlite::Row<'_>) -> rusqlite::Result<DocumentRecord> {
    Ok(DocumentRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        source: row.get(2)?,
        media_type: row.get(3)?,
        object_hash: row.get(4)?,
        metadata: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or(serde_json::json!({})),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        chunk_ids: Vec::new(),
    })
}

fn map_chunk(row: &rusqlite::Row<'_>) -> rusqlite::Result<DocumentChunk> {
    Ok(DocumentChunk {
        id: row.get(0)?,
        document_id: row.get(1)?,
        seq: row.get(2)?,
        content: row.get(3)?,
        memory_node_id: row.get(4)?,
        created_at: row.get(5)?,
    })
}
