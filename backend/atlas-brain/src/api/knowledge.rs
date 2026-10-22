use crate::brain::recall_knowledge;
use crate::error::{AtlasError, AtlasResult};
use crate::state::AppState;
use crate::store::{KnowledgeHit, KnowledgeNode, LearnRequest, RecallRequest};
use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

pub async fn list(State(state): State<AppState>) -> AtlasResult<Json<Vec<KnowledgeNode>>> {
    Ok(Json(state.store.list_knowledge()?))
}

pub async fn learn(
    State(state): State<AppState>,
    Json(body): Json<LearnRequest>,
) -> AtlasResult<Json<KnowledgeNode>> {
    if body.content.trim().is_empty() {
        return Err(AtlasError::BadRequest("content mag niet leeg zijn.".into()));
    }
    let node = KnowledgeNode {
        id: Uuid::new_v4().to_string(),
        title: body.title,
        content: body.content,
        tags: body.tags,
        created_at: Utc::now().to_rfc3339(),
    };
    state.store.insert_knowledge(&node)?;
    Ok(Json(node))
}

pub async fn recall(
    State(state): State<AppState>,
    Json(body): Json<RecallRequest>,
) -> AtlasResult<Json<Vec<KnowledgeHit>>> {
    let cfg = state.config.read().await.clone();
    let hits = recall_knowledge(&state.store, &cfg, &body.query, body.top_k)?;
    Ok(Json(hits))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<serde_json::Value>> {
    let removed = state.store.delete_knowledge(&id)?;
    if !removed {
        return Err(AtlasError::NotFound(format!("Kennis {id} niet gevonden")));
    }
    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}
