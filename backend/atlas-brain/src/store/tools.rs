use crate::error::AtlasResult;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRecord {
    pub name: String,
    pub version: String,
    pub description: String,
    pub schema: Value,
    /// Runtime policy: allow | deny | ask
    pub policy: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Store {
    pub fn migrate_agent_os_schema(&self) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS tools (
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                description TEXT NOT NULL,
                schema_json TEXT NOT NULL,
                policy TEXT NOT NULL,
                capabilities_json TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(name, version)
            );
            CREATE INDEX IF NOT EXISTS idx_tools_enabled ON tools(enabled, name);

            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                status TEXT NOT NULL,
                conversation_id TEXT,
                parent_id TEXT,
                plan_json TEXT NOT NULL,
                state_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status, updated_at DESC);

            CREATE TABLE IF NOT EXISTS task_checkpoints (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                label TEXT NOT NULL,
                snapshot_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_checkpoints_task ON task_checkpoints(task_id, created_at);

            CREATE TABLE IF NOT EXISTS prompt_versions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                version INTEGER NOT NULL,
                content TEXT NOT NULL,
                model_id TEXT,
                active INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                UNIQUE(name, version)
            );
            CREATE INDEX IF NOT EXISTS idx_prompts_active ON prompt_versions(name, active);

            CREATE TABLE IF NOT EXISTS model_registry (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                label TEXT NOT NULL,
                meta_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    pub fn seed_builtin_tools(&self) -> AtlasResult<usize> {
        let builtins = [
            (
                "memory.learn",
                "1",
                "Schrijf een feit of skill naar Memory Cortex",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "content": {"type": "string"},
                        "kind": {"type": "string", "enum": ["semantic","episodic","procedural","self_profile"]}
                    },
                    "required": ["content"]
                }),
                "allow",
                vec!["memory.write"],
            ),
            (
                "memory.recall",
                "1",
                "Zoek in Memory Cortex (hybrid)",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "top_k": {"type": "integer"}
                    },
                    "required": ["query"]
                }),
                "allow",
                vec!["memory.read"],
            ),
            (
                "documents.ingest",
                "1",
                "Ingest tekst/document naar RAG + object storage",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "content": {"type": "string"}
                    },
                    "required": ["content"]
                }),
                "ask",
                vec!["documents.write", "objects.write"],
            ),
            (
                "web.search",
                "1",
                "Zoek het web (stub corpus of HTTP search provider)",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "limit": {"type": "integer"}
                    },
                    "required": ["query"]
                }),
                "allow",
                vec!["research.search"],
            ),
            (
                "web.fetch",
                "1",
                "Haal een pagina op en normaliseer naar leesbare tekst",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {"type": "string"}
                    },
                    "required": ["url"]
                }),
                "allow",
                vec!["research.fetch"],
            ),
            (
                "research.run",
                "1",
                "Start of tick een research-job in Atlas Brain",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "question": {"type": "string"},
                        "mode": {"type": "string"},
                        "job_id": {"type": "string"},
                        "action": {"type": "string", "enum": ["create","tick","pause","clarify"]}
                    },
                    "required": ["action"]
                }),
                "allow",
                vec!["research.write"],
            ),
            (
                "datasets.search",
                "1",
                "Zoek dataset-cataloguskaarten (Hub stub/http)",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "modality": {"type": "string"},
                        "limit": {"type": "integer"}
                    },
                    "required": ["query"]
                }),
                "allow",
                vec!["datasets.read"],
            ),
            (
                "datasets.acquire",
                "1",
                "Pin + acquire een dataset-revisie (shards + checksums)",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source": {"type": "string"},
                        "id": {"type": "string"},
                        "revision": {"type": "string"}
                    },
                    "required": ["source", "id"]
                }),
                "ask",
                vec!["datasets.write", "objects.write"],
            ),
            (
                "datasets.run",
                "1",
                "Sample/full dataset-job: create, tick, pause, approve recipe",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {"type": "string", "enum": ["create","tick","pause","approve"]},
                        "dataset_id": {"type": "string"},
                        "job_id": {"type": "string"},
                        "recipe_id": {"type": "string"},
                        "kind": {"type": "string", "enum": ["sample","full"]}
                    },
                    "required": ["action"]
                }),
                "allow",
                vec!["datasets.write"],
            ),
            (
                "datasets.get_record",
                "1",
                "Zoek records in verwerkte datasets (FTS)",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "dataset_id": {"type": "string"},
                        "limit": {"type": "integer"}
                    },
                    "required": ["query"]
                }),
                "allow",
                vec!["datasets.read"],
            ),
            (
                "datasets.neighbors",
                "1",
                "Graph neighbors voor dataset/record node",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "node": {"type": "string"}
                    },
                    "required": ["node"]
                }),
                "allow",
                vec!["datasets.read"],
            ),
            (
                "datasets.subgraph",
                "1",
                "Subgraph rond een dataset/record node",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "node": {"type": "string"}
                    },
                    "required": ["node"]
                }),
                "allow",
                vec!["datasets.read"],
            ),
            (
                "datasets.compile_mix",
                "1",
                "Compileer een trainingsmix (blokkeert contaminated zonder override)",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "mix_id": {"type": "string"}
                    },
                    "required": ["mix_id"]
                }),
                "ask",
                vec!["datasets.export"],
            ),
        ];
        let mut n = 0usize;
        for (name, ver, desc, schema, policy, caps) in builtins {
            if self.get_tool(name, Some(ver))?.is_some() {
                continue;
            }
            self.upsert_tool(&ToolRecord {
                name: name.into(),
                version: ver.into(),
                description: desc.into(),
                schema,
                policy: policy.into(),
                capabilities: caps.into_iter().map(str::to_string).collect(),
                enabled: true,
                created_at: Utc::now().to_rfc3339(),
                updated_at: Utc::now().to_rfc3339(),
            })?;
            n += 1;
        }
        Ok(n)
    }

    pub fn upsert_tool(&self, tool: &ToolRecord) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO tools (name, version, description, schema_json, policy, capabilities_json, enabled, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(name, version) DO UPDATE SET
               description=excluded.description,
               schema_json=excluded.schema_json,
               policy=excluded.policy,
               capabilities_json=excluded.capabilities_json,
               enabled=excluded.enabled,
               updated_at=excluded.updated_at",
            params![
                tool.name,
                tool.version,
                tool.description,
                serde_json::to_string(&tool.schema)?,
                tool.policy,
                serde_json::to_string(&tool.capabilities)?,
                if tool.enabled { 1 } else { 0 },
                tool.created_at,
                tool.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn list_tools(&self, enabled_only: bool) -> AtlasResult<Vec<ToolRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let sql = if enabled_only {
            "SELECT name, version, description, schema_json, policy, capabilities_json, enabled, created_at, updated_at
             FROM tools WHERE enabled = 1 ORDER BY name, version DESC"
        } else {
            "SELECT name, version, description, schema_json, policy, capabilities_json, enabled, created_at, updated_at
             FROM tools ORDER BY name, version DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], map_tool)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_tool(&self, name: &str, version: Option<&str>) -> AtlasResult<Option<ToolRecord>> {
        let conn = self.conn.lock().expect("store lock");
        if let Some(ver) = version {
            let mut stmt = conn.prepare(
                "SELECT name, version, description, schema_json, policy, capabilities_json, enabled, created_at, updated_at
                 FROM tools WHERE name = ?1 AND version = ?2",
            )?;
            return Ok(stmt
                .query_row(params![name, ver], map_tool)
                .optional()?);
        }
        let mut stmt = conn.prepare(
            "SELECT name, version, description, schema_json, policy, capabilities_json, enabled, created_at, updated_at
             FROM tools WHERE name = ?1 ORDER BY version DESC LIMIT 1",
        )?;
        Ok(stmt.query_row(params![name], map_tool).optional()?)
    }

    pub fn set_tool_enabled(&self, name: &str, version: &str, enabled: bool) -> AtlasResult<bool> {
        let conn = self.conn.lock().expect("store lock");
        let n = conn.execute(
            "UPDATE tools SET enabled = ?1, updated_at = ?2 WHERE name = ?3 AND version = ?4",
            params![
                if enabled { 1 } else { 0 },
                Utc::now().to_rfc3339(),
                name,
                version
            ],
        )?;
        Ok(n > 0)
    }

    pub fn set_tool_policy(&self, name: &str, version: &str, policy: &str) -> AtlasResult<bool> {
        if !matches!(policy, "allow" | "deny" | "ask") {
            return Err(crate::error::AtlasError::BadRequest(
                "policy must be allow, deny, or ask".into(),
            ));
        }
        let conn = self.conn.lock().expect("store lock");
        let n = conn.execute(
            "UPDATE tools SET policy = ?1, updated_at = ?2 WHERE name = ?3 AND version = ?4",
            params![policy, Utc::now().to_rfc3339(), name, version],
        )?;
        Ok(n > 0)
    }
}

fn map_tool(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolRecord> {
    Ok(ToolRecord {
        name: row.get(0)?,
        version: row.get(1)?,
        description: row.get(2)?,
        schema: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or(Value::Null),
        policy: row.get(4)?,
        capabilities: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
        enabled: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}
