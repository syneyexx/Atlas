use crate::error::AtlasResult;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub plan: Value,
    pub state: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCheckpoint {
    pub id: String,
    pub task_id: String,
    pub label: String,
    pub snapshot: Value,
    pub created_at: String,
}

impl Store {
    pub fn create_task(&self, task: &TaskRecord) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO tasks (id, title, status, conversation_id, parent_id, plan_json, state_json, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                task.id,
                task.title,
                task.status,
                task.conversation_id,
                task.parent_id,
                serde_json::to_string(&task.plan)?,
                serde_json::to_string(&task.state)?,
                task.created_at,
                task.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_task(&self, id: &str) -> AtlasResult<Option<TaskRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, title, status, conversation_id, parent_id, plan_json, state_json, created_at, updated_at
             FROM tasks WHERE id = ?1",
        )?;
        Ok(stmt.query_row(params![id], map_task).optional()?)
    }

    pub fn list_tasks(
        &self,
        status: Option<&str>,
        limit: usize,
    ) -> AtlasResult<Vec<TaskRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 200) as i64;
        let mut out = Vec::new();
        if let Some(st) = status {
            let mut stmt = conn.prepare(
                "SELECT id, title, status, conversation_id, parent_id, plan_json, state_json, created_at, updated_at
                 FROM tasks WHERE status = ?1 ORDER BY updated_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![st, limit], map_task)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, title, status, conversation_id, parent_id, plan_json, state_json, created_at, updated_at
                 FROM tasks ORDER BY updated_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], map_task)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn update_task_status(&self, id: &str, status: &str, state: Option<&Value>) -> AtlasResult<bool> {
        let conn = self.conn.lock().expect("store lock");
        let now = Utc::now().to_rfc3339();
        let n = if let Some(st) = state {
            conn.execute(
                "UPDATE tasks SET status = ?1, state_json = ?2, updated_at = ?3 WHERE id = ?4",
                params![status, serde_json::to_string(st)?, now, id],
            )?
        } else {
            conn.execute(
                "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status, now, id],
            )?
        };
        Ok(n > 0)
    }

    pub fn add_checkpoint(
        &self,
        task_id: &str,
        label: &str,
        snapshot: &Value,
    ) -> AtlasResult<TaskCheckpoint> {
        let cp = TaskCheckpoint {
            id: Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            label: label.to_string(),
            snapshot: snapshot.clone(),
            created_at: Utc::now().to_rfc3339(),
        };
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO task_checkpoints (id, task_id, label, snapshot_json, created_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![
                cp.id,
                cp.task_id,
                cp.label,
                serde_json::to_string(&cp.snapshot)?,
                cp.created_at
            ],
        )?;
        // Touch task updated_at
        conn.execute(
            "UPDATE tasks SET updated_at = ?1 WHERE id = ?2",
            params![cp.created_at, task_id],
        )?;
        Ok(cp)
    }

    pub fn list_checkpoints(&self, task_id: &str) -> AtlasResult<Vec<TaskCheckpoint>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, task_id, label, snapshot_json, created_at
             FROM task_checkpoints WHERE task_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![task_id], |row| {
            Ok(TaskCheckpoint {
                id: row.get(0)?,
                task_id: row.get(1)?,
                label: row.get(2)?,
                snapshot: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or(Value::Null),
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

fn map_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        status: row.get(2)?,
        conversation_id: row.get(3)?,
        parent_id: row.get(4)?,
        plan: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or(Value::Null),
        state: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or(Value::Null),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}
