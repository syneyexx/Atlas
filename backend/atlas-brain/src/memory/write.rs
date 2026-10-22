use crate::config::BrainConfig;
use crate::error::{AtlasError, AtlasResult};
use crate::memory::embed::{cosine, EmbedClient};
use crate::memory::types::{
    LearnMemoryRequest, MemoryKind, MemoryNode, MemoryScope, MemorySource, Provenance, WriteAction,
    WriteDecision,
};
use crate::store::Store;
use chrono::Utc;
use uuid::Uuid;

pub struct WriteGovernor;

impl WriteGovernor {
    pub async fn learn(
        store: &Store,
        cfg: &BrainConfig,
        embed: &EmbedClient,
        req: LearnMemoryRequest,
    ) -> AtlasResult<WriteDecision> {
        let content = req.content.trim().to_string();
        if content.is_empty() {
            return Err(AtlasError::BadRequest("content mag niet leeg zijn.".into()));
        }
        if content.chars().count() < 8 {
            return Ok(WriteDecision {
                action: WriteAction::Discarded,
                node: None,
                reason: "content te kort voor memory commit".into(),
            });
        }

        let kind = req
            .kind
            .as_deref()
            .and_then(MemoryKind::parse)
            .unwrap_or(MemoryKind::Semantic);

        let source = req
            .source
            .as_deref()
            .map(MemorySource::parse)
            .unwrap_or(MemorySource::Learn);
        let scope = req
            .scope
            .as_deref()
            .map(MemoryScope::parse)
            .unwrap_or(MemoryScope::Global);
        let confidence = req.confidence.unwrap_or(0.8).clamp(0.05, 1.0);
        let title = if req.title.trim().is_empty() {
            content.chars().take(48).collect::<String>()
        } else {
            req.title.trim().to_string()
        };

        let chunks = chunk_text(&title, &content, cfg.memory_chunk_chars);
        let mut last: Option<WriteDecision> = None;
        for (idx, chunk) in chunks.into_iter().enumerate() {
            let chunk_title = if idx == 0 {
                title.clone()
            } else {
                format!("{title} ({})", idx + 1)
            };
            let decision = commit_one(
                store,
                cfg,
                embed,
                kind,
                source,
                scope,
                confidence,
                chunk_title,
                chunk,
                req.tags.clone(),
                req.conversation_id.clone(),
                req.parent_ids.clone(),
            )
            .await?;
            last = Some(decision);
        }
        last.ok_or_else(|| AtlasError::BadRequest("geen chunks om te schrijven".into()))
    }
}

async fn commit_one(
    store: &Store,
    cfg: &BrainConfig,
    embed: &EmbedClient,
    kind: MemoryKind,
    source: MemorySource,
    scope: MemoryScope,
    confidence: f64,
    title: String,
    content: String,
    tags: Vec<String>,
    conversation_id: Option<String>,
    parent_ids: Vec<String>,
) -> AtlasResult<WriteDecision> {
    let vector = embed.embed(cfg, &format!("{title}\n{content}")).await?;
    let existing = store.list_memory_nodes(Some(kind))?;
    let conflict_low = cfg.memory_conflict_sim_low.clamp(0.5, cfg.memory_dedupe_threshold);

    // Near-duplicate → dedupe (keep stronger salience).
    for node in &existing {
        if node.archived_at.is_some() {
            continue;
        }
        if let Some(ev) = store.get_embedding(&node.id)? {
            let sim = cosine(&vector, &ev);
            if sim >= cfg.memory_dedupe_threshold {
                return Ok(WriteDecision {
                    action: WriteAction::Deduped,
                    node: Some(node.clone()),
                    reason: format!("near-duplicate van {} (sim={sim:.3})", node.id),
                });
            }
            // Same title family + high-ish sim → merge content if different.
            if !title.is_empty()
                && node.title.eq_ignore_ascii_case(&title)
                && sim >= cfg.memory_dedupe_threshold * 0.85
                && node.content != content
                && !content_diverges(&node.content, &content)
            {
                let mut merged = node.clone();
                merged.content = format!("{}\n---\n{content}", node.content);
                merged.updated_at = Utc::now().to_rfc3339();
                merged.provenance.confidence =
                    ((node.provenance.confidence + confidence) / 2.0).clamp(0.05, 1.0);
                store.upsert_memory_node(&merged)?;
                store.upsert_embedding(&merged.id, &vector, &embed_model_key(cfg))?;
                let _ = store.enqueue_outbox(
                    "memory.merged",
                    &serde_json::json!({
                        "node_id": merged.id,
                        "kind": merged.kind.as_str(),
                    }),
                );
                return Ok(WriteDecision {
                    action: WriteAction::Merged,
                    node: Some(merged),
                    reason: format!("samengevoegd met {}", node.id),
                });
            }

            // Conflict / truth: same neighborhood, divergent claims — keep both + edge.
            if matches!(kind, MemoryKind::Semantic | MemoryKind::Procedural | MemoryKind::SelfProfile)
                && sim >= conflict_low
                && content_diverges(&node.content, &content)
            {
                let now = Utc::now().to_rfc3339();
                let mut parents = parent_ids.clone();
                if !parents.contains(&node.id) {
                    parents.push(node.id.clone());
                }
                let new_node = MemoryNode {
                    id: Uuid::new_v4().to_string(),
                    kind,
                    title: title.clone(),
                    content: content.clone(),
                    tags: tags.clone(),
                    provenance: Provenance {
                        source,
                        created_at: now.clone(),
                        parent_ids: parents,
                        confidence,
                        scope,
                        conversation_id: conversation_id.clone(),
                    },
                    salience: 1.0,
                    decay: 1.0,
                    archived_at: None,
                    updated_at: now,
                };
                store.upsert_memory_node(&new_node)?;
                store.upsert_embedding(&new_node.id, &vector, &embed_model_key(cfg))?;

                // Soften the older claim slightly — both remain retrievable.
                let mut older = node.clone();
                older.provenance.confidence =
                    (older.provenance.confidence * 0.92).clamp(0.05, 1.0);
                older.updated_at = Utc::now().to_rfc3339();
                let _ = store.upsert_memory_node(&older);

                let _ = store.upsert_edge(
                    &older.id,
                    "conflicts_with",
                    &new_node.id,
                    sim,
                    &serde_json::json!({
                        "source": "write_governor",
                        "sim": sim,
                        "reason": "divergent content in same neighborhood",
                    }),
                );

                if kind == MemoryKind::Semantic {
                    let _ = store.mirror_knowledge_from_memory(&new_node);
                }
                let _ = store.enqueue_outbox(
                    "memory.conflict",
                    &serde_json::json!({
                        "node_id": new_node.id,
                        "conflicts_with": older.id,
                        "sim": sim,
                        "kind": kind.as_str(),
                    }),
                );
                return Ok(WriteDecision {
                    action: WriteAction::Conflicted,
                    node: Some(new_node),
                    reason: format!(
                        "conflict met {} (sim={sim:.3}) — beide bewaard + conflicts_with edge",
                        node.id
                    ),
                });
            }
        }
    }

    let now = Utc::now().to_rfc3339();
    let node = MemoryNode {
        id: Uuid::new_v4().to_string(),
        kind,
        title,
        content,
        tags,
        provenance: Provenance {
            source,
            created_at: now.clone(),
            parent_ids,
            confidence,
            scope,
            conversation_id,
        },
        salience: if kind == MemoryKind::Procedural {
            1.15
        } else {
            1.0
        },
        decay: 1.0,
        archived_at: None,
        updated_at: now,
    };
    store.upsert_memory_node(&node)?;
    store.upsert_embedding(&node.id, &vector, &embed_model_key(cfg))?;

    // Compat mirror into legacy knowledge table for semantic learns.
    if kind == MemoryKind::Semantic {
        let _ = store.mirror_knowledge_from_memory(&node);
    }

    let _ = store.enqueue_outbox(
        "memory.inserted",
        &serde_json::json!({
            "node_id": node.id,
            "kind": node.kind.as_str(),
            "conversation_id": node.provenance.conversation_id,
            "embedding_model_id": embed_model_key(cfg),
        }),
    );

    Ok(WriteDecision {
        action: WriteAction::Inserted,
        node: Some(node),
        reason: "committed via write governor".into(),
    })
}

pub fn embed_model_key(cfg: &BrainConfig) -> String {
    let model = if cfg.embed_model.trim().is_empty() {
        "default"
    } else {
        cfg.embed_model.trim()
    };
    format!("{}:{model}", cfg.embed_provider)
}

/// True when two texts are about a similar topic but assert different content.
fn content_diverges(a: &str, b: &str) -> bool {
    if a.trim().eq_ignore_ascii_case(b.trim()) {
        return false;
    }
    let aa: String = a.chars().take(160).collect::<String>().to_lowercase();
    let bb: String = b.chars().take(160).collect::<String>().to_lowercase();
    if aa == bb {
        return false;
    }
    let ta = token_set(a);
    let tb = token_set(b);
    if ta.is_empty() || tb.is_empty() {
        return true;
    }
    let inter = ta.intersection(&tb).count() as f64;
    let union = ta.union(&tb).count() as f64;
    let jaccard = if union > 0.0 { inter / union } else { 0.0 };
    // High embed sim + modest lexical overlap ⇒ likely competing claims.
    jaccard < 0.72
}

fn token_set(text: &str) -> std::collections::HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

fn chunk_text(title: &str, content: &str, max_chars: usize) -> Vec<String> {
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
        chunks.push(format!("{title}: {content}"));
    }
    chunks
}
