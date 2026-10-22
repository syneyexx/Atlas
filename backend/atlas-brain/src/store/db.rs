use crate::error::AtlasResult;
use crate::store::{ConversationRecord, KnowledgeNode};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(data_dir: &Path) -> AtlasResult<Self> {
        std::fs::create_dir_all(data_dir)?;
        let path = data_dir.join("atlas.db");
        let conn = Connection::open(path)?;
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
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn list_conversations(&self) -> AtlasResult<Vec<ConversationRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, title, messages_json, updated_at FROM conversations ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ConversationRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                messages: serde_json::from_str(&row.get::<_, String>(2)?)
                    .unwrap_or_default(),
                updated_at: row.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_conversation(&self, id: &str) -> AtlasResult<Option<ConversationRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, title, messages_json, updated_at FROM conversations WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ConversationRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                messages: serde_json::from_str(&row.get::<_, String>(2)?)
                    .unwrap_or_default(),
                updated_at: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_conversation(&self, record: &ConversationRecord) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        let messages = serde_json::to_string(&record.messages)?;
        conn.execute(
            "INSERT INTO conversations (id, title, messages_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
               title=excluded.title,
               messages_json=excluded.messages_json,
               updated_at=excluded.updated_at",
            params![record.id, record.title, messages, record.updated_at],
        )?;
        Ok(())
    }

    pub fn delete_conversation(&self, id: &str) -> AtlasResult<bool> {
        let conn = self.conn.lock().expect("store lock");
        let n = conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(n > 0)
    }

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
