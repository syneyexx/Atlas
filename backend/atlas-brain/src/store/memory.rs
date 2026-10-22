use crate::error::AtlasResult;
use crate::memory::embed::{decode_vector, encode_vector};
use crate::memory::types::{
    MemoryKind, MemoryNode, MemoryScope, MemorySource, Provenance,
};
use crate::store::conversations::sanitize_fts_query;
use crate::store::KnowledgeNode;
use rusqlite::{params, OptionalExtension};

use super::db::Store;

impl Store {
    pub fn migrate_memory_schema(&self) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memory_nodes (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                source TEXT NOT NULL,
                created_at TEXT NOT NULL,
                parent_ids_json TEXT NOT NULL,
                confidence REAL NOT NULL,
                scope TEXT NOT NULL,
                conversation_id TEXT,
                salience REAL NOT NULL,
                decay REAL NOT NULL,
                archived_at TEXT,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS memory_embeddings (
                node_id TEXT PRIMARY KEY,
                dim INTEGER NOT NULL,
                vector_json BLOB NOT NULL,
                FOREIGN KEY(node_id) REFERENCES memory_nodes(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_memory_kind ON memory_nodes(kind);
            CREATE INDEX IF NOT EXISTS idx_memory_updated ON memory_nodes(updated_at);
            CREATE TABLE IF NOT EXISTS working_memory (
                conversation_id TEXT PRIMARY KEY,
                state_json TEXT NOT NULL,
                query_embedding BLOB,
                updated_at TEXT NOT NULL,
                expires_at TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    pub fn upsert_memory_node(&self, node: &MemoryNode) -> AtlasResult<()> {
        let tags_json = serde_json::to_string(&node.tags)?;
        let parents_json = serde_json::to_string(&node.provenance.parent_ids)?;
        let tags_joined = node.tags.join(" ");
        {
            let conn = self.conn.lock().expect("store lock");
            conn.execute(
                "INSERT INTO memory_nodes (
                    id, kind, title, content, tags_json, source, created_at, parent_ids_json,
                    confidence, scope, conversation_id, salience, decay, archived_at, updated_at
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
                 ON CONFLICT(id) DO UPDATE SET
                    kind=excluded.kind,
                    title=excluded.title,
                    content=excluded.content,
                    tags_json=excluded.tags_json,
                    source=excluded.source,
                    created_at=excluded.created_at,
                    parent_ids_json=excluded.parent_ids_json,
                    confidence=excluded.confidence,
                    scope=excluded.scope,
                    conversation_id=excluded.conversation_id,
                    salience=excluded.salience,
                    decay=excluded.decay,
                    archived_at=excluded.archived_at,
                    updated_at=excluded.updated_at",
                params![
                    node.id,
                    node.kind.as_str(),
                    node.title,
                    node.content,
                    tags_json,
                    node.provenance.source.as_str(),
                    node.provenance.created_at,
                    parents_json,
                    node.provenance.confidence,
                    node.provenance.scope.as_str(),
                    node.provenance.conversation_id,
                    node.salience,
                    node.decay,
                    node.archived_at,
                    node.updated_at,
                ],
            )?;
            // Rebuild FTS row for this node.
            let _ = conn.execute(
                "DELETE FROM memory_fts WHERE node_id = ?1",
                params![node.id],
            );
            conn.execute(
                "INSERT INTO memory_fts (node_id, kind, title, content, tags)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    node.id,
                    node.kind.as_str(),
                    node.title,
                    node.content,
                    tags_joined
                ],
            )?;
        }
        // Materialize parent_ids as graph edges (light knowledge graph).
        for parent in &node.provenance.parent_ids {
            if parent.is_empty() || parent == &node.id {
                continue;
            }
            let _ = self.upsert_edge(
                parent,
                "parent_of",
                &node.id,
                1.0,
                &serde_json::json!({
                    "source": "provenance.parent_ids",
                    "child_kind": node.kind.as_str(),
                }),
            );
        }
        Ok(())
    }

    pub fn get_memory_node(&self, id: &str) -> AtlasResult<Option<MemoryNode>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, kind, title, content, tags_json, source, created_at, parent_ids_json,
                    confidence, scope, conversation_id, salience, decay, archived_at, updated_at
             FROM memory_nodes WHERE id = ?1",
        )?;
        let node = stmt
            .query_row(params![id], row_to_node)
            .optional()?;
        Ok(node)
    }

    pub fn list_memory_nodes(&self, kind: Option<MemoryKind>) -> AtlasResult<Vec<MemoryNode>> {
        let conn = self.conn.lock().expect("store lock");
        let mut out = Vec::new();
        if let Some(kind) = kind {
            let mut stmt = conn.prepare(
                "SELECT id, kind, title, content, tags_json, source, created_at, parent_ids_json,
                        confidence, scope, conversation_id, salience, decay, archived_at, updated_at
                 FROM memory_nodes WHERE kind = ?1 ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map(params![kind.as_str()], row_to_node)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, kind, title, content, tags_json, source, created_at, parent_ids_json,
                        confidence, scope, conversation_id, salience, decay, archived_at, updated_at
                 FROM memory_nodes ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map([], row_to_node)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn archive_memory_node(&self, id: &str) -> AtlasResult<bool> {
        let conn = self.conn.lock().expect("store lock");
        let n = conn.execute(
            "UPDATE memory_nodes SET archived_at = ?1, updated_at = ?1 WHERE id = ?2 AND archived_at IS NULL",
            params![chrono::Utc::now().to_rfc3339(), id],
        )?;
        Ok(n > 0)
    }

    pub fn delete_memory_node(&self, id: &str) -> AtlasResult<bool> {
        let conn = self.conn.lock().expect("store lock");
        let _ = conn.execute("DELETE FROM memory_fts WHERE node_id = ?1", params![id]);
        let _ = conn.execute("DELETE FROM memory_embeddings WHERE node_id = ?1", params![id]);
        let _ = conn.execute(
            "DELETE FROM memory_edges WHERE src_id = ?1 OR dst_id = ?1",
            params![id],
        );
        let n = conn.execute("DELETE FROM memory_nodes WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

    pub fn upsert_embedding(
        &self,
        node_id: &str,
        vector: &[f32],
        embedding_model_id: &str,
    ) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO memory_embeddings (node_id, dim, vector_json, embedding_model_id, stale)
             VALUES (?1, ?2, ?3, ?4, 0)
             ON CONFLICT(node_id) DO UPDATE SET
               dim=excluded.dim,
               vector_json=excluded.vector_json,
               embedding_model_id=excluded.embedding_model_id,
               stale=0",
            params![
                node_id,
                vector.len() as i64,
                encode_vector(vector)?,
                embedding_model_id
            ],
        )?;
        Ok(())
    }

    pub fn get_embedding(&self, node_id: &str) -> AtlasResult<Option<Vec<f32>>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT vector_json, COALESCE(stale, 0) FROM memory_embeddings WHERE node_id = ?1",
        )?;
        let row: Option<(Vec<u8>, i64)> = stmt
            .query_row(params![node_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional()?;
        match row {
            Some((bytes, stale)) if stale == 0 => Ok(Some(decode_vector(&bytes)?)),
            Some(_) => Ok(None), // stale embedding → treat as missing (re-embed on write)
            None => Ok(None),
        }
    }

    #[allow(dead_code)]
    pub fn get_embedding_model_id(&self, node_id: &str) -> AtlasResult<Option<String>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT embedding_model_id FROM memory_embeddings WHERE node_id = ?1",
        )?;
        let id: Option<Option<String>> = stmt
            .query_row(params![node_id], |row| row.get(0))
            .optional()?;
        Ok(id.flatten())
    }

    /// Mark all embeddings for a different model as stale (request-driven reindex later).
    pub fn mark_embeddings_stale_except(&self, current_model: &str) -> AtlasResult<usize> {
        let conn = self.conn.lock().expect("store lock");
        let n = conn.execute(
            "UPDATE memory_embeddings SET stale = 1
             WHERE COALESCE(embedding_model_id, '') != ?1",
            params![current_model],
        )?;
        Ok(n)
    }

    /// BM25/FTS memory search — returns (node_id, rank) with higher = better.
    pub fn search_memory_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> AtlasResult<Vec<(String, f64)>> {
        let q = sanitize_fts_query(query);
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 100) as i64;
        let mut stmt = conn.prepare(
            "SELECT node_id, bm25(memory_fts) FROM memory_fts
             WHERE memory_fts MATCH ?1
             ORDER BY bm25(memory_fts) LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![q, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (-row.get::<_, f64>(1)?).max(0.0),
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn mirror_knowledge_from_memory(&self, node: &MemoryNode) -> AtlasResult<()> {
        let kn = KnowledgeNode {
            id: node.id.clone(),
            title: node.title.clone(),
            content: node.content.clone(),
            tags: node.tags.clone(),
            created_at: node.provenance.created_at.clone(),
        };
        let conn = self.conn.lock().expect("store lock");
        let tags = serde_json::to_string(&kn.tags)?;
        conn.execute(
            "INSERT INTO knowledge (id, title, content, tags_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               title=excluded.title,
               content=excluded.content,
               tags_json=excluded.tags_json,
               created_at=excluded.created_at",
            params![kn.id, kn.title, kn.content, tags, kn.created_at],
        )?;
        Ok(())
    }

    /// Lazy migrate legacy knowledge rows into memory_nodes (on-demand, no background job).
    pub fn migrate_knowledge_to_memory(&self) -> AtlasResult<usize> {
        let legacy = self.list_knowledge()?;
        let mut migrated = 0usize;
        for kn in legacy {
            if self.get_memory_node(&kn.id)?.is_some() {
                continue;
            }
            let node = MemoryNode {
                id: kn.id.clone(),
                kind: MemoryKind::Semantic,
                title: kn.title,
                content: kn.content,
                tags: kn.tags,
                provenance: Provenance {
                    source: MemorySource::Migrate,
                    created_at: kn.created_at.clone(),
                    parent_ids: Vec::new(),
                    confidence: 0.7,
                    scope: MemoryScope::Global,
                    conversation_id: None,
                },
                salience: 1.0,
                decay: 1.0,
                archived_at: None,
                updated_at: kn.created_at,
            };
            self.upsert_memory_node(&node)?;
            migrated += 1;
        }
        Ok(migrated)
    }

    pub fn get_working_memory(
        &self,
        conversation_id: &str,
    ) -> AtlasResult<Option<crate::memory::working::WorkingMemoryState>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT state_json FROM working_memory WHERE conversation_id = ?1",
        )?;
        let raw: Option<String> = stmt
            .query_row(params![conversation_id], |row| row.get(0))
            .optional()?;
        match raw {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    pub fn upsert_working_memory(
        &self,
        state: &crate::memory::working::WorkingMemoryState,
        query_embedding: Option<&[u8]>,
    ) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO working_memory (conversation_id, state_json, query_embedding, updated_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(conversation_id) DO UPDATE SET
               state_json=excluded.state_json,
               query_embedding=COALESCE(excluded.query_embedding, working_memory.query_embedding),
               updated_at=excluded.updated_at,
               expires_at=excluded.expires_at",
            params![
                state.conversation_id,
                serde_json::to_string(state)?,
                query_embedding,
                state.updated_at,
                state.expires_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_working_embedding(&self, conversation_id: &str) -> AtlasResult<Option<Vec<u8>>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt =
            conn.prepare("SELECT query_embedding FROM working_memory WHERE conversation_id = ?1")?;
        let blob: Option<Vec<u8>> = stmt
            .query_row(params![conversation_id], |row| row.get(0))
            .optional()?;
        Ok(blob)
    }

    pub fn delete_working_memory(&self, conversation_id: &str) -> AtlasResult<bool> {
        let conn = self.conn.lock().expect("store lock");
        let n = conn.execute(
            "DELETE FROM working_memory WHERE conversation_id = ?1",
            params![conversation_id],
        )?;
        Ok(n > 0)
    }
}

fn row_to_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryNode> {
    let kind = MemoryKind::parse(&row.get::<_, String>(1)?).unwrap_or(MemoryKind::Semantic);
    let source = MemorySource::parse(&row.get::<_, String>(5)?);
    let scope = MemoryScope::parse(&row.get::<_, String>(9)?);
    Ok(MemoryNode {
        id: row.get(0)?,
        kind,
        title: row.get(2)?,
        content: row.get(3)?,
        tags: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
        provenance: Provenance {
            source,
            created_at: row.get(6)?,
            parent_ids: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
            confidence: row.get(8)?,
            scope,
            conversation_id: row.get(10)?,
        },
        salience: row.get(11)?,
        decay: row.get(12)?,
        archived_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}
