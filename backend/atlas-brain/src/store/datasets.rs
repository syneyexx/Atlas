use crate::error::AtlasResult;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRecord {
    pub id: String,
    pub source: String,
    pub hub_id: String,
    pub revision: String,
    pub title: String,
    pub description: String,
    pub license: String,
    pub modality: String,
    pub gated: bool,
    pub root_hash: String,
    pub status: String,
    pub project_id: Option<String>,
    pub meta: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetShard {
    pub id: String,
    pub dataset_id: String,
    pub name: String,
    pub split: String,
    pub path: String,
    pub size_bytes: i64,
    pub checksum: String,
    pub object_hash: Option<String>,
    pub download_state: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRecipe {
    pub id: String,
    pub dataset_id: String,
    pub version: String,
    pub status: String,
    pub body: Value,
    pub sample_report: Value,
    pub verified: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetJob {
    pub id: String,
    pub dataset_id: String,
    pub recipe_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub phase: String,
    pub checkpoint: Value,
    pub budget: Value,
    pub metrics: Value,
    pub paused: bool,
    pub task_id: Option<String>,
    pub trace_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetEvent {
    pub id: String,
    pub job_id: String,
    pub seq: i64,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub lines: Vec<String>,
    pub details: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetEdge {
    pub id: String,
    pub src: String,
    pub dst: String,
    pub rel: String,
    pub score: f64,
    pub provenance: Value,
    pub auditor_status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRecordMeta {
    pub id: String,
    pub dataset_id: String,
    pub split: String,
    pub row_uid: String,
    pub payload_hash: String,
    pub language: String,
    pub modality: String,
    pub token_count: i64,
    pub quality: f64,
    pub title: String,
    pub tags_json: Value,
    pub contaminated: bool,
    pub meta: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalGuard {
    pub id: String,
    pub name: String,
    pub text_hash: String,
    pub text: String,
    pub meta: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetMix {
    pub id: String,
    pub name: String,
    pub definition: Value,
    pub status: String,
    pub export_manifest: Value,
    pub created_at: String,
    pub updated_at: String,
}

impl Store {
    pub fn migrate_datasets_schema(&self) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS dataset_catalog_cache (
                source TEXT NOT NULL,
                hub_id TEXT NOT NULL,
                card_json TEXT NOT NULL,
                embedding_json TEXT,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(source, hub_id)
            );

            CREATE TABLE IF NOT EXISTS datasets (
                id TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                hub_id TEXT NOT NULL,
                revision TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                license TEXT NOT NULL DEFAULT '',
                modality TEXT NOT NULL DEFAULT 'text',
                gated INTEGER NOT NULL DEFAULT 0,
                root_hash TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'draft',
                project_id TEXT,
                meta_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(source, hub_id, revision)
            );
            CREATE INDEX IF NOT EXISTS idx_datasets_updated ON datasets(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_datasets_project ON datasets(project_id);

            CREATE TABLE IF NOT EXISTS dataset_shards (
                id TEXT PRIMARY KEY,
                dataset_id TEXT NOT NULL,
                name TEXT NOT NULL,
                split TEXT NOT NULL DEFAULT 'train',
                path TEXT NOT NULL,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                checksum TEXT NOT NULL DEFAULT '',
                object_hash TEXT,
                download_state TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                FOREIGN KEY(dataset_id) REFERENCES datasets(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_dataset_shards_ds ON dataset_shards(dataset_id);

            CREATE TABLE IF NOT EXISTS dataset_recipes (
                id TEXT PRIMARY KEY,
                dataset_id TEXT NOT NULL,
                version TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'draft',
                body_json TEXT NOT NULL,
                sample_report_json TEXT NOT NULL DEFAULT '{}',
                verified INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(dataset_id) REFERENCES datasets(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_dataset_recipes_ds ON dataset_recipes(dataset_id);

            CREATE TABLE IF NOT EXISTS dataset_jobs (
                id TEXT PRIMARY KEY,
                dataset_id TEXT NOT NULL,
                recipe_id TEXT,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                phase TEXT NOT NULL,
                checkpoint_json TEXT NOT NULL DEFAULT '{}',
                budget_json TEXT NOT NULL DEFAULT '{}',
                metrics_json TEXT NOT NULL DEFAULT '{}',
                paused INTEGER NOT NULL DEFAULT 0,
                task_id TEXT,
                trace_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY(dataset_id) REFERENCES datasets(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_dataset_jobs_updated ON dataset_jobs(updated_at DESC);

            CREATE TABLE IF NOT EXISTS dataset_events (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                timestamp TEXT NOT NULL,
                event_type TEXT NOT NULL,
                lines_json TEXT NOT NULL,
                details TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY(job_id) REFERENCES dataset_jobs(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_dataset_events_job ON dataset_events(job_id, seq);

            CREATE TABLE IF NOT EXISTS dataset_records (
                id TEXT PRIMARY KEY,
                dataset_id TEXT NOT NULL,
                split TEXT NOT NULL,
                row_uid TEXT NOT NULL,
                payload_hash TEXT NOT NULL,
                language TEXT NOT NULL DEFAULT '',
                modality TEXT NOT NULL DEFAULT 'text',
                token_count INTEGER NOT NULL DEFAULT 0,
                quality REAL NOT NULL DEFAULT 0.5,
                title TEXT NOT NULL DEFAULT '',
                tags_json TEXT NOT NULL DEFAULT '[]',
                contaminated INTEGER NOT NULL DEFAULT 0,
                meta_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                UNIQUE(dataset_id, split, row_uid),
                FOREIGN KEY(dataset_id) REFERENCES datasets(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_dataset_records_ds ON dataset_records(dataset_id);
            CREATE INDEX IF NOT EXISTS idx_dataset_records_hash ON dataset_records(payload_hash);

            CREATE TABLE IF NOT EXISTS dataset_edges (
                id TEXT PRIMARY KEY,
                src TEXT NOT NULL,
                dst TEXT NOT NULL,
                rel TEXT NOT NULL,
                score REAL NOT NULL DEFAULT 0,
                provenance_json TEXT NOT NULL DEFAULT '{}',
                auditor_status TEXT NOT NULL DEFAULT 'unreviewed',
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_dataset_edges_src ON dataset_edges(src);
            CREATE INDEX IF NOT EXISTS idx_dataset_edges_dst ON dataset_edges(dst);
            CREATE INDEX IF NOT EXISTS idx_dataset_edges_rel ON dataset_edges(rel);

            CREATE TABLE IF NOT EXISTS enrichment_cache (
                cache_key TEXT PRIMARY KEY,
                field TEXT NOT NULL,
                value_json TEXT NOT NULL,
                model_id TEXT NOT NULL DEFAULT '',
                confidence REAL NOT NULL DEFAULT 1.0,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS eval_guards (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                text_hash TEXT NOT NULL UNIQUE,
                text TEXT NOT NULL,
                meta_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_eval_guards_name ON eval_guards(name);

            CREATE TABLE IF NOT EXISTS dataset_mixes (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                definition_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'draft',
                export_manifest_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS dataset_records_fts USING fts5(
                record_id UNINDEXED,
                dataset_id UNINDEXED,
                title,
                content,
                tags
            );
            ",
        )?;
        Ok(())
    }

    pub fn datasets_dir(&self) -> std::path::PathBuf {
        self.data_dir.join("datasets")
    }

    // --- catalog ---

    pub fn upsert_catalog_card(&self, source: &str, hub_id: &str, card: &Value) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO dataset_catalog_cache (source, hub_id, card_json, updated_at)
             VALUES (?1,?2,?3,?4)
             ON CONFLICT(source, hub_id) DO UPDATE SET card_json=excluded.card_json, updated_at=excluded.updated_at",
            params![source, hub_id, serde_json::to_string(card)?, now],
        )?;
        Ok(())
    }

    pub fn search_catalog(
        &self,
        query: &str,
        modality: Option<&str>,
        license: Option<&str>,
        limit: usize,
    ) -> AtlasResult<Vec<Value>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 100) as i64;
        let mut stmt = conn.prepare(
            "SELECT card_json FROM dataset_catalog_cache ORDER BY updated_at DESC LIMIT 200",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let q = query.trim().to_lowercase();
        let mut out = Vec::new();
        for row in rows {
            let raw = row?;
            let card: Value = serde_json::from_str(&raw).unwrap_or(json!({}));
            let hay = format!(
                "{} {} {} {}",
                card.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                card.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                card.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                card.get("tags")
                    .map(|t| t.to_string())
                    .unwrap_or_default()
            )
            .to_lowercase();
            if !q.is_empty() && !hay.contains(&q) {
                continue;
            }
            if let Some(m) = modality {
                if card.get("modality").and_then(|v| v.as_str()).unwrap_or("") != m {
                    continue;
                }
            }
            if let Some(lic) = license {
                if !card
                    .get("license")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .eq_ignore_ascii_case(lic)
                {
                    continue;
                }
            }
            out.push(card);
            if out.len() as i64 >= limit {
                break;
            }
        }
        Ok(out)
    }

    pub fn get_catalog_card(&self, source: &str, hub_id: &str) -> AtlasResult<Option<Value>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT card_json FROM dataset_catalog_cache WHERE source=?1 AND hub_id=?2",
        )?;
        Ok(stmt
            .query_row(params![source, hub_id], |row| {
                let raw: String = row.get(0)?;
                Ok(serde_json::from_str(&raw).unwrap_or(json!({})))
            })
            .optional()?)
    }

    // --- datasets ---

    pub fn upsert_dataset(&self, d: &DatasetRecord) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO datasets (
                id, source, hub_id, revision, title, description, license, modality, gated,
                root_hash, status, project_id, meta_json, created_at, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
             ON CONFLICT(id) DO UPDATE SET
                title=excluded.title, description=excluded.description, license=excluded.license,
                modality=excluded.modality, gated=excluded.gated, root_hash=excluded.root_hash,
                status=excluded.status, project_id=excluded.project_id, meta_json=excluded.meta_json,
                updated_at=excluded.updated_at",
            params![
                d.id,
                d.source,
                d.hub_id,
                d.revision,
                d.title,
                d.description,
                d.license,
                d.modality,
                if d.gated { 1 } else { 0 },
                d.root_hash,
                d.status,
                d.project_id,
                serde_json::to_string(&d.meta)?,
                d.created_at,
                d.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_dataset(&self, id: &str) -> AtlasResult<Option<DatasetRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, source, hub_id, revision, title, description, license, modality, gated,
                    root_hash, status, project_id, meta_json, created_at, updated_at
             FROM datasets WHERE id=?1",
        )?;
        Ok(stmt.query_row(params![id], map_dataset).optional()?)
    }

    pub fn get_dataset_by_ref(
        &self,
        source: &str,
        hub_id: &str,
        revision: &str,
    ) -> AtlasResult<Option<DatasetRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, source, hub_id, revision, title, description, license, modality, gated,
                    root_hash, status, project_id, meta_json, created_at, updated_at
             FROM datasets WHERE source=?1 AND hub_id=?2 AND revision=?3",
        )?;
        Ok(stmt
            .query_row(params![source, hub_id, revision], map_dataset)
            .optional()?)
    }

    pub fn list_datasets(&self, limit: usize) -> AtlasResult<Vec<DatasetRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 200) as i64;
        let mut stmt = conn.prepare(
            "SELECT id, source, hub_id, revision, title, description, license, modality, gated,
                    root_hash, status, project_id, meta_json, created_at, updated_at
             FROM datasets ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], map_dataset)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // --- shards ---

    pub fn upsert_shard(&self, s: &DatasetShard) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO dataset_shards (
                id, dataset_id, name, split, path, size_bytes, checksum, object_hash,
                download_state, created_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
             ON CONFLICT(id) DO UPDATE SET
                path=excluded.path, size_bytes=excluded.size_bytes, checksum=excluded.checksum,
                object_hash=excluded.object_hash, download_state=excluded.download_state",
            params![
                s.id,
                s.dataset_id,
                s.name,
                s.split,
                s.path,
                s.size_bytes,
                s.checksum,
                s.object_hash,
                s.download_state,
                s.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_shards(&self, dataset_id: &str) -> AtlasResult<Vec<DatasetShard>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, dataset_id, name, split, path, size_bytes, checksum, object_hash,
                    download_state, created_at
             FROM dataset_shards WHERE dataset_id=?1 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![dataset_id], map_shard)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // --- recipes ---

    pub fn upsert_recipe(&self, r: &DatasetRecipe) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO dataset_recipes (
                id, dataset_id, version, status, body_json, sample_report_json, verified,
                created_at, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(id) DO UPDATE SET
                version=excluded.version, status=excluded.status, body_json=excluded.body_json,
                sample_report_json=excluded.sample_report_json, verified=excluded.verified,
                updated_at=excluded.updated_at",
            params![
                r.id,
                r.dataset_id,
                r.version,
                r.status,
                serde_json::to_string(&r.body)?,
                serde_json::to_string(&r.sample_report)?,
                if r.verified { 1 } else { 0 },
                r.created_at,
                r.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_recipe(&self, id: &str) -> AtlasResult<Option<DatasetRecipe>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, dataset_id, version, status, body_json, sample_report_json, verified,
                    created_at, updated_at
             FROM dataset_recipes WHERE id=?1",
        )?;
        Ok(stmt.query_row(params![id], map_recipe).optional()?)
    }

    pub fn list_recipes(&self, dataset_id: &str) -> AtlasResult<Vec<DatasetRecipe>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, dataset_id, version, status, body_json, sample_report_json, verified,
                    created_at, updated_at
             FROM dataset_recipes WHERE dataset_id=?1 ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map(params![dataset_id], map_recipe)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // --- jobs / events ---

    pub fn create_dataset_job(&self, j: &DatasetJob) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO dataset_jobs (
                id, dataset_id, recipe_id, kind, status, phase, checkpoint_json, budget_json,
                metrics_json, paused, task_id, trace_id, created_at, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                j.id,
                j.dataset_id,
                j.recipe_id,
                j.kind,
                j.status,
                j.phase,
                serde_json::to_string(&j.checkpoint)?,
                serde_json::to_string(&j.budget)?,
                serde_json::to_string(&j.metrics)?,
                if j.paused { 1 } else { 0 },
                j.task_id,
                j.trace_id,
                j.created_at,
                j.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn update_dataset_job(&self, j: &DatasetJob) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "UPDATE dataset_jobs SET dataset_id=?2, recipe_id=?3, kind=?4, status=?5, phase=?6,
                checkpoint_json=?7, budget_json=?8, metrics_json=?9, paused=?10, task_id=?11,
                trace_id=?12, updated_at=?13 WHERE id=?1",
            params![
                j.id,
                j.dataset_id,
                j.recipe_id,
                j.kind,
                j.status,
                j.phase,
                serde_json::to_string(&j.checkpoint)?,
                serde_json::to_string(&j.budget)?,
                serde_json::to_string(&j.metrics)?,
                if j.paused { 1 } else { 0 },
                j.task_id,
                j.trace_id,
                j.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_dataset_job(&self, id: &str) -> AtlasResult<Option<DatasetJob>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, dataset_id, recipe_id, kind, status, phase, checkpoint_json, budget_json,
                    metrics_json, paused, task_id, trace_id, created_at, updated_at
             FROM dataset_jobs WHERE id=?1",
        )?;
        Ok(stmt.query_row(params![id], map_job).optional()?)
    }

    pub fn list_dataset_jobs(&self, limit: usize) -> AtlasResult<Vec<DatasetJob>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 200) as i64;
        let mut stmt = conn.prepare(
            "SELECT id, dataset_id, recipe_id, kind, status, phase, checkpoint_json, budget_json,
                    metrics_json, paused, task_id, trace_id, created_at, updated_at
             FROM dataset_jobs ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], map_job)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn append_dataset_event(
        &self,
        job_id: &str,
        event_type: &str,
        lines: &[String],
        details: Option<&str>,
    ) -> AtlasResult<DatasetEvent> {
        let conn = self.conn.lock().expect("store lock");
        let seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq),0)+1 FROM dataset_events WHERE job_id=?1",
            params![job_id],
            |row| row.get(0),
        )?;
        let now = Utc::now().to_rfc3339();
        let ev = DatasetEvent {
            id: Uuid::new_v4().to_string(),
            job_id: job_id.to_string(),
            seq,
            timestamp: now.clone(),
            event_type: event_type.to_string(),
            lines: lines.to_vec(),
            details: details.map(|s| s.to_string()),
            created_at: now,
        };
        conn.execute(
            "INSERT INTO dataset_events (id, job_id, seq, timestamp, event_type, lines_json, details, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                ev.id,
                ev.job_id,
                ev.seq,
                ev.timestamp,
                ev.event_type,
                serde_json::to_string(&ev.lines)?,
                ev.details,
                ev.created_at
            ],
        )?;
        Ok(ev)
    }

    pub fn list_dataset_events(&self, job_id: &str) -> AtlasResult<Vec<DatasetEvent>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, job_id, seq, timestamp, event_type, lines_json, details, created_at
             FROM dataset_events WHERE job_id=?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![job_id], |row| {
            let lines_raw: String = row.get(5)?;
            Ok(DatasetEvent {
                id: row.get(0)?,
                job_id: row.get(1)?,
                seq: row.get(2)?,
                timestamp: row.get(3)?,
                event_type: row.get(4)?,
                lines: serde_json::from_str(&lines_raw).unwrap_or_default(),
                details: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // --- records ---

    pub fn upsert_dataset_record(&self, r: &DatasetRecordMeta) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO dataset_records (
                id, dataset_id, split, row_uid, payload_hash, language, modality, token_count,
                quality, title, tags_json, contaminated, meta_json, created_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
             ON CONFLICT(dataset_id, split, row_uid) DO UPDATE SET
                payload_hash=excluded.payload_hash, language=excluded.language,
                modality=excluded.modality, token_count=excluded.token_count,
                quality=excluded.quality, title=excluded.title, tags_json=excluded.tags_json,
                contaminated=excluded.contaminated, meta_json=excluded.meta_json",
            params![
                r.id,
                r.dataset_id,
                r.split,
                r.row_uid,
                r.payload_hash,
                r.language,
                r.modality,
                r.token_count,
                r.quality,
                r.title,
                serde_json::to_string(&r.tags_json)?,
                if r.contaminated { 1 } else { 0 },
                serde_json::to_string(&r.meta)?,
                r.created_at
            ],
        )?;
        let content = r
            .meta
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tags = r.tags_json.to_string();
        let _ = conn.execute(
            "DELETE FROM dataset_records_fts WHERE record_id=?1",
            params![r.id],
        );
        let _ = conn.execute(
            "INSERT INTO dataset_records_fts (record_id, dataset_id, title, content, tags)
             VALUES (?1,?2,?3,?4,?5)",
            params![r.id, r.dataset_id, r.title, content, tags],
        );
        Ok(())
    }

    pub fn list_dataset_records(
        &self,
        dataset_id: &str,
        limit: usize,
        offset: usize,
    ) -> AtlasResult<Vec<DatasetRecordMeta>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 500) as i64;
        let offset = offset as i64;
        let mut stmt = conn.prepare(
            "SELECT id, dataset_id, split, row_uid, payload_hash, language, modality, token_count,
                    quality, title, tags_json, contaminated, meta_json, created_at
             FROM dataset_records WHERE dataset_id=?1
             ORDER BY split, row_uid LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![dataset_id, limit, offset], map_record)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn search_dataset_records(
        &self,
        dataset_id: Option<&str>,
        query: &str,
        limit: usize,
    ) -> AtlasResult<Vec<DatasetRecordMeta>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 100) as i64;
        let q = query.trim();
        if q.is_empty() {
            return Ok(vec![]);
        }
        let sql = if dataset_id.is_some() {
            "SELECT r.id, r.dataset_id, r.split, r.row_uid, r.payload_hash, r.language, r.modality,
                    r.token_count, r.quality, r.title, r.tags_json, r.contaminated, r.meta_json, r.created_at
             FROM dataset_records_fts f
             JOIN dataset_records r ON r.id = f.record_id
             WHERE dataset_records_fts MATCH ?1 AND f.dataset_id = ?2
             LIMIT ?3"
        } else {
            "SELECT r.id, r.dataset_id, r.split, r.row_uid, r.payload_hash, r.language, r.modality,
                    r.token_count, r.quality, r.title, r.tags_json, r.contaminated, r.meta_json, r.created_at
             FROM dataset_records_fts f
             JOIN dataset_records r ON r.id = f.record_id
             WHERE dataset_records_fts MATCH ?1
             LIMIT ?2"
        };
        let mut out = Vec::new();
        if let Some(ds) = dataset_id {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map(params![q, ds, limit], map_record)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map(params![q, limit], map_record)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn record_facets(&self, dataset_id: &str) -> AtlasResult<Value> {
        let conn = self.conn.lock().expect("store lock");
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM dataset_records WHERE dataset_id=?1",
            params![dataset_id],
            |row| row.get(0),
        )?;
        let contaminated: i64 = conn.query_row(
            "SELECT COUNT(*) FROM dataset_records WHERE dataset_id=?1 AND contaminated=1",
            params![dataset_id],
            |row| row.get(0),
        )?;
        let mut lang_stmt = conn.prepare(
            "SELECT language, COUNT(*) FROM dataset_records WHERE dataset_id=?1 GROUP BY language",
        )?;
        let lang_rows = lang_stmt.query_map(params![dataset_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut languages = serde_json::Map::new();
        for row in lang_rows {
            let (k, v) = row?;
            languages.insert(k, json!(v));
        }
        let mut split_stmt = conn.prepare(
            "SELECT split, COUNT(*) FROM dataset_records WHERE dataset_id=?1 GROUP BY split",
        )?;
        let split_rows = split_stmt.query_map(params![dataset_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut splits = serde_json::Map::new();
        for row in split_rows {
            let (k, v) = row?;
            splits.insert(k, json!(v));
        }
        Ok(json!({
            "total": total,
            "contaminated": contaminated,
            "languages": languages,
            "splits": splits
        }))
    }

    // --- edges ---

    pub fn upsert_dataset_edge(&self, e: &DatasetEdge) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO dataset_edges (id, src, dst, rel, score, provenance_json, auditor_status, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(id) DO UPDATE SET score=excluded.score, provenance_json=excluded.provenance_json,
                auditor_status=excluded.auditor_status",
            params![
                e.id,
                e.src,
                e.dst,
                e.rel,
                e.score,
                serde_json::to_string(&e.provenance)?,
                e.auditor_status,
                e.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_dataset_edges(
        &self,
        node: Option<&str>,
        rel: Option<&str>,
        limit: usize,
    ) -> AtlasResult<Vec<DatasetEdge>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 500) as i64;
        let mut sql = String::from(
            "SELECT id, src, dst, rel, score, provenance_json, auditor_status, created_at
             FROM dataset_edges WHERE 1=1",
        );
        if node.is_some() {
            sql.push_str(" AND (src=?1 OR dst=?1)");
        }
        if rel.is_some() {
            if node.is_some() {
                sql.push_str(" AND rel=?2");
            } else {
                sql.push_str(" AND rel=?1");
            }
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ");
        sql.push_str(&limit.to_string());

        let mut out = Vec::new();
        match (node, rel) {
            (Some(n), Some(r)) => {
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![n, r], map_edge)?;
                for row in rows {
                    out.push(row?);
                }
            }
            (Some(n), None) => {
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![n], map_edge)?;
                for row in rows {
                    out.push(row?);
                }
            }
            (None, Some(r)) => {
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![r], map_edge)?;
                for row in rows {
                    out.push(row?);
                }
            }
            (None, None) => {
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map([], map_edge)?;
                for row in rows {
                    out.push(row?);
                }
            }
        }
        Ok(out)
    }

    // --- enrichment cache ---

    pub fn get_enrichment_cache(&self, key: &str) -> AtlasResult<Option<(Value, f64, String)>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT value_json, confidence, model_id FROM enrichment_cache WHERE cache_key=?1",
        )?;
        Ok(stmt
            .query_row(params![key], |row| {
                let raw: String = row.get(0)?;
                let conf: f64 = row.get(1)?;
                let model: String = row.get(2)?;
                Ok((
                    serde_json::from_str(&raw).unwrap_or(json!(null)),
                    conf,
                    model,
                ))
            })
            .optional()?)
    }

    pub fn put_enrichment_cache(
        &self,
        key: &str,
        field: &str,
        value: &Value,
        model_id: &str,
        confidence: f64,
    ) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO enrichment_cache (cache_key, field, value_json, model_id, confidence, created_at)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(cache_key) DO UPDATE SET value_json=excluded.value_json,
                model_id=excluded.model_id, confidence=excluded.confidence",
            params![
                key,
                field,
                serde_json::to_string(value)?,
                model_id,
                confidence,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    // --- eval guards ---

    pub fn upsert_eval_guard(&self, g: &EvalGuard) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO eval_guards (id, name, text_hash, text, meta_json, created_at)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(text_hash) DO UPDATE SET name=excluded.name, text=excluded.text,
                meta_json=excluded.meta_json",
            params![
                g.id,
                g.name,
                g.text_hash,
                g.text,
                serde_json::to_string(&g.meta)?,
                g.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_eval_guards(&self, name: Option<&str>) -> AtlasResult<Vec<EvalGuard>> {
        let conn = self.conn.lock().expect("store lock");
        let mut out = Vec::new();
        if let Some(n) = name {
            let mut stmt = conn.prepare(
                "SELECT id, name, text_hash, text, meta_json, created_at FROM eval_guards WHERE name=?1",
            )?;
            let rows = stmt.query_map(params![n], map_guard)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, name, text_hash, text, meta_json, created_at FROM eval_guards ORDER BY name",
            )?;
            let rows = stmt.query_map([], map_guard)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn eval_guard_hashes(&self) -> AtlasResult<std::collections::HashSet<String>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare("SELECT text_hash FROM eval_guards")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut set = std::collections::HashSet::new();
        for row in rows {
            set.insert(row?);
        }
        Ok(set)
    }

    // --- mixes ---

    pub fn upsert_mix(&self, m: &DatasetMix) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute(
            "INSERT INTO dataset_mixes (id, name, definition_json, status, export_manifest_json, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, definition_json=excluded.definition_json,
                status=excluded.status, export_manifest_json=excluded.export_manifest_json,
                updated_at=excluded.updated_at",
            params![
                m.id,
                m.name,
                serde_json::to_string(&m.definition)?,
                m.status,
                serde_json::to_string(&m.export_manifest)?,
                m.created_at,
                m.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_mix(&self, id: &str) -> AtlasResult<Option<DatasetMix>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, name, definition_json, status, export_manifest_json, created_at, updated_at
             FROM dataset_mixes WHERE id=?1",
        )?;
        Ok(stmt.query_row(params![id], map_mix).optional()?)
    }

    pub fn list_mixes(&self, limit: usize) -> AtlasResult<Vec<DatasetMix>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 100) as i64;
        let mut stmt = conn.prepare(
            "SELECT id, name, definition_json, status, export_manifest_json, created_at, updated_at
             FROM dataset_mixes ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], map_mix)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn seed_default_eval_guards(&self) -> AtlasResult<usize> {
        let samples = [
            ("mmlu", "What is the capital of France? Paris is the capital."),
            ("hellaswag", "A person walks into a kitchen and opens the fridge to get milk."),
            ("gsm8k", "Janet’s ducks lay 16 eggs per day. She eats three for breakfast."),
        ];
        let mut n = 0;
        for (name, text) in samples {
            let hash = Store::content_hash(text.as_bytes());
            if self
                .list_eval_guards(Some(name))?
                .iter()
                .any(|g| g.text_hash == hash)
            {
                continue;
            }
            self.upsert_eval_guard(&EvalGuard {
                id: Uuid::new_v4().to_string(),
                name: name.into(),
                text_hash: hash,
                text: text.into(),
                meta: json!({ "seed": true }),
                created_at: Utc::now().to_rfc3339(),
            })?;
            n += 1;
        }
        Ok(n)
    }
}

fn map_dataset(row: &rusqlite::Row<'_>) -> rusqlite::Result<DatasetRecord> {
    let meta_raw: String = row.get(12)?;
    Ok(DatasetRecord {
        id: row.get(0)?,
        source: row.get(1)?,
        hub_id: row.get(2)?,
        revision: row.get(3)?,
        title: row.get(4)?,
        description: row.get(5)?,
        license: row.get(6)?,
        modality: row.get(7)?,
        gated: row.get::<_, i64>(8)? != 0,
        root_hash: row.get(9)?,
        status: row.get(10)?,
        project_id: row.get(11)?,
        meta: serde_json::from_str(&meta_raw).unwrap_or(json!({})),
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

fn map_shard(row: &rusqlite::Row<'_>) -> rusqlite::Result<DatasetShard> {
    Ok(DatasetShard {
        id: row.get(0)?,
        dataset_id: row.get(1)?,
        name: row.get(2)?,
        split: row.get(3)?,
        path: row.get(4)?,
        size_bytes: row.get(5)?,
        checksum: row.get(6)?,
        object_hash: row.get(7)?,
        download_state: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn map_recipe(row: &rusqlite::Row<'_>) -> rusqlite::Result<DatasetRecipe> {
    let body_raw: String = row.get(4)?;
    let report_raw: String = row.get(5)?;
    Ok(DatasetRecipe {
        id: row.get(0)?,
        dataset_id: row.get(1)?,
        version: row.get(2)?,
        status: row.get(3)?,
        body: serde_json::from_str(&body_raw).unwrap_or(json!({})),
        sample_report: serde_json::from_str(&report_raw).unwrap_or(json!({})),
        verified: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<DatasetJob> {
    let cp: String = row.get(6)?;
    let bud: String = row.get(7)?;
    let met: String = row.get(8)?;
    Ok(DatasetJob {
        id: row.get(0)?,
        dataset_id: row.get(1)?,
        recipe_id: row.get(2)?,
        kind: row.get(3)?,
        status: row.get(4)?,
        phase: row.get(5)?,
        checkpoint: serde_json::from_str(&cp).unwrap_or(json!({})),
        budget: serde_json::from_str(&bud).unwrap_or(json!({})),
        metrics: serde_json::from_str(&met).unwrap_or(json!({})),
        paused: row.get::<_, i64>(9)? != 0,
        task_id: row.get(10)?,
        trace_id: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<DatasetRecordMeta> {
    let tags_raw: String = row.get(10)?;
    let meta_raw: String = row.get(12)?;
    Ok(DatasetRecordMeta {
        id: row.get(0)?,
        dataset_id: row.get(1)?,
        split: row.get(2)?,
        row_uid: row.get(3)?,
        payload_hash: row.get(4)?,
        language: row.get(5)?,
        modality: row.get(6)?,
        token_count: row.get(7)?,
        quality: row.get(8)?,
        title: row.get(9)?,
        tags_json: serde_json::from_str(&tags_raw).unwrap_or(json!([])),
        contaminated: row.get::<_, i64>(11)? != 0,
        meta: serde_json::from_str(&meta_raw).unwrap_or(json!({})),
        created_at: row.get(13)?,
    })
}

fn map_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<DatasetEdge> {
    let prov: String = row.get(5)?;
    Ok(DatasetEdge {
        id: row.get(0)?,
        src: row.get(1)?,
        dst: row.get(2)?,
        rel: row.get(3)?,
        score: row.get(4)?,
        provenance: serde_json::from_str(&prov).unwrap_or(json!({})),
        auditor_status: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn map_guard(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvalGuard> {
    let meta_raw: String = row.get(4)?;
    Ok(EvalGuard {
        id: row.get(0)?,
        name: row.get(1)?,
        text_hash: row.get(2)?,
        text: row.get(3)?,
        meta: serde_json::from_str(&meta_raw).unwrap_or(json!({})),
        created_at: row.get(5)?,
    })
}

fn map_mix(row: &rusqlite::Row<'_>) -> rusqlite::Result<DatasetMix> {
    let def: String = row.get(2)?;
    let man: String = row.get(4)?;
    Ok(DatasetMix {
        id: row.get(0)?,
        name: row.get(1)?,
        definition: serde_json::from_str(&def).unwrap_or(json!({})),
        status: row.get(3)?,
        export_manifest: serde_json::from_str(&man).unwrap_or(json!({})),
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}
