use crate::error::{AtlasError, AtlasResult};
use crate::memory::{
    recall_into_context, LearnMemoryRequest, WriteAction, WriteGovernor,
};
use crate::state::AppState;
use crate::store::{KnowledgeHit, KnowledgeNode, LearnRequest, RecallRequest};
use axum::extract::{Path, State};
use axum::Json;

/// Compat list — prefers memory semantic nodes, falls back to legacy table.
pub async fn list(State(state): State<AppState>) -> AtlasResult<Json<Vec<KnowledgeNode>>> {
    let nodes = state.store.list_memory_nodes(Some(crate::memory::MemoryKind::Semantic))?;
    if !nodes.is_empty() {
        let mapped = nodes
            .into_iter()
            .map(|n| KnowledgeNode {
                id: n.id,
                title: n.title,
                content: n.content,
                tags: n.tags,
                created_at: n.provenance.created_at,
            })
            .collect();
        return Ok(Json(mapped));
    }
    Ok(Json(state.store.list_knowledge()?))
}

/// Compat learn → Write Governor (semantic).
pub async fn learn(
    State(state): State<AppState>,
    Json(body): Json<LearnRequest>,
) -> AtlasResult<Json<KnowledgeNode>> {
    let cfg = state.config.read().await.clone();
    let decision = WriteGovernor::learn(
        &state.store,
        &cfg,
        &state.embed,
        LearnMemoryRequest {
            title: body.title,
            content: body.content,
            tags: body.tags,
            kind: Some("semantic".into()),
            source: Some("learn".into()),
            scope: Some("global".into()),
            confidence: Some(0.85),
            conversation_id: None,
        },
    )
    .await?;

    match decision.action {
        WriteAction::Discarded => Err(AtlasError::BadRequest(decision.reason)),
        WriteAction::Inserted | WriteAction::Merged | WriteAction::Deduped => {
            let node = decision.node.ok_or_else(|| {
                AtlasError::BadRequest("write governor gaf geen node terug".into())
            })?;
            Ok(Json(KnowledgeNode {
                id: node.id,
                title: node.title,
                content: node.content,
                tags: node.tags,
                created_at: node.provenance.created_at,
            }))
        }
    }
}

/// Compat recall → Memory Cortex fused hits.
pub async fn recall(
    State(state): State<AppState>,
    Json(body): Json<RecallRequest>,
) -> AtlasResult<Json<Vec<KnowledgeHit>>> {
    let cfg = state.config.read().await.clone();
    let recalled = recall_into_context(
        &state.store,
        &cfg,
        &state.embed,
        &body.query,
        &[],
        None,
        false,
    )
    .await?;
    let top = body.top_k.unwrap_or(cfg.knowledge_top_k);
    let hits = recalled
        .hits
        .into_iter()
        .take(top)
        .map(|h| KnowledgeHit {
            id: h.id,
            title: h.title,
            content: h.content,
            score: h.score,
        })
        .collect();
    Ok(Json(hits))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<serde_json::Value>> {
    let removed_mem = state.store.delete_memory_node(&id)?;
    let removed_legacy = state.store.delete_knowledge(&id)?;
    if !removed_mem && !removed_legacy {
        return Err(AtlasError::NotFound(format!("Kennis {id} niet gevonden")));
    }
    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}
