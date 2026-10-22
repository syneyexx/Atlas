use crate::error::AtlasResult;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersion {
    pub id: String,
    pub name: String,
    pub version: i64,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub active: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRegistryEntry {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub meta: Value,
    pub created_at: String,
}

impl Store {
    pub fn next_prompt_version(&self, name: &str) -> AtlasResult<i64> {
        let conn = self.conn.lock().expect("store lock");
        let max: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM prompt_versions WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(max + 1)
    }

    pub fn insert_prompt_version(&self, prompt: &PromptVersion) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        if prompt.active {
            conn.execute(
                "UPDATE prompt_versions SET active = 0 WHERE name = ?1",
                params![prompt.name],
            )?;
        }
        conn.execute(
            "INSERT INTO prompt_versions (id, name, version, content, model_id, active, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                prompt.id,
                prompt.name,
                prompt.version,
                prompt.content,
                prompt.model_id,
                if prompt.active { 1 } else { 0 },
                prompt.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_prompt_versions(&self, name: Option<&str>) -> AtlasResult<Vec<PromptVersion>> {
        let conn = self.conn.lock().expect("store lock");
        let mut out = Vec::new();
        if let Some(n) = name {
            let mut stmt = conn.prepare(
                "SELECT id, name, version, content, model_id, active, created_at
                 FROM prompt_versions WHERE name = ?1 ORDER BY version DESC",
            )?;
            let rows = stmt.query_map(params![n], map_prompt)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, name, version, content, model_id, active, created_at
                 FROM prompt_versions ORDER BY name, version DESC",
            )?;
            let rows = stmt.query_map([], map_prompt)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn get_prompt_version(&self, id: &str) -> AtlasResult<Option<PromptVersion>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, name, version, content, model_id, active, created_at
             FROM prompt_versions WHERE id = ?1",
        )?;
        Ok(stmt.query_row(params![id], map_prompt).optional()?)
    }

    pub fn activate_prompt_version(&self, id: &str) -> AtlasResult<Option<PromptVersion>> {
        let Some(prompt) = self.get_prompt_version(id)? else {
            return Ok(None);
        };
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE prompt_versions SET active = 0 WHERE name = ?1",
            params![prompt.name],
        )?;
        conn.execute(
            "UPDATE prompt_versions SET active = 1 WHERE id = ?1",
            params![id],
        )?;
        self.get_prompt_version(id)
    }

    pub fn get_active_prompt(&self, name: &str) -> AtlasResult<Option<PromptVersion>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, name, version, content, model_id, active, created_at
             FROM prompt_versions WHERE name = ?1 AND active = 1
             ORDER BY version DESC LIMIT 1",
        )?;
        Ok(stmt.query_row(params![name], map_prompt).optional()?)
    }

    /// Snapshot current system prompt if content changed (request-driven versioning).
    pub fn snapshot_system_prompt_if_changed(
        &self,
        content: &str,
        model_id: Option<&str>,
    ) -> AtlasResult<Option<PromptVersion>> {
        let name = "system_prompt";
        if let Some(active) = self.get_active_prompt(name)? {
            if active.content == content
                && active.model_id.as_deref() == model_id.filter(|s| !s.is_empty())
            {
                return Ok(None);
            }
        } else if content.trim().is_empty() {
            return Ok(None);
        }
        let version = self.next_prompt_version(name)?;
        let prompt = PromptVersion {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            version,
            content: content.to_string(),
            model_id: model_id.filter(|s| !s.is_empty()).map(str::to_string),
            active: true,
            created_at: Utc::now().to_rfc3339(),
        };
        self.insert_prompt_version(&prompt)?;
        Ok(Some(prompt))
    }

    pub fn upsert_model_registry(&self, entry: &ModelRegistryEntry) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO model_registry (id, provider, label, meta_json, created_at)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(id) DO UPDATE SET
               provider=excluded.provider,
               label=excluded.label,
               meta_json=excluded.meta_json",
            params![
                entry.id,
                entry.provider,
                entry.label,
                serde_json::to_string(&entry.meta)?,
                entry.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_model_registry(&self) -> AtlasResult<Vec<ModelRegistryEntry>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, provider, label, meta_json, created_at FROM model_registry ORDER BY label",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ModelRegistryEntry {
                id: row.get(0)?,
                provider: row.get(1)?,
                label: row.get(2)?,
                meta: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or(Value::Null),
                created_at: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

fn map_prompt(row: &rusqlite::Row<'_>) -> rusqlite::Result<PromptVersion> {
    Ok(PromptVersion {
        id: row.get(0)?,
        name: row.get(1)?,
        version: row.get(2)?,
        content: row.get(3)?,
        model_id: row.get(4)?,
        active: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
    })
}
