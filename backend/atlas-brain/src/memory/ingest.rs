use crate::config::BrainConfig;
use crate::error::{AtlasError, AtlasResult};
use crate::memory::embed::EmbedClient;
use crate::memory::types::{LearnMemoryRequest, WriteAction, WriteDecision};
use crate::memory::WriteGovernor;
use crate::store::{DocumentChunk, DocumentRecord, Store};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct IngestDocumentRequest {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestDocumentResult {
    pub document: DocumentRecord,
    pub chunks: usize,
    pub memory_writes: Vec<WriteDecision>,
    pub object_hash: String,
    pub object_new: bool,
}

/// Ingest raw text/document → object store + chunks + semantic memory nodes (hybrid RAG).
pub async fn ingest_document(
    store: &Store,
    cfg: &BrainConfig,
    embed: &EmbedClient,
    req: IngestDocumentRequest,
) -> AtlasResult<IngestDocumentResult> {
    let content = req.content.trim().to_string();
    if content.is_empty() {
        return Err(AtlasError::BadRequest("document content mag niet leeg zijn.".into()));
    }
    let title = if req.title.trim().is_empty() {
        content.chars().take(48).collect::<String>()
    } else {
        req.title.trim().to_string()
    };
    let source = req
        .source
        .unwrap_or_else(|| "paste".into())
        .trim()
        .to_string();
    let media_type = req
        .media_type
        .unwrap_or_else(|| "text/plain".into())
        .trim()
        .to_string();

    let (object_hash, _path, object_new) =
        store.put_object(&cfg.data_dir, content.as_bytes())?;

    let now = Store::now_rfc3339();
    let doc_id = Store::new_document_id();
    let mut metadata = req.metadata.unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = metadata.as_object_mut() {
        obj.insert("tags".into(), serde_json::json!(req.tags));
        obj.insert("bytes".into(), serde_json::json!(content.len()));
    }

    let doc = DocumentRecord {
        id: doc_id.clone(),
        title: title.clone(),
        source,
        media_type,
        object_hash: object_hash.clone(),
        metadata,
        created_at: now.clone(),
        updated_at: now.clone(),
        chunk_ids: Vec::new(),
    };
    store.upsert_document(&doc)?;

    let chunks = chunk_document(&content, cfg.memory_chunk_chars);
    let mut memory_writes = Vec::new();
    let mut chunk_ids = Vec::new();

    for (seq, chunk_text) in chunks.into_iter().enumerate() {
        let chunk_id = Uuid::new_v4().to_string();
        let mut tags = req.tags.clone();
        tags.push("document".into());
        tags.push(format!("doc:{doc_id}"));

        let decision = WriteGovernor::learn(
            store,
            cfg,
            embed,
            LearnMemoryRequest {
                title: if seq == 0 {
                    title.clone()
                } else {
                    format!("{title} [{}]", seq + 1)
                },
                content: chunk_text.clone(),
                tags,
                kind: Some("semantic".into()),
                source: Some("attachment".into()),
                scope: Some("project".into()),
                confidence: Some(0.85),
                conversation_id: None,
                parent_ids: vec![doc_id.clone()],
            },
        )
        .await?;

        let memory_node_id = decision.node.as_ref().map(|n| n.id.clone());
        if let Some(ref nid) = memory_node_id {
            let _ = store.upsert_edge(
                &doc_id,
                "has_chunk",
                nid,
                1.0,
                &serde_json::json!({ "seq": seq, "chunk_id": chunk_id }),
            );
        }

        let chunk = DocumentChunk {
            id: chunk_id.clone(),
            document_id: doc_id.clone(),
            seq: seq as i64,
            content: chunk_text,
            memory_node_id,
            created_at: now.clone(),
        };
        store.insert_document_chunk(&chunk, &title)?;
        chunk_ids.push(chunk_id);
        if !matches!(decision.action, WriteAction::Discarded) {
            memory_writes.push(decision);
        }
    }

    let mut doc_out = doc;
    doc_out.chunk_ids = chunk_ids;

    Ok(IngestDocumentResult {
        document: doc_out,
        chunks: memory_writes.len(),
        memory_writes,
        object_hash,
        object_new,
    })
}

fn chunk_document(content: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(256);
    if content.chars().count() <= max_chars {
        return vec![content.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for para in content.split("\n\n") {
        let candidate = if current.is_empty() {
            para.to_string()
        } else {
            format!("{current}\n\n{para}")
        };
        if candidate.chars().count() > max_chars && !current.is_empty() {
            chunks.push(current);
            current = para.to_string();
        } else if candidate.chars().count() > max_chars {
            let mut buf = String::new();
            for ch in para.chars() {
                buf.push(ch);
                if buf.chars().count() >= max_chars {
                    chunks.push(buf);
                    buf = String::new();
                }
            }
            current = buf;
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() {
        chunks.push(content.to_string());
    }
    chunks
}
