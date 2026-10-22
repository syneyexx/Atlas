//! Gap stores 21–25 + secrets + code index (platform plane).
use crate::error::{AtlasError, AtlasResult};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use chrono::Utc;
use rand::RngCore;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::db::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub meta: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub kind: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub meta: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEntry {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub token_in: i64,
    pub token_out: i64,
    pub latency_ms: i64,
    pub cost_usd: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeFileRecord {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub path: String,
    pub language: String,
    pub content_hash: String,
    pub size_bytes: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct CodeSymbol {
    pub id: String,
    pub file_id: String,
    pub name: String,
    pub kind: String,
    pub line_start: i64,
    pub line_end: i64,
    pub signature: String,
}

impl Store {
    pub fn migrate_platform_schema(&self) -> AtlasResult<()> {
        let conn = self.conn.lock().expect("store lock");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL DEFAULT '',
                meta_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                conversation_id TEXT,
                task_id TEXT,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                object_hash TEXT,
                content TEXT,
                meta_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_artifacts_project ON artifacts(project_id, created_at);
            CREATE TABLE IF NOT EXISTS cost_ledger (
                id TEXT PRIMARY KEY,
                trace_id TEXT,
                conversation_id TEXT,
                model_id TEXT,
                token_in INTEGER NOT NULL DEFAULT 0,
                token_out INTEGER NOT NULL DEFAULT 0,
                latency_ms INTEGER NOT NULL DEFAULT 0,
                cost_usd REAL NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cost_created ON cost_ledger(created_at DESC);
            CREATE TABLE IF NOT EXISTS secrets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                ciphertext TEXT NOT NULL,
                nonce TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS code_files (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                path TEXT NOT NULL,
                language TEXT NOT NULL DEFAULT '',
                content_hash TEXT NOT NULL,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                UNIQUE(project_id, path)
            );
            CREATE TABLE IF NOT EXISTS code_symbols (
                id TEXT PRIMARY KEY,
                file_id TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                line_start INTEGER NOT NULL,
                line_end INTEGER NOT NULL,
                signature TEXT NOT NULL DEFAULT '',
                FOREIGN KEY(file_id) REFERENCES code_files(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_code_symbols_name ON code_symbols(name);
            CREATE VIRTUAL TABLE IF NOT EXISTS code_symbols_fts USING fts5(
                symbol_id UNINDEXED,
                name,
                signature,
                path UNINDEXED,
                tokenize = 'unicode61'
            );
            ",
        )?;
        // Trace cost columns (M4 gap → M5).
        let _ = conn.execute("ALTER TABLE traces ADD COLUMN token_in INTEGER", []);
        let _ = conn.execute("ALTER TABLE traces ADD COLUMN token_out INTEGER", []);
        let _ = conn.execute("ALTER TABLE traces ADD COLUMN latency_ms INTEGER", []);
        let _ = conn.execute("ALTER TABLE traces ADD COLUMN cost_usd REAL", []);
        let _ = conn.execute("ALTER TABLE conversations ADD COLUMN project_id TEXT", []);
        let _ = conn.execute("ALTER TABLE memory_nodes ADD COLUMN project_id TEXT", []);
        Ok(())
    }

    pub fn upsert_project(&self, p: &ProjectRecord) -> AtlasResult<()> {
        {
            let conn = self.conn.lock().expect("store lock");
            conn.execute(
                "INSERT INTO projects (id, name, slug, description, meta_json, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(id) DO UPDATE SET
                   name=excluded.name, slug=excluded.slug, description=excluded.description,
                   meta_json=excluded.meta_json, updated_at=excluded.updated_at",
                params![
                    p.id,
                    p.name,
                    p.slug,
                    p.description,
                    serde_json::to_string(&p.meta)?,
                    p.created_at,
                    p.updated_at
                ],
            )?;
        }
        if let Some(pg) = &self.engines.postgres {
            if let Err(err) = crate::store::engines::pg_mirror::mirror_project(pg, p) {
                tracing::warn!("postgres mirror project failed: {err}");
            }
        }
        Ok(())
    }

    pub fn list_projects(&self) -> AtlasResult<Vec<ProjectRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, name, slug, description, meta_json, created_at, updated_at
             FROM projects ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProjectRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                slug: row.get(2)?,
                description: row.get(3)?,
                meta: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or(serde_json::json!({})),
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_project(&self, id: &str) -> AtlasResult<Option<ProjectRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, name, slug, description, meta_json, created_at, updated_at
             FROM projects WHERE id = ?1",
        )?;
        Ok(stmt
            .query_row(params![id], |row| {
                Ok(ProjectRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    slug: row.get(2)?,
                    description: row.get(3)?,
                    meta: serde_json::from_str(&row.get::<_, String>(4)?)
                        .unwrap_or(serde_json::json!({})),
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .optional()?)
    }

    pub fn insert_artifact(&self, a: &ArtifactRecord) -> AtlasResult<()> {
        {
            let conn = self.conn.lock().expect("store lock");
            conn.execute(
                "INSERT INTO artifacts (id, project_id, conversation_id, task_id, kind, title, object_hash, content, meta_json, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    a.id,
                    a.project_id,
                    a.conversation_id,
                    a.task_id,
                    a.kind,
                    a.title,
                    a.object_hash,
                    a.content,
                    serde_json::to_string(&a.meta)?,
                    a.created_at
                ],
            )?;
        }
        if let Some(pg) = &self.engines.postgres {
            if let Err(err) = crate::store::engines::pg_mirror::mirror_artifact(pg, a) {
                tracing::warn!("postgres mirror artifact failed: {err}");
            }
        }
        Ok(())
    }

    pub fn list_artifacts(
        &self,
        project_id: Option<&str>,
        limit: usize,
    ) -> AtlasResult<Vec<ArtifactRecord>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 200) as i64;
        let mut out = Vec::new();
        if let Some(pid) = project_id {
            let mut stmt = conn.prepare(
                "SELECT id, project_id, conversation_id, task_id, kind, title, object_hash, content, meta_json, created_at
                 FROM artifacts WHERE project_id = ?1 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![pid, limit], map_artifact)?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, project_id, conversation_id, task_id, kind, title, object_hash, content, meta_json, created_at
                 FROM artifacts ORDER BY created_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], map_artifact)?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    pub fn insert_cost(&self, e: &CostEntry) -> AtlasResult<()> {
        {
            let conn = self.conn.lock().expect("store lock");
            conn.execute(
                "INSERT INTO cost_ledger (id, trace_id, conversation_id, model_id, token_in, token_out, latency_ms, cost_usd, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    e.id,
                    e.trace_id,
                    e.conversation_id,
                    e.model_id,
                    e.token_in,
                    e.token_out,
                    e.latency_ms,
                    e.cost_usd,
                    e.created_at
                ],
            )?;
        }
        if let Some(pg) = &self.engines.postgres {
            if let Err(err) = crate::store::engines::pg_mirror::mirror_cost(pg, e) {
                tracing::warn!("postgres mirror cost failed: {err}");
            }
        }
        Ok(())
    }

    pub fn list_costs(&self, limit: usize) -> AtlasResult<Vec<CostEntry>> {
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 500) as i64;
        let mut stmt = conn.prepare(
            "SELECT id, trace_id, conversation_id, model_id, token_in, token_out, latency_ms, cost_usd, created_at
             FROM cost_ledger ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(CostEntry {
                id: row.get(0)?,
                trace_id: row.get(1)?,
                conversation_id: row.get(2)?,
                model_id: row.get(3)?,
                token_in: row.get(4)?,
                token_out: row.get(5)?,
                latency_ms: row.get(6)?,
                cost_usd: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn put_secret(&self, name: &str, plaintext: &str, key_material: &str) -> AtlasResult<()> {
        let key = derive_secrets_key(key_material)?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| AtlasError::Upstream(format!("encrypt secret: {e}")))?;
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        let ct_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &ct);
        let nonce_b64 =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, nonce_bytes);
        {
            let conn = self.conn.lock().expect("store lock");
            conn.execute(
                "INSERT INTO secrets (id, name, ciphertext, nonce, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?5)
                 ON CONFLICT(name) DO UPDATE SET
                   ciphertext=excluded.ciphertext,
                   nonce=excluded.nonce,
                   updated_at=excluded.updated_at",
                params![id, name, ct_b64, nonce_b64, now],
            )?;
        }
        if let Some(pg) = &self.engines.postgres {
            if let Err(err) =
                crate::store::engines::pg_mirror::mirror_secret(pg, &id, name, &ct_b64, &nonce_b64, &now)
            {
                tracing::warn!("postgres mirror secret failed: {err}");
            }
        }
        Ok(())
    }

    pub fn get_secret(&self, name: &str, key_material: &str) -> AtlasResult<Option<String>> {
        let conn = self.conn.lock().expect("store lock");
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT ciphertext, nonce FROM secrets WHERE name = ?1",
                params![name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((ct_b64, nonce_b64)) = row else {
            return Ok(None);
        };
        let key = derive_secrets_key(key_material)?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let ct = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, ct_b64)
            .map_err(|e| AtlasError::Upstream(format!("b64 ct: {e}")))?;
        let nonce_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, nonce_b64)
            .map_err(|e| AtlasError::Upstream(format!("b64 nonce: {e}")))?;
        if nonce_bytes.len() != 12 {
            return Err(AtlasError::Upstream("invalid secret nonce".into()));
        }
        let nonce = Nonce::from_slice(&nonce_bytes);
        let pt = cipher
            .decrypt(nonce, ct.as_ref())
            .map_err(|e| AtlasError::Upstream(format!("decrypt secret: {e}")))?;
        Ok(Some(
            String::from_utf8(pt).map_err(|e| AtlasError::Upstream(format!("secret utf8: {e}")))?,
        ))
    }

    pub fn list_secret_names(&self) -> AtlasResult<Vec<String>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare("SELECT name FROM secrets ORDER BY name")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Index a source file: store file row + extract coarse symbols (fn/struct/class/def).
    pub fn index_code_file(
        &self,
        project_id: Option<&str>,
        path: &str,
        language: &str,
        content: &str,
    ) -> AtlasResult<(CodeFileRecord, usize)> {
        let hash = {
            let mut h = Sha256::new();
            h.update(content.as_bytes());
            hex::encode(h.finalize())
        };
        let now = Utc::now().to_rfc3339();
        let file_id = Uuid::new_v4().to_string();
        let file = CodeFileRecord {
            id: file_id.clone(),
            project_id: project_id.map(str::to_string),
            path: path.to_string(),
            language: language.to_string(),
            content_hash: hash,
            size_bytes: content.len() as i64,
            updated_at: now.clone(),
        };
        {
            let conn = self.conn.lock().expect("store lock");
            // Replace prior file at same path.
            let old: Option<String> = conn
                .query_row(
                    "SELECT id FROM code_files WHERE IFNULL(project_id,'') = IFNULL(?1,'') AND path = ?2",
                    params![project_id, path],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(oid) = old {
                let _ = conn.execute("DELETE FROM code_symbols_fts WHERE path = ?1", params![path]);
                let _ = conn.execute("DELETE FROM code_symbols WHERE file_id = ?1", params![oid]);
                let _ = conn.execute("DELETE FROM code_files WHERE id = ?1", params![oid]);
            }
            conn.execute(
                "INSERT INTO code_files (id, project_id, path, language, content_hash, size_bytes, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    file.id,
                    file.project_id,
                    file.path,
                    file.language,
                    file.content_hash,
                    file.size_bytes,
                    file.updated_at
                ],
            )?;
        }
        let symbols = extract_symbols(content, language);
        let mut n = 0usize;
        for (line, kind, name, signature) in symbols {
            let sid = Uuid::new_v4().to_string();
            let conn = self.conn.lock().expect("store lock");
            conn.execute(
                "INSERT INTO code_symbols (id, file_id, name, kind, line_start, line_end, signature)
                 VALUES (?1,?2,?3,?4,?5,?5,?6)",
                params![sid, file_id, name.clone(), kind, line, signature.clone()],
            )?;
            conn.execute(
                "INSERT INTO code_symbols_fts (symbol_id, name, signature, path)
                 VALUES (?1,?2,?3,?4)",
                params![sid, name, signature, path],
            )?;
            n += 1;
        }
        Ok((file, n))
    }

    pub fn search_code(&self, query: &str, limit: usize) -> AtlasResult<Vec<Value>> {
        let q = super::conversations::sanitize_fts_query(query);
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().expect("store lock");
        let limit = limit.clamp(1, 100) as i64;
        let mut stmt = conn.prepare(
            "SELECT s.id, s.name, s.kind, s.line_start, s.signature, f.path, f.language
             FROM code_symbols_fts fts
             JOIN code_symbols s ON s.id = fts.symbol_id
             JOIN code_files f ON f.id = s.file_id
             WHERE code_symbols_fts MATCH ?1
             ORDER BY bm25(code_symbols_fts) LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![q, limit], |row| {
            Ok(serde_json::json!({
                "symbol_id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "kind": row.get::<_, String>(2)?,
                "line": row.get::<_, i64>(3)?,
                "signature": row.get::<_, String>(4)?,
                "path": row.get::<_, String>(5)?,
                "language": row.get::<_, String>(6)?,
            }))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

fn map_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactRecord> {
    Ok(ArtifactRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        conversation_id: row.get(2)?,
        task_id: row.get(3)?,
        kind: row.get(4)?,
        title: row.get(5)?,
        object_hash: row.get(6)?,
        content: row.get(7)?,
        meta: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or(serde_json::json!({})),
        created_at: row.get(9)?,
    })
}

fn derive_secrets_key(material: &str) -> AtlasResult<[u8; 32]> {
    if material.trim().is_empty() {
        return Err(AtlasError::BadRequest(
            "secrets_key / ATLAS_SECRETS_KEY is verplicht voor secrets store".into(),
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(material.as_bytes());
    let dig = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&dig);
    Ok(key)
}

/// Coarse language-agnostic symbol scrape — good enough for coding-agent retrieval.
fn extract_symbols(content: &str, _language: &str) -> Vec<(i64, String, String, String)> {
    let mut out = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let line_no = (idx + 1) as i64;
        let t = line.trim();
        let (kind, rest) = if let Some(r) = t.strip_prefix("fn ") {
            ("fn", r)
        } else if let Some(r) = t.strip_prefix("pub fn ") {
            ("fn", r)
        } else if let Some(r) = t.strip_prefix("struct ") {
            ("struct", r)
        } else if let Some(r) = t.strip_prefix("pub struct ") {
            ("struct", r)
        } else if let Some(r) = t.strip_prefix("class ") {
            ("class", r)
        } else if let Some(r) = t.strip_prefix("def ") {
            ("def", r)
        } else if let Some(r) = t.strip_prefix("function ") {
            ("function", r)
        } else if let Some(r) = t.strip_prefix("export function ") {
            ("function", r)
        } else if let Some(r) = t.strip_prefix("export const ") {
            ("const", r)
        } else if let Some(r) = t.strip_prefix("const ") {
            ("const", r)
        } else if let Some(r) = t.strip_prefix("impl ") {
            ("impl", r)
        } else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if name.len() < 2 {
            continue;
        }
        out.push((line_no, kind.to_string(), name, t.chars().take(160).collect()));
        if out.len() >= 400 {
            break;
        }
    }
    out
}
