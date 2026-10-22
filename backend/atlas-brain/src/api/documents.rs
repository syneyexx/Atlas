use crate::error::{AtlasError, AtlasResult};
use crate::memory::ingest::{ingest_document, IngestDocumentRequest, IngestDocumentResult};
use crate::state::AppState;
use crate::store::DocumentRecord;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

pub async fn list(State(state): State<AppState>) -> AtlasResult<Json<Vec<DocumentRecord>>> {
    Ok(Json(state.store.list_documents()?))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    let mut doc = state
        .store
        .get_document(&id)?
        .ok_or_else(|| AtlasError::NotFound(format!("Document {id} niet gevonden")))?;
    let chunks = state.store.list_document_chunks(&id)?;
    doc.chunk_ids = chunks.iter().map(|c| c.id.clone()).collect();
    Ok(Json(json!({ "document": doc, "chunks": chunks })))
}

pub async fn ingest(
    State(state): State<AppState>,
    Json(body): Json<IngestDocumentRequest>,
) -> AtlasResult<Json<IngestDocumentResult>> {
    let cfg = state.config.read().await.clone();
    let result = ingest_document(&state.store, &cfg, &state.embed, body).await?;
    // Request-driven outbox drain after ingest.
    let _ = state.store.drain_outbox(64);
    Ok(Json(result))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    // Soft-delete linked memory nodes (archive) then remove document rows.
    let chunks = state.store.list_document_chunks(&id)?;
    for chunk in &chunks {
        if let Some(ref nid) = chunk.memory_node_id {
            let _ = state.store.archive_memory_node(nid);
        }
    }
    let removed = state.store.delete_document(&id)?;
    if !removed {
        return Err(AtlasError::NotFound(format!("Document {id} niet gevonden")));
    }
    Ok(Json(json!({ "deleted": true, "id": id })))
}

pub async fn search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> AtlasResult<Json<Value>> {
    let limit = q.limit.unwrap_or(10);
    let hits = state.store.search_documents_fts(&q.q, limit)?;
    let mut enriched = Vec::new();
    for (doc_id, chunk_id, rank) in hits {
        let title = state
            .store
            .get_document(&doc_id)?
            .map(|d| d.title)
            .unwrap_or_default();
        enriched.push(json!({
            "document_id": doc_id,
            "chunk_id": chunk_id,
            "title": title,
            "rank": rank,
        }));
    }
    Ok(Json(json!({ "query": q.q, "hits": enriched })))
}
