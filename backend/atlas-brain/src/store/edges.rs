use crate::error::AtlasResult;
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEdge {
    pub id: String,
    pub src_id: String,
    pub rel: String,
    pub dst_id: String,
    pub weight: f64,
    pub provenance: Value,
    pub created_at: String,
}

impl Store {
    pub fn upsert_edge(
        &self,
        src_id: &str,
        rel: &str,
        dst_id: &str,
        weight: f64,
        provenance: &Value,
    ) -> AtlasResult<MemoryEdge> {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let weight = weight.clamp(0.0, 10.0);
        let prov = serde_json::to_string(provenance)?;
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO memory_edges (id, src_id, rel, dst_id, weight, provenance_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(src_id, rel, dst_id) DO UPDATE SET
               weight=excluded.weight,
               provenance_json=excluded.provenance_json",
            params![id, src_id, rel, dst_id, weight, prov, created_at],
        )?;
        // Re-read canonical id after upsert (conflict keeps old id).
        let mut stmt = conn.prepare(
            "SELECT id, src_id, rel, dst_id, weight, provenance_json, created_at
             FROM memory_edges WHERE src_id = ?1 AND rel = ?2 AND dst_id = ?3",
        )?;
        let edge = stmt.query_row(params![src_id, rel, dst_id], map_edge)?;
        Ok(edge)
    }

    pub fn list_edges_from(&self, src_id: &str) -> AtlasResult<Vec<MemoryEdge>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, src_id, rel, dst_id, weight, provenance_json, created_at
             FROM memory_edges WHERE src_id = ?1 ORDER BY weight DESC",
        )?;
        let rows = stmt.query_map(params![src_id], map_edge)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_edges_to(&self, dst_id: &str) -> AtlasResult<Vec<MemoryEdge>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, src_id, rel, dst_id, weight, provenance_json, created_at
             FROM memory_edges WHERE dst_id = ?1 ORDER BY weight DESC",
        )?;
        let rows = stmt.query_map(params![dst_id], map_edge)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn neighbor_ids(&self, node_id: &str, limit: usize) -> AtlasResult<Vec<String>> {
        let mut ids = Vec::new();
        for e in self.list_edges_from(node_id)? {
            ids.push(e.dst_id);
            if ids.len() >= limit {
                return Ok(ids);
            }
        }
        for e in self.list_edges_to(node_id)? {
            if !ids.contains(&e.src_id) {
                ids.push(e.src_id);
            }
            if ids.len() >= limit {
                break;
            }
        }
        Ok(ids)
    }
}

fn map_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEdge> {
    Ok(MemoryEdge {
        id: row.get(0)?,
        src_id: row.get(1)?,
        rel: row.get(2)?,
        dst_id: row.get(3)?,
        weight: row.get(4)?,
        provenance: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or(Value::Null),
        created_at: row.get(6)?,
    })
}
