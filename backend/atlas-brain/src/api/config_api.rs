use crate::config::{PublicSettings, PublicSettingsPatch};
use crate::error::AtlasResult;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;

pub async fn get_settings(State(state): State<AppState>) -> AtlasResult<Json<PublicSettings>> {
    let cfg = state.config.read().await;
    Ok(Json(PublicSettings::from(&*cfg)))
}

pub async fn put_settings(
    State(state): State<AppState>,
    Json(patch): Json<PublicSettingsPatch>,
) -> AtlasResult<Json<PublicSettings>> {
    let mut cfg = state.config.write().await;
    let prev_model = cfg.model_id.clone();
    let prev_prompt = cfg.system_prompt.clone();
    cfg.apply_patch(patch)?;
    if cfg.model_id != prev_model || cfg.system_prompt != prev_prompt {
        let _ = state.store.cache_invalidate_all();
    }
    // Request-driven prompt registry snapshot when system_prompt / model changes.
    let _ = state.store.snapshot_system_prompt_if_changed(
        &cfg.system_prompt,
        Some(cfg.model_id.as_str()),
    );
    // Keep model registry in sync with active model_id.
    if !cfg.model_id.trim().is_empty() {
        let _ = state.store.upsert_model_registry(&crate::store::ModelRegistryEntry {
            id: cfg.model_id.clone(),
            provider: cfg.lm_provider.clone(),
            label: cfg.model_id.clone(),
            meta: serde_json::json!({
                "lm_base_url": cfg.lm_base_url,
                "embed_provider": cfg.embed_provider,
                "embed_model": cfg.embed_model,
            }),
            created_at: chrono::Utc::now().to_rfc3339(),
        });
    }
    Ok(Json(PublicSettings::from(&*cfg)))
}
