use crate::config::BrainConfig;
use crate::error::AtlasResult;
use crate::memory::attention::{fuse_hits, multi_head_recall, route_query};
use crate::memory::decay::bump_salience;
use crate::memory::embed::EmbedClient;
use crate::memory::residual::{emit_context, finalize_trace, EmitResult};
use crate::memory::types::{MemoryHit, MemoryKind};
use crate::store::{ChatMessage, Store};

pub struct CortexRecall {
    pub hits: Vec<MemoryHit>,
    pub emit: EmitResult,
}

/// One-brain memory pass: route → multi-head attention → fuse → residual emit.
/// Optional second hop re-queries with top hit titles (still request-scoped).
pub async fn recall_into_context(
    store: &Store,
    cfg: &BrainConfig,
    embed: &EmbedClient,
    query: &str,
    conversation: &[ChatMessage],
    conversation_id: Option<&str>,
    want_trace: bool,
) -> AtlasResult<CortexRecall> {
    let router = route_query(query);
    let top_k = cfg.knowledge_top_k.max(1);
    let mut heads =
        multi_head_recall(store, cfg, embed, query, top_k, conversation_id, &router).await?;

    // Multi-hop (2nd block): expand query with top semantic titles if present.
    if cfg.memory_hops > 1 {
        let fused_tmp = fuse_hits(&heads, top_k);
        let expand = fused_tmp
            .iter()
            .take(2)
            .map(|h| h.title.as_str())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !expand.is_empty() {
            let q2 = format!("{query}\n{expand}");
            let heads2 =
                multi_head_recall(store, cfg, embed, &q2, top_k, conversation_id, &router).await?;
            heads.extend(heads2);
        }
    }

    let fused = fuse_hits(&heads, top_k.saturating_mul(2).max(top_k));

    let conv_pairs: Vec<(String, String)> = conversation
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant" || m.role == "system")
        .map(|m| {
            let content = match &m.callout {
                Some(c) => format!("{}\n\n{}\n{}", m.content, c.title, c.content),
                None => m.content.clone(),
            };
            (m.role.clone(), content)
        })
        .collect();

    let mut emit = emit_context(
        cfg,
        &cfg.system_prompt,
        &router,
        &heads,
        &fused,
        &conv_pairs,
    );
    if want_trace {
        emit = finalize_trace(emit);
    } else {
        emit.trace.frames.clear();
        emit.trace.heads.clear();
    }

    // Request-driven salience write-back for used semantic/self hits.
    for hit in fused.iter().filter(|h| {
        matches!(
            h.kind,
            MemoryKind::Semantic | MemoryKind::SelfProfile | MemoryKind::Episodic
        )
    }) {
        if let Some(mut node) = store.get_memory_node(&hit.id)? {
            node.salience = bump_salience(node.salience);
            node.updated_at = chrono::Utc::now().to_rfc3339();
            let _ = store.upsert_memory_node(&node);
        }
    }

    Ok(CortexRecall {
        hits: fused,
        emit,
    })
}
