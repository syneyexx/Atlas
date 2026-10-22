use crate::error::{AtlasError, AtlasResult};
use crate::memory::{
    consolidate_episodic, recall_into_context, ConsolidateRequest, LearnMemoryRequest, MemoryKind,
    MemoryNode, MemoryScope, MemorySource, ProfileUpsertRequest, Provenance, RecallMemoryRequest,
    RecallMemoryResponse, WorkingMemoryState, WriteAction, WriteDecision, WriteGovernor,
};
use crate::memory::working::load_working;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use uuid::Uuid;

pub async fn list(State(state): State<AppState>) -> AtlasResult<Json<Vec<MemoryNode>>> {
    Ok(Json(state.store.list_memory_nodes(None)?))
}

pub async fn learn(
    State(state): State<AppState>,
    Json(body): Json<LearnMemoryRequest>,
) -> AtlasResult<Json<WriteDecision>> {
    let cfg = state.config.read().await.clone();
    let decision = WriteGovernor::learn(&state.store, &cfg, &state.embed, body).await?;
    if decision.action == WriteAction::Discarded {
        return Err(AtlasError::BadRequest(decision.reason));
    }
    Ok(Json(decision))
}

pub async fn recall(
    State(state): State<AppState>,
    Json(body): Json<RecallMemoryRequest>,
) -> AtlasResult<Json<RecallMemoryResponse>> {
    let cfg = state.config.read().await.clone();
    let want_trace = body.trace.unwrap_or(false);
    let recalled = recall_into_context(
        &state.store,
        &cfg,
        &state.embed,
        &body.query,
        &[],
        body.conversation_id.as_deref(),
        want_trace,
    )
    .await?;
    let top = body.top_k.unwrap_or(cfg.knowledge_top_k);
    Ok(Json(RecallMemoryResponse {
        hits: recalled.hits.into_iter().take(top).collect(),
        trace: if want_trace {
            Some(recalled.emit.trace)
        } else {
            None
        },
    }))
}

pub async fn forget(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<serde_json::Value>> {
    let archived = state.store.archive_memory_node(&id)?;
    if !archived {
        // Hard delete if already archived or missing archive path
        let deleted = state.store.delete_memory_node(&id)?;
        if !deleted {
            return Err(AtlasError::NotFound(format!("Memory {id} niet gevonden")));
        }
        return Ok(Json(serde_json::json!({ "forgotten": true, "mode": "deleted", "id": id })));
    }
    Ok(Json(serde_json::json!({ "forgotten": true, "mode": "archived", "id": id })))
}

pub async fn migrate(State(state): State<AppState>) -> AtlasResult<Json<serde_json::Value>> {
    let cfg = state.config.read().await.clone();
    let n = state.store.migrate_knowledge_to_memory()?;
    // Embed migrated nodes on-demand in this request (no background job).
    let mut embedded = 0usize;
    for node in state.store.list_memory_nodes(Some(MemoryKind::Semantic))? {
        if state.store.get_embedding(&node.id)?.is_some() {
            continue;
        }
        let vector = state
            .embed
            .embed(&cfg, &format!("{}\n{}", node.title, node.content))
            .await?;
        state.store.upsert_embedding(&node.id, &vector)?;
        embedded += 1;
    }
    Ok(Json(serde_json::json!({
        "migrated_rows": n,
        "embedded": embedded
    })))
}

pub async fn list_profile(State(state): State<AppState>) -> AtlasResult<Json<Vec<MemoryNode>>> {
    Ok(Json(
        state
            .store
            .list_memory_nodes(Some(MemoryKind::SelfProfile))?,
    ))
}

pub async fn upsert_profile(
    State(state): State<AppState>,
    Json(body): Json<ProfileUpsertRequest>,
) -> AtlasResult<Json<MemoryNode>> {
    if body.content.trim().is_empty() || body.key.trim().is_empty() {
        return Err(AtlasError::BadRequest(
            "profile key en content zijn verplicht".into(),
        ));
    }
    let cfg = state.config.read().await.clone();
    let title = body
        .title
        .unwrap_or_else(|| format!("profile:{}", body.key.trim()));
    // Upsert by matching title key.
    let existing = state
        .store
        .list_memory_nodes(Some(MemoryKind::SelfProfile))?;
    let now = Utc::now().to_rfc3339();
    let mut node = if let Some(found) = existing.into_iter().find(|n| n.title == title) {
        let mut n = found;
        n.content = body.content.trim().to_string();
        n.provenance.confidence = body.confidence.unwrap_or(n.provenance.confidence);
        n.updated_at = now.clone();
        n
    } else {
        MemoryNode {
            id: Uuid::new_v4().to_string(),
            kind: MemoryKind::SelfProfile,
            title,
            content: body.content.trim().to_string(),
            tags: vec!["profile".into(), body.key.trim().to_string()],
            provenance: Provenance {
                source: MemorySource::Profile,
                created_at: now.clone(),
                parent_ids: Vec::new(),
                confidence: body.confidence.unwrap_or(0.95),
                scope: MemoryScope::Global,
                conversation_id: None,
            },
            salience: 1.5,
            decay: 1.0,
            archived_at: None,
            updated_at: now,
        }
    };
    let vector = state
        .embed
        .embed(&cfg, &format!("{}\n{}", node.title, node.content))
        .await?;
    state.store.upsert_memory_node(&node)?;
    state.store.upsert_embedding(&node.id, &vector)?;
    let _ = &mut node;
    Ok(Json(node))
}

pub async fn consolidate(
    State(state): State<AppState>,
    Json(body): Json<ConsolidateRequest>,
) -> AtlasResult<Json<crate::memory::ConsolidateResult>> {
    let cfg = state.config.read().await.clone();
    if cfg.model_id.trim().is_empty() && cfg.lm_provider != "stub" {
        return Err(AtlasError::BadRequest(
            "model_id verplicht voor consolidatie met local_http.".into(),
        ));
    }
    let result =
        consolidate_episodic(&state.store, &cfg, &state.lm, &state.embed, body).await?;
    Ok(Json(result))
}

pub async fn get_working(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<WorkingMemoryState>> {
    let cfg = state.config.read().await.clone();
    load_working(&state.store, &cfg, &id)?
        .map(Json)
        .ok_or_else(|| AtlasError::NotFound(format!("Working memory voor {id} niet gevonden")))
}
