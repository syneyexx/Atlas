use crate::error::{AtlasError, AtlasResult};
use crate::state::AppState;
use crate::store::{DrainResult, MemoryEdge, OutboxEvent, TraceRecord};
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct TraceListQuery {
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct DrainQuery {
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct EdgeBody {
    pub src_id: String,
    pub rel: String,
    pub dst_id: String,
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(default)]
    pub provenance: Option<Value>,
}

pub async fn list_traces(
    State(state): State<AppState>,
    Query(q): Query<TraceListQuery>,
) -> AtlasResult<Json<Vec<TraceRecord>>> {
    let limit = q.limit.unwrap_or(50);
    Ok(Json(state.store.list_traces(
        q.conversation_id.as_deref(),
        limit,
    )?))
}

pub async fn get_trace(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<TraceRecord>> {
    state
        .store
        .get_trace(&id)?
        .map(Json)
        .ok_or_else(|| AtlasError::NotFound(format!("Trace {id} niet gevonden")))
}

pub async fn list_outbox(
    State(state): State<AppState>,
    Query(q): Query<DrainQuery>,
) -> AtlasResult<Json<Vec<OutboxEvent>>> {
    Ok(Json(
        state.store.list_pending_outbox(q.limit.unwrap_or(50))?,
    ))
}

pub async fn drain_outbox(
    State(state): State<AppState>,
    Query(q): Query<DrainQuery>,
) -> AtlasResult<Json<DrainResult>> {
    Ok(Json(state.store.drain_outbox(q.limit.unwrap_or(100))?))
}

pub async fn upsert_edge(
    State(state): State<AppState>,
    Json(body): Json<EdgeBody>,
) -> AtlasResult<Json<MemoryEdge>> {
    if body.src_id.trim().is_empty()
        || body.dst_id.trim().is_empty()
        || body.rel.trim().is_empty()
    {
        return Err(AtlasError::BadRequest(
            "src_id, rel en dst_id zijn verplicht.".into(),
        ));
    }
    let edge = state.store.upsert_edge(
        body.src_id.trim(),
        body.rel.trim(),
        body.dst_id.trim(),
        body.weight.unwrap_or(1.0),
        &body.provenance.unwrap_or_else(|| json!({"source": "api"})),
    )?;
    Ok(Json(edge))
}

pub async fn list_edges(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    let from = state.store.list_edges_from(&id)?;
    let to = state.store.list_edges_to(&id)?;
    Ok(Json(json!({ "from": from, "to": to })))
}
