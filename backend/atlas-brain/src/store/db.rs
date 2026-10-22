use crate::config::BrainConfig;
use crate::error::AtlasResult;
use crate::store::engines::EngineBundle;
use crate::store::{ConversationRecord, KnowledgeNode};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Store {
    pub(crate) conn: Mutex<Connection>,
    pub(crate) engines: EngineBundle,
    pub(crate) data_dir: PathBuf,
}

impl Store {
    pub fn open(cfg: &BrainConfig) -> AtlasResult<Self> {
        let data_dir = cfg.data_dir.clone();
        std::fs::create_dir_all(&data_dir)?;
        let path = data_dir.join("atlas.db");
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                messages_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS knowledge (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            ",
        )?;
        let engines = EngineBundle::from_config(cfg)?;
        let store = Self {
            conn: Mutex::new(conn),
            engines,
            data_dir,
        };
        store.migrate_memory_schema()?;
        store.migrate_store_plane_schema()?;
        store.migrate_documents_schema()?;
        store.migrate_agent_os_schema()?;
        store.migrate_eval_schema()?;
        store.migrate_platform_schema()?;
        store.migrate_research_schema()?;
        let _ = store.migrate_messages_from_blobs()?;
        let _ = store.seed_builtin_tools()?;
        tracing::info!(engines = %store.engines.status_json(), "store plane engines");
        Ok(store)
    }

    pub fn engine_status(&self) -> serde_json::Value {
        self.engines.status_json()
    }

    #[allow(dead_code)]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn list_conversations(&self) -> AtlasResult<Vec<ConversationRecord>> {
        let ids: Vec<(String, String, String)> = {
            let conn = self.conn.lock().expect("store lock");
            let mut stmt = conn.prepare(
                "SELECT id, title, updated_at FROM conversations ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            out
        };
        let mut out = Vec::new();
        for (id, title, updated_at) in ids {
            let messages = self.messages_for_conversation(&id)?;
            out.push(ConversationRecord {
                id,
                title,
                messages,
                updated_at,
            });
        }
        Ok(out)
    }

    pub fn get_conversation(&self, id: &str) -> AtlasResult<Option<ConversationRecord>> {
        let meta: Option<(String, String)> = {
            let conn = self.conn.lock().expect("store lock");
            let mut stmt =
                conn.prepare("SELECT title, updated_at FROM conversations WHERE id = ?1")?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                Some((row.get(0)?, row.get(1)?))
            } else {
                None
            }
        };
        let Some((title, updated_at)) = meta else {
            return Ok(None);
        };
        Ok(Some(ConversationRecord {
            id: id.to_string(),
            title,
            messages: self.messages_for_conversation(id)?,
            updated_at,
        }))
    }

    fn messages_for_conversation(&self, id: &str) -> AtlasResult<Vec<crate::store::ChatMessage>> {
        let normalized = self.load_messages(id)?;
        if !normalized.is_empty() {
            return Ok(normalized);
        }
        // Fallback to legacy blob (pre-M0 DBs mid-migration).
        Ok(self
            .load_conversation_blob(id)?
            .map(|r| r.messages)
            .unwrap_or_default())
    }

    pub fn upsert_conversation(&self, record: &ConversationRecord) -> AtlasResult<()> {
        // Ensure message ids exist so FTS/trace links stay stable across upserts.
        let mut record = record.clone();
        for msg in &mut record.messages {
            if msg.id.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
                msg.id = Some(uuid::Uuid::new_v4().to_string());
            }
        }
        self.replace_messages(&record)
    }

    pub fn delete_conversation(&self, id: &str) -> AtlasResult<bool> {
        // FTS rows first (no FK to messages_fts).
        let mids: Vec<String> = {
            let conn = self.conn.lock().expect("store lock");
            let mut stmt =
                conn.prepare("SELECT id FROM messages WHERE conversation_id = ?1")?;
            let rows = stmt.query_map(params![id], |row| row.get(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            out
        };
        let conn = self.conn.lock().expect("store lock");
        for mid in &mids {
            let _ = conn.execute(
                "DELETE FROM messages_fts WHERE message_id = ?1",
                params![mid],
            );
        }
        let _ = conn.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![id],
        );
        let n = conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

    #[allow(dead_code)]
    pub fn insert_knowledge(&self, node: &KnowledgeNode) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        let tags = serde_json::to_string(&node.tags)?;
        conn.execute(
            "INSERT INTO knowledge (id, title, content, tags_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![node.id, node.title, node.content, tags, node.created_at],
        )?;
        Ok(())
    }

    pub fn list_knowledge(&self) -> AtlasResult<Vec<KnowledgeNode>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt =
            conn.prepare("SELECT id, title, content, tags_json, created_at FROM knowledge ORDER BY created_at DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(KnowledgeNode {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                tags: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                created_at: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn delete_knowledge(&self, id: &str) -> AtlasResult<bool> {
        let conn = self.conn.lock().expect("store lock");
        let n = conn.execute("DELETE FROM knowledge WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }
}
