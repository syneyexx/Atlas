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
    cfg.apply_patch(patch)?;
    Ok(Json(PublicSettings::from(&*cfg)))
}
