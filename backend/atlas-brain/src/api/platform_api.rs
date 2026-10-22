use crate::error::{AtlasError, AtlasResult};
use crate::state::AppState;
use crate::store::{ArtifactRecord, CostEntry, ProjectRecord};
use axum::extract::{Path, Query, State};
use axum::Json;
use base64::Engine;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ProjectBody {
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub meta: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct ArtifactBody {
    pub title: String,
    pub kind: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub meta: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SecretBody {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct CodeIndexBody {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CodeSearchQuery {
    pub q: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

pub async fn engines(State(state): State<AppState>) -> AtlasResult<Json<Value>> {
    Ok(Json(state.store.engine_status()))
}

pub async fn list_projects(State(state): State<AppState>) -> AtlasResult<Json<Vec<ProjectRecord>>> {
    Ok(Json(state.store.list_projects()?))
}

pub async fn upsert_project(
    State(state): State<AppState>,
    Json(body): Json<ProjectBody>,
) -> AtlasResult<Json<ProjectRecord>> {
    if body.name.trim().is_empty() {
        return Err(AtlasError::BadRequest("project name is verplicht".into()));
    }
    let slug = body
        .slug
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| slugify(&body.name));
    let now = Utc::now().to_rfc3339();
    let project = ProjectRecord {
        id: Uuid::new_v4().to_string(),
        name: body.name.trim().to_string(),
        slug,
        description: body.description.unwrap_or_default(),
        meta: body.meta.unwrap_or_else(|| json!({})),
        created_at: now.clone(),
        updated_at: now,
    };
    state.store.upsert_project(&project)?;
    Ok(Json(project))
}

pub async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<ProjectRecord>> {
    state
        .store
        .get_project(&id)?
        .map(Json)
        .ok_or_else(|| AtlasError::NotFound(format!("Project {id} niet gevonden")))
}

pub async fn create_artifact(
    State(state): State<AppState>,
    Json(body): Json<ArtifactBody>,
) -> AtlasResult<Json<ArtifactRecord>> {
    let cfg = state.config.read().await.clone();
    let mut object_hash = None;
    if let Some(ref content) = body.content {
        if !content.is_empty() {
            let (hash, _, _) = state.store.put_object(&cfg.data_dir, content.as_bytes())?;
            object_hash = Some(hash);
        }
    }
    let art = ArtifactRecord {
        id: Uuid::new_v4().to_string(),
        project_id: body.project_id,
        conversation_id: body.conversation_id,
        task_id: body.task_id,
        kind: body.kind,
        title: body.title,
        object_hash,
        content: body.content,
        meta: body.meta.unwrap_or_else(|| json!({})),
        created_at: Utc::now().to_rfc3339(),
    };
    state.store.insert_artifact(&art)?;
    Ok(Json(art))
}

pub async fn list_artifacts(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> AtlasResult<Json<Vec<ArtifactRecord>>> {
    Ok(Json(state.store.list_artifacts(
        q.project_id.as_deref(),
        q.limit.unwrap_or(50),
    )?))
}

pub async fn list_costs(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> AtlasResult<Json<Vec<CostEntry>>> {
    Ok(Json(state.store.list_costs(q.limit.unwrap_or(100))?))
}

pub async fn put_secret(
    State(state): State<AppState>,
    Json(body): Json<SecretBody>,
) -> AtlasResult<Json<Value>> {
    let cfg = state.config.read().await.clone();
    state
        .store
        .put_secret(&body.name, &body.value, &cfg.secrets_key)?;
    Ok(Json(json!({ "ok": true, "name": body.name })))
}

pub async fn get_secret(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AtlasResult<Json<Value>> {
    let cfg = state.config.read().await.clone();
    let value = state
        .store
        .get_secret(&name, &cfg.secrets_key)?
        .ok_or_else(|| AtlasError::NotFound(format!("Secret {name} niet gevonden")))?;
    Ok(Json(json!({ "name": name, "value": value })))
}

pub async fn list_secrets(State(state): State<AppState>) -> AtlasResult<Json<Value>> {
    Ok(Json(json!({ "names": state.store.list_secret_names()? })))
}

pub async fn index_code(
    State(state): State<AppState>,
    Json(body): Json<CodeIndexBody>,
) -> AtlasResult<Json<Value>> {
    let lang = body.language.unwrap_or_else(|| guess_lang(&body.path));
    let (file, symbols) = state.store.index_code_file(
        body.project_id.as_deref(),
        &body.path,
        &lang,
        &body.content,
    )?;
    Ok(Json(json!({ "file": file, "symbols_indexed": symbols })))
}

pub async fn search_code(
    State(state): State<AppState>,
    Query(q): Query<CodeSearchQuery>,
) -> AtlasResult<Json<Value>> {
    let hits = state
        .store
        .search_code(&q.q, q.limit.unwrap_or(20))?;
    Ok(Json(json!({ "query": q.q, "hits": hits })))
}

pub async fn get_object(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> AtlasResult<Json<Value>> {
    if hash.len() < 16 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AtlasError::BadRequest("invalid object hash".into()));
    }
    let cfg = state.config.read().await.clone();
    let bytes = state
        .store
        .read_object(&cfg.data_dir, &hash)?
        .ok_or_else(|| AtlasError::NotFound(format!("object {hash} niet gevonden")))?;
    let preview = if bytes.len() <= 64 * 1024
        && std::str::from_utf8(&bytes).is_ok()
    {
        Some(String::from_utf8_lossy(&bytes).to_string())
    } else {
        None
    };
    Ok(Json(json!({
        "hash": hash,
        "size": bytes.len(),
        "content_base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
        "text": preview,
    })))
}

fn slugify(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        s
    }
}

fn guess_lang(path: &str) -> String {
    match path.rsplit('.').next().unwrap_or("") {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        _ => "text",
    }
    .into()
}
