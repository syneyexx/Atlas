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
        if kind == MemoryKind::Procedural {
            return Ok(WriteDecision {
                action: WriteAction::Discarded,
                node: None,
                reason: "procedural memory is later — nog niet schrijfbaar".into(),
            });
        }

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
) -> AtlasResult<WriteDecision> {
    let vector = embed.embed(cfg, &format!("{title}\n{content}")).await?;
    let existing = store.list_memory_nodes(Some(kind))?;

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
            {
                let mut merged = node.clone();
                merged.content = format!("{}\n---\n{content}", node.content);
                merged.updated_at = Utc::now().to_rfc3339();
                merged.provenance.confidence =
                    ((node.provenance.confidence + confidence) / 2.0).clamp(0.05, 1.0);
                store.upsert_memory_node(&merged)?;
                store.upsert_embedding(&merged.id, &vector)?;
                return Ok(WriteDecision {
                    action: WriteAction::Merged,
                    node: Some(merged),
                    reason: format!("samengevoegd met {}", node.id),
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
            parent_ids: Vec::new(),
            confidence,
            scope,
            conversation_id,
        },
        salience: 1.0,
        decay: 1.0,
        archived_at: None,
        updated_at: now,
    };
    store.upsert_memory_node(&node)?;
    store.upsert_embedding(&node.id, &vector)?;

    // Compat mirror into legacy knowledge table for semantic learns.
    if kind == MemoryKind::Semantic {
        let _ = store.mirror_knowledge_from_memory(&node);
    }

    Ok(WriteDecision {
        action: WriteAction::Inserted,
        node: Some(node),
        reason: "committed via write governor".into(),
    })
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
            // Hard split oversized paragraph.
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
