use crate::error::AtlasResult;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

/// OpenAI-compatible model list. Probes the local LM only when this route is hit.
pub async fn list_models(State(state): State<AppState>) -> AtlasResult<Json<Value>> {
    let cfg = state.config.read().await.clone();
    let mut ids = state.lm.list_models(&cfg).await?;
    if !cfg.model_id.is_empty() && !ids.iter().any(|id| id == &cfg.model_id) {
        // Still advertise the configured id so the UI can connect when the LM
        // uses a different listing shape; completion will validate upstream.
        ids.insert(0, cfg.model_id.clone());
    }
    if ids.is_empty() && !cfg.model_id.is_empty() {
        ids.push(cfg.model_id.clone());
    }
    let data: Vec<Value> = ids
        .into_iter()
        .map(|id| json!({ "id": id, "object": "model", "owned_by": "local" }))
        .collect();
    Ok(Json(json!({ "object": "list", "data": data })))
}
