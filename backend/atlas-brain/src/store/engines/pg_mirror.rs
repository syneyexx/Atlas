//! Durable mirror writes into Postgres when `database_url` engines are active.
use crate::error::AtlasResult;
use crate::memory::types::MemoryNode;
use crate::memory::working::WorkingMemoryState;
use crate::store::engines::PostgresHandle;
use crate::store::{
    ArtifactRecord, CostEntry, DocumentChunk, DocumentRecord, MemoryEdge, ProjectRecord,
};

pub fn mirror_memory_node(pg: &PostgresHandle, node: &MemoryNode) -> AtlasResult<()> {
    pg.execute(
        "INSERT INTO memory_nodes (
            id, kind, title, content, tags_json, source, created_at, parent_ids_json,
            confidence, scope, conversation_id, project_id, salience, decay, archived_at, updated_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,NULL,$12,$13,$14,$15)
         ON CONFLICT (id) DO UPDATE SET
           kind=EXCLUDED.kind, title=EXCLUDED.title, content=EXCLUDED.content,
           tags_json=EXCLUDED.tags_json, source=EXCLUDED.source,
           parent_ids_json=EXCLUDED.parent_ids_json, confidence=EXCLUDED.confidence,
           scope=EXCLUDED.scope, conversation_id=EXCLUDED.conversation_id,
           salience=EXCLUDED.salience, decay=EXCLUDED.decay,
           archived_at=EXCLUDED.archived_at, updated_at=EXCLUDED.updated_at",
        &[
            &node.id,
            &node.kind.as_str(),
            &node.title,
            &node.content,
            &serde_json::to_string(&node.tags).unwrap_or_else(|_| "[]".into()),
            &node.provenance.source.as_str(),
            &node.provenance.created_at,
            &serde_json::to_string(&node.provenance.parent_ids).unwrap_or_else(|_| "[]".into()),
            &node.provenance.confidence,
            &node.provenance.scope.as_str(),
            &node.provenance.conversation_id,
            &node.salience,
            &node.decay,
            &node.archived_at,
            &node.updated_at,
        ],
    )?;
    Ok(())
}

pub fn mirror_cost(pg: &PostgresHandle, e: &CostEntry) -> AtlasResult<()> {
    pg.execute(
        "INSERT INTO cost_ledger (
            id, trace_id, conversation_id, model_id, token_in, token_out, latency_ms, cost_usd, created_at
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         ON CONFLICT (id) DO NOTHING",
        &[
            &e.id,
            &e.trace_id,
            &e.conversation_id,
            &e.model_id,
            &e.token_in,
            &e.token_out,
            &e.latency_ms,
            &e.cost_usd,
            &e.created_at,
        ],
    )?;
    Ok(())
}

pub fn mirror_trace(
    pg: &PostgresHandle,
    id: &str,
    kind: &str,
    conversation_id: Option<&str>,
    model_id: Option<&str>,
    payload_json: &str,
    created_at: &str,
) -> AtlasResult<()> {
    let conv = conversation_id.map(|s| s.to_string());
    let model = model_id.map(|s| s.to_string());
    pg.execute(
        "INSERT INTO traces (id, kind, conversation_id, model_id, payload_json, created_at)
         VALUES ($1,$2,$3,$4,$5,$6)
         ON CONFLICT (id) DO NOTHING",
        &[&id, &kind, &conv, &model, &payload_json, &created_at],
    )?;
    Ok(())
}

pub fn mirror_edge(pg: &PostgresHandle, edge: &MemoryEdge) -> AtlasResult<()> {
    let prov = serde_json::to_string(&edge.provenance).unwrap_or_else(|_| "{}".into());
    pg.execute(
        "INSERT INTO memory_edges (id, src_id, rel, dst_id, weight, provenance_json, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         ON CONFLICT (src_id, rel, dst_id) DO UPDATE SET
           weight=EXCLUDED.weight,
           provenance_json=EXCLUDED.provenance_json",
        &[
            &edge.id,
            &edge.src_id,
            &edge.rel,
            &edge.dst_id,
            &edge.weight,
            &prov,
            &edge.created_at,
        ],
    )?;
    Ok(())
}

pub fn mirror_working(
    pg: &PostgresHandle,
    state: &WorkingMemoryState,
    query_embedding: Option<&[u8]>,
) -> AtlasResult<()> {
    let json = serde_json::to_string(state)?;
    let emb = query_embedding.map(|b| b.to_vec());
    pg.execute(
        "INSERT INTO working_memory (conversation_id, state_json, query_embedding, updated_at, expires_at)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (conversation_id) DO UPDATE SET
           state_json=EXCLUDED.state_json,
           query_embedding=COALESCE(EXCLUDED.query_embedding, working_memory.query_embedding),
           updated_at=EXCLUDED.updated_at,
           expires_at=EXCLUDED.expires_at",
        &[
            &state.conversation_id,
            &json,
            &emb,
            &state.updated_at,
            &state.expires_at,
        ],
    )?;
    Ok(())
}

pub fn mirror_document(pg: &PostgresHandle, doc: &DocumentRecord) -> AtlasResult<()> {
    let meta = serde_json::to_string(&doc.metadata).unwrap_or_else(|_| "{}".into());
    pg.execute(
        "INSERT INTO documents (id, title, source, media_type, object_hash, metadata_json, project_id, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,NULL,$7,$8)
         ON CONFLICT (id) DO UPDATE SET
           title=EXCLUDED.title, source=EXCLUDED.source, media_type=EXCLUDED.media_type,
           object_hash=EXCLUDED.object_hash, metadata_json=EXCLUDED.metadata_json,
           updated_at=EXCLUDED.updated_at",
        &[
            &doc.id,
            &doc.title,
            &doc.source,
            &doc.media_type,
            &doc.object_hash,
            &meta,
            &doc.created_at,
            &doc.updated_at,
        ],
    )?;
    Ok(())
}

pub fn mirror_document_chunk(pg: &PostgresHandle, chunk: &DocumentChunk) -> AtlasResult<()> {
    pg.execute(
        "INSERT INTO document_chunks (id, document_id, seq, content, memory_node_id, created_at)
         VALUES ($1,$2,$3,$4,$5,$6)
         ON CONFLICT (id) DO NOTHING",
        &[
            &chunk.id,
            &chunk.document_id,
            &chunk.seq,
            &chunk.content,
            &chunk.memory_node_id,
            &chunk.created_at,
        ],
    )?;
    Ok(())
}

pub fn mirror_outbox(
    pg: &PostgresHandle,
    id: &str,
    event_type: &str,
    payload_json: &str,
    created_at: &str,
) -> AtlasResult<()> {
    pg.execute(
        "INSERT INTO outbox (id, event_type, payload_json, created_at, processed_at)
         VALUES ($1,$2,$3,$4,NULL)
         ON CONFLICT (id) DO NOTHING",
        &[&id, &event_type, &payload_json, &created_at],
    )?;
    Ok(())
}

pub fn mirror_outbox_processed(pg: &PostgresHandle, id: &str, processed_at: &str) -> AtlasResult<()> {
    pg.execute(
        "UPDATE outbox SET processed_at = $1 WHERE id = $2 AND processed_at IS NULL",
        &[&processed_at, &id],
    )?;
    Ok(())
}

pub fn mirror_project(pg: &PostgresHandle, p: &ProjectRecord) -> AtlasResult<()> {
    let meta = serde_json::to_string(&p.meta).unwrap_or_else(|_| "{}".into());
    pg.execute(
        "INSERT INTO projects (id, name, slug, description, meta_json, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         ON CONFLICT (id) DO UPDATE SET
           name=EXCLUDED.name, slug=EXCLUDED.slug, description=EXCLUDED.description,
           meta_json=EXCLUDED.meta_json, updated_at=EXCLUDED.updated_at",
        &[
            &p.id,
            &p.name,
            &p.slug,
            &p.description,
            &meta,
            &p.created_at,
            &p.updated_at,
        ],
    )?;
    Ok(())
}

pub fn mirror_artifact(pg: &PostgresHandle, a: &ArtifactRecord) -> AtlasResult<()> {
    let meta = serde_json::to_string(&a.meta).unwrap_or_else(|_| "{}".into());
    pg.execute(
        "INSERT INTO artifacts (id, project_id, conversation_id, task_id, kind, title, object_hash, content, meta_json, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (id) DO NOTHING",
        &[
            &a.id,
            &a.project_id,
            &a.conversation_id,
            &a.task_id,
            &a.kind,
            &a.title,
            &a.object_hash,
            &a.content,
            &meta,
            &a.created_at,
        ],
    )?;
    Ok(())
}

pub fn mirror_secret(
    pg: &PostgresHandle,
    id: &str,
    name: &str,
    ciphertext: &str,
    nonce: &str,
    now: &str,
) -> AtlasResult<()> {
    pg.execute(
        "INSERT INTO secrets (id, name, ciphertext, nonce, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$5)
         ON CONFLICT (name) DO UPDATE SET
           ciphertext=EXCLUDED.ciphertext, nonce=EXCLUDED.nonce, updated_at=EXCLUDED.updated_at",
        &[&id, &name, &ciphertext, &nonce, &now],
    )?;
    Ok(())
}
