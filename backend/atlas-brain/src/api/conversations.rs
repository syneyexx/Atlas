use crate::error::{AtlasError, AtlasResult};
use crate::state::AppState;
use crate::store::ConversationRecord;
use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;

pub async fn list(State(state): State<AppState>) -> AtlasResult<Json<Vec<ConversationRecord>>> {
    Ok(Json(state.store.list_conversations()?))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<ConversationRecord>> {
    state
        .store
        .get_conversation(&id)?
        .map(Json)
        .ok_or_else(|| AtlasError::NotFound(format!("Gesprek {id} niet gevonden")))
}

pub async fn upsert(
    State(state): State<AppState>,
    Json(mut record): Json<ConversationRecord>,
) -> AtlasResult<Json<ConversationRecord>> {
    if record.id.trim().is_empty() {
        return Err(AtlasError::BadRequest("conversation id is verplicht.".into()));
    }
    if record.updated_at.trim().is_empty() {
        record.updated_at = Utc::now().to_rfc3339();
    }
    state.store.upsert_conversation(&record)?;
    Ok(Json(record))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<serde_json::Value>> {
    let removed = state.store.delete_conversation(&id)?;
    if !removed {
        return Err(AtlasError::NotFound(format!("Gesprek {id} niet gevonden")));
    }
    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}
