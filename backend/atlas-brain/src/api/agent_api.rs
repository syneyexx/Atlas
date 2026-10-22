use crate::error::{AtlasError, AtlasResult};
use crate::state::AppState;
use crate::store::{
    ModelRegistryEntry, PromptVersion, TaskCheckpoint, TaskRecord, ToolRecord,
};
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ToolUpsertBody {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    pub description: String,
    pub schema: Value,
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ToolPatchBody {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ToolsQuery {
    #[serde(default)]
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct TaskCreateBody {
    pub title: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub plan: Option<Value>,
    #[serde(default)]
    pub state: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct TasksQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct CheckpointBody {
    pub label: String,
    #[serde(default)]
    pub snapshot: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct TaskStatusBody {
    pub status: String,
    #[serde(default)]
    pub state: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct PromptCreateBody {
    #[serde(default)]
    pub name: Option<String>,
    pub content: String,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub activate: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PromptsQuery {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ModelRegBody {
    pub id: String,
    pub provider: String,
    pub label: String,
    #[serde(default)]
    pub meta: Option<Value>,
}

pub async fn list_tools(
    State(state): State<AppState>,
    Query(q): Query<ToolsQuery>,
) -> AtlasResult<Json<Vec<ToolRecord>>> {
    let enabled_only = !q.all.unwrap_or(false);
    Ok(Json(state.store.list_tools(enabled_only)?))
}

pub async fn upsert_tool(
    State(state): State<AppState>,
    Json(body): Json<ToolUpsertBody>,
) -> AtlasResult<Json<ToolRecord>> {
    if body.name.trim().is_empty() {
        return Err(AtlasError::BadRequest("tool name is verplicht".into()));
    }
    let policy = body.policy.unwrap_or_else(|| "ask".into());
    if !matches!(policy.as_str(), "allow" | "deny" | "ask") {
        return Err(AtlasError::BadRequest(
            "policy must be allow, deny, or ask".into(),
        ));
    }
    let now = Utc::now().to_rfc3339();
    let tool = ToolRecord {
        name: body.name.trim().to_string(),
        version: body
            .version
            .unwrap_or_else(|| "1".into())
            .trim()
            .to_string(),
        description: body.description,
        schema: body.schema,
        policy,
        capabilities: body.capabilities,
        enabled: body.enabled.unwrap_or(true),
        created_at: now.clone(),
        updated_at: now,
    };
    state.store.upsert_tool(&tool)?;
    Ok(Json(tool))
}

pub async fn get_tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> AtlasResult<Json<ToolRecord>> {
    state
        .store
        .get_tool(&name, None)?
        .map(Json)
        .ok_or_else(|| AtlasError::NotFound(format!("Tool {name} niet gevonden")))
}

pub async fn patch_tool(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<ToolPatchBody>,
) -> AtlasResult<Json<ToolRecord>> {
    let version = body.version.unwrap_or_else(|| "1".into());
    if let Some(policy) = body.policy {
        if !state.store.set_tool_policy(&name, &version, &policy)? {
            return Err(AtlasError::NotFound(format!(
                "Tool {name}@{version} niet gevonden"
            )));
        }
    }
    if let Some(enabled) = body.enabled {
        if !state.store.set_tool_enabled(&name, &version, enabled)? {
            return Err(AtlasError::NotFound(format!(
                "Tool {name}@{version} niet gevonden"
            )));
        }
    }
    state
        .store
        .get_tool(&name, Some(&version))?
        .map(Json)
        .ok_or_else(|| AtlasError::NotFound(format!("Tool {name}@{version} niet gevonden")))
}

pub async fn list_tasks(
    State(state): State<AppState>,
    Query(q): Query<TasksQuery>,
) -> AtlasResult<Json<Vec<TaskRecord>>> {
    Ok(Json(state.store.list_tasks(
        q.status.as_deref(),
        q.limit.unwrap_or(50),
    )?))
}

pub async fn create_task(
    State(state): State<AppState>,
    Json(body): Json<TaskCreateBody>,
) -> AtlasResult<Json<TaskRecord>> {
    if body.title.trim().is_empty() {
        return Err(AtlasError::BadRequest("task title is verplicht".into()));
    }
    let now = Utc::now().to_rfc3339();
    let task = TaskRecord {
        id: Uuid::new_v4().to_string(),
        title: body.title.trim().to_string(),
        status: "queued".into(),
        conversation_id: body.conversation_id,
        parent_id: body.parent_id,
        plan: body.plan.unwrap_or_else(|| json!([])),
        state: body.state.unwrap_or_else(|| json!({})),
        created_at: now.clone(),
        updated_at: now,
    };
    state.store.create_task(&task)?;
    let _ = state.store.enqueue_outbox(
        "task.created",
        &json!({ "task_id": task.id, "title": task.title }),
    );
    Ok(Json(task))
}

pub async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    let task = state
        .store
        .get_task(&id)?
        .ok_or_else(|| AtlasError::NotFound(format!("Task {id} niet gevonden")))?;
    let checkpoints = state.store.list_checkpoints(&id)?;
    Ok(Json(json!({ "task": task, "checkpoints": checkpoints })))
}

pub async fn checkpoint_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CheckpointBody>,
) -> AtlasResult<Json<TaskCheckpoint>> {
    if state.store.get_task(&id)?.is_none() {
        return Err(AtlasError::NotFound(format!("Task {id} niet gevonden")));
    }
    let snap = body.snapshot.unwrap_or_else(|| json!({}));
    let cp = state.store.add_checkpoint(&id, &body.label, &snap)?;
    Ok(Json(cp))
}

pub async fn set_task_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<TaskStatusBody>,
) -> AtlasResult<Json<TaskRecord>> {
    let status = body.status.trim();
    if !matches!(
        status,
        "queued" | "running" | "blocked" | "completed" | "failed" | "cancelled"
    ) {
        return Err(AtlasError::BadRequest(
            "status must be queued|running|blocked|completed|failed|cancelled".into(),
        ));
    }
    if !state
        .store
        .update_task_status(&id, status, body.state.as_ref())?
    {
        return Err(AtlasError::NotFound(format!("Task {id} niet gevonden")));
    }
    state
        .store
        .get_task(&id)?
        .map(Json)
        .ok_or_else(|| AtlasError::NotFound(format!("Task {id} niet gevonden")))
}

pub async fn list_prompts(
    State(state): State<AppState>,
    Query(q): Query<PromptsQuery>,
) -> AtlasResult<Json<Vec<PromptVersion>>> {
    Ok(Json(
        state.store.list_prompt_versions(q.name.as_deref())?,
    ))
}

pub async fn create_prompt(
    State(state): State<AppState>,
    Json(body): Json<PromptCreateBody>,
) -> AtlasResult<Json<PromptVersion>> {
    let name = body
        .name
        .unwrap_or_else(|| "system_prompt".into())
        .trim()
        .to_string();
    if name.is_empty() {
        return Err(AtlasError::BadRequest("prompt name is verplicht".into()));
    }
    let version = state.store.next_prompt_version(&name)?;
    let prompt = PromptVersion {
        id: Uuid::new_v4().to_string(),
        name,
        version,
        content: body.content,
        model_id: body.model_id.filter(|s| !s.is_empty()),
        active: body.activate.unwrap_or(true),
        created_at: Utc::now().to_rfc3339(),
    };
    state.store.insert_prompt_version(&prompt)?;
    Ok(Json(prompt))
}

pub async fn get_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<PromptVersion>> {
    state
        .store
        .get_prompt_version(&id)?
        .map(Json)
        .ok_or_else(|| AtlasError::NotFound(format!("Prompt {id} niet gevonden")))
}

pub async fn activate_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<PromptVersion>> {
    state
        .store
        .activate_prompt_version(&id)?
        .map(Json)
        .ok_or_else(|| AtlasError::NotFound(format!("Prompt {id} niet gevonden")))
}

pub async fn list_models_registry(
    State(state): State<AppState>,
) -> AtlasResult<Json<Vec<ModelRegistryEntry>>> {
    Ok(Json(state.store.list_model_registry()?))
}

pub async fn upsert_model_registry(
    State(state): State<AppState>,
    Json(body): Json<ModelRegBody>,
) -> AtlasResult<Json<ModelRegistryEntry>> {
    if body.id.trim().is_empty() {
        return Err(AtlasError::BadRequest("model id is verplicht".into()));
    }
    let entry = ModelRegistryEntry {
        id: body.id.trim().to_string(),
        provider: body.provider,
        label: body.label,
        meta: body.meta.unwrap_or_else(|| json!({})),
        created_at: Utc::now().to_rfc3339(),
    };
    state.store.upsert_model_registry(&entry)?;
    Ok(Json(entry))
}
