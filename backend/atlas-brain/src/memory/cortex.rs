use crate::config::BrainConfig;
use crate::error::AtlasResult;
use crate::memory::attention::{fuse_hits, multi_head_recall};
use crate::memory::decay::{bump_salience, soften_salience};
use crate::memory::embed::EmbedClient;
use crate::memory::moe::route_experts;
use crate::memory::residual::{emit_context, finalize_trace, EmitResult};
use crate::memory::types::{MemoryHit, MemoryKind};
use crate::memory::working::{load_working, save_working, working_hits};
use crate::store::{ChatMessage, Store};
use std::collections::HashSet;

pub struct CortexRecall {
    pub hits: Vec<MemoryHit>,
    pub emit: EmitResult,
}

/// One-brain memory pass: MoE route → multi-head (+ working KV) → fuse → residual → salience.
pub async fn recall_into_context(
    store: &Store,
    cfg: &BrainConfig,
    embed: &EmbedClient,
    query: &str,
    conversation: &[ChatMessage],
    conversation_id: Option<&str>,
    want_trace: bool,
) -> AtlasResult<CortexRecall> {
    let working = match conversation_id {
        Some(cid) => load_working(store, cfg, cid)?,
        None => None,
    };
    let router = route_experts(query, working.as_ref());
    let top_k = cfg.knowledge_top_k.max(1);

    let mut heads =
        multi_head_recall(store, cfg, embed, query, top_k, conversation_id, &router).await?;

    // Working KV head (M3) — synthetic hits from session state.
    if let Some(ref wm) = working {
        let wh = working_hits(wm, router.working);
        if !wh.is_empty() {
            heads.push(crate::memory::types::AttentionHeadResult {
                head: "working".into(),
                hits: wh,
            });
        }
    }

    if cfg.memory_hops > 1 {
        let fused_tmp = fuse_hits(&heads, top_k);
        let mut expand_parts: Vec<String> = fused_tmp
            .iter()
            .take(2)
            .map(|h| h.title.clone())
            .filter(|t| !t.is_empty())
            .collect();
        // Store-plane graph hop: follow memory_edges from top hits.
        for hit in fused_tmp.iter().take(3) {
            if hit.id.starts_with("working-") {
                continue;
            }
            for nid in store.neighbor_ids(&hit.id, 4)? {
                if let Some(n) = store.get_memory_node(&nid)? {
                    if !n.title.is_empty() {
                        expand_parts.push(n.title);
                    }
                }
            }
        }
        expand_parts.truncate(6);
        let expand = expand_parts.join(" ");
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

    // M4 salience: bump used hits, soften near-miss candidates from heads.
    let used_ids: HashSet<String> = fused.iter().map(|h| h.id.clone()).collect();
    for hit in &fused {
        if hit.id.starts_with("working-") {
            continue;
        }
        if let Some(mut node) = store.get_memory_node(&hit.id)? {
            node.salience = bump_salience(node.salience);
            node.updated_at = chrono::Utc::now().to_rfc3339();
            let _ = store.upsert_memory_node(&node);
        }
    }
    for head in &heads {
        for hit in head.hits.iter().skip(1).take(3) {
            if used_ids.contains(&hit.id) || hit.id.starts_with("working-") {
                continue;
            }
            if let Some(mut node) = store.get_memory_node(&hit.id)? {
                node.salience = soften_salience(node.salience);
                node.updated_at = chrono::Utc::now().to_rfc3339();
                let _ = store.upsert_memory_node(&node);
            }
        }
    }

    // Persist working KV for this conversation (on-demand write).
    if let Some(cid) = conversation_id {
        let q_emb = embed.embed(cfg, query).await.ok();
        let pins: Vec<String> = fused
            .iter()
            .filter(|h| h.kind == MemoryKind::SelfProfile || h.score > 0.45)
            .take(2)
            .map(|h| {
                if h.title.is_empty() {
                    h.content.chars().take(160).collect()
                } else {
                    format!("{} — {}", h.title, h.content.chars().take(120).collect::<String>())
                }
            })
            .collect();
        let _ = save_working(
            store,
            cfg,
            cid,
            query,
            &router,
            &fused,
            q_emb.as_deref(),
            &pins,
        );
    }

    Ok(CortexRecall {
        hits: fused,
        emit,
    })
}
