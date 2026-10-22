use crate::config::BrainConfig;
use crate::error::AtlasResult;
use crate::memory::decay::{effective_salience, should_skip};
use crate::memory::embed::{cosine, EmbedClient};
use crate::memory::moe::expert_top_k;
use crate::memory::types::{
    AttentionHeadResult, MemoryHit, MemoryKind, MemoryNode, RouterDecision,
};
use crate::store::Store;
use chrono::Utc;
use std::collections::HashMap;

pub async fn multi_head_recall(
    store: &Store,
    cfg: &BrainConfig,
    embed: &EmbedClient,
    query: &str,
    top_k: usize,
    conversation_id: Option<&str>,
    router: &RouterDecision,
) -> AtlasResult<Vec<AttentionHeadResult>> {
    let now = Utc::now();
    let nodes = store.list_memory_nodes(None)?;
    let query_vec = embed.embed(cfg, query).await?;
    let query_lex = tokenize(query);
    // Hybrid: FTS5 BM25 ranks feed the lexical head (and lightly boost others).
    let fts_ranks: HashMap<String, f64> = store
        .search_memory_fts(query, top_k.saturating_mul(4).max(16))?
        .into_iter()
        .collect();

    let mut heads = Vec::new();
    heads.push(run_head(
        "semantic",
        router.semantic,
        &nodes,
        store,
        cfg,
        &query_vec,
        &query_lex,
        &fts_ranks,
        conversation_id,
        now,
        |n| n.kind == MemoryKind::Semantic,
        expert_top_k(top_k, router.semantic),
    )?);
    heads.push(run_head(
        "episodic",
        router.episodic,
        &nodes,
        store,
        cfg,
        &query_vec,
        &query_lex,
        &fts_ranks,
        conversation_id,
        now,
        |n| n.kind == MemoryKind::Episodic,
        expert_top_k(top_k, router.episodic),
    )?);
    heads.push(run_head(
        "self_profile",
        router.self_profile,
        &nodes,
        store,
        cfg,
        &query_vec,
        &query_lex,
        &fts_ranks,
        conversation_id,
        now,
        |n| n.kind == MemoryKind::SelfProfile,
        expert_top_k(top_k.max(4), router.self_profile),
    )?);
    heads.push(run_head(
        "lexical",
        0.2 + 0.3 * router.semantic,
        &nodes,
        store,
        cfg,
        &query_vec,
        &query_lex,
        &fts_ranks,
        conversation_id,
        now,
        |_| true,
        expert_top_k(top_k, 0.35),
    )?);
    heads.push(run_head(
        "recency",
        router.working.max(router.episodic * 0.5),
        &nodes,
        store,
        cfg,
        &query_vec,
        &query_lex,
        &fts_ranks,
        conversation_id,
        now,
        |n| n.kind == MemoryKind::Episodic || n.kind == MemoryKind::Working,
        expert_top_k(top_k, router.working.max(0.2)),
    )?);
    Ok(heads)
}

fn run_head(
    name: &str,
    head_weight: f64,
    nodes: &[MemoryNode],
    store: &Store,
    cfg: &BrainConfig,
    query_vec: &[f32],
    query_lex: &HashMap<String, f64>,
    fts_ranks: &HashMap<String, f64>,
    conversation_id: Option<&str>,
    now: chrono::DateTime<Utc>,
    filter: impl Fn(&MemoryNode) -> bool,
    top_k: usize,
) -> AtlasResult<AttentionHeadResult> {
    let mut scored: Vec<MemoryHit> = Vec::new();
    // Normalize FTS ranks to 0..1 for mixing.
    let fts_max = fts_ranks.values().cloned().fold(0.0_f64, f64::max).max(1e-6);
    for node in nodes.iter().filter(|n| filter(n)) {
        if should_skip(node, cfg, now) {
            continue;
        }
        if let (Some(cid), Some(nc)) = (conversation_id, node.provenance.conversation_id.as_deref())
        {
            if node.kind == MemoryKind::Episodic && nc != cid {
                // Soft preference: still allow, but later score dampens via scope.
            }
        }
        let sal = effective_salience(node, cfg, now);
        let fts = fts_ranks.get(&node.id).copied().unwrap_or(0.0) / fts_max;
        let mut score = match name {
            "lexical" => {
                // Hybrid lexical: in-memory TF + FTS5 BM25.
                let lex = lexical_score(node, query_lex);
                (0.45 * lex + 0.55 * fts) * head_weight * sal
            }
            "recency" => {
                let vec_score = store
                    .get_embedding(&node.id)?
                    .map(|v| cosine(query_vec, &v))
                    .unwrap_or(0.0);
                (0.55 * recency_boost(node) + 0.45 * vec_score) * head_weight * sal
            }
            "self_profile" => {
                let vec_score = store
                    .get_embedding(&node.id)?
                    .map(|v| cosine(query_vec, &v))
                    .unwrap_or(0.15);
                // Profile always has a floor so identity stays available.
                (vec_score.max(0.2) + 0.15) * head_weight * sal
            }
            _ => {
                let vec_score = store
                    .get_embedding(&node.id)?
                    .map(|v| cosine(query_vec, &v))
                    .unwrap_or(0.0);
                let lex = lexical_score(node, query_lex);
                // α·vector + β·bm25/lex (store-plane hybrid)
                (0.70 * vec_score + 0.18 * lex + 0.12 * fts) * head_weight * sal
            }
        };
        if conversation_id.is_some()
            && node.provenance.conversation_id.as_deref() == conversation_id
        {
            score *= 1.08;
        }
        if score < cfg.memory_min_hit_score {
            continue;
        }
        scored.push(MemoryHit {
            id: node.id.clone(),
            kind: node.kind,
            title: node.title.clone(),
            content: node.content.clone(),
            score,
            head: name.to_string(),
            confidence: node.provenance.confidence,
            salience: sal,
        });
    }
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // Light diversity: drop near-identical content after first.
    let mut unique = Vec::new();
    for hit in scored {
        if unique
            .iter()
            .any(|u: &MemoryHit| content_near_dup(&u.content, &hit.content))
        {
            continue;
        }
        unique.push(hit);
        if unique.len() >= top_k {
            break;
        }
    }
    Ok(AttentionHeadResult {
        head: name.to_string(),
        hits: unique,
    })
}

fn content_near_dup(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let aa: String = a.chars().take(120).collect();
    let bb: String = b.chars().take(120).collect();
    aa == bb
}

fn recency_boost(node: &MemoryNode) -> f64 {
    let age = chrono::DateTime::parse_from_rfc3339(&node.updated_at)
        .ok()
        .map(|dt| {
            let age_h = (Utc::now() - dt.with_timezone(&Utc)).num_minutes().max(0) as f64 / 60.0;
            (-0.08 * age_h).exp()
        })
        .unwrap_or(0.3);
    age.clamp(0.05, 1.0)
}

fn lexical_score(node: &MemoryNode, query: &HashMap<String, f64>) -> f64 {
    let blob = format!("{} {} {}", node.title, node.content, node.tags.join(" "));
    let doc = tokenize(&blob);
    if doc.is_empty() || query.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut qn = 0.0;
    let mut dn = 0.0;
    for (t, qw) in query {
        qn += qw * qw;
        if let Some(dw) = doc.get(t) {
            dot += qw * dw;
        }
    }
    for dw in doc.values() {
        dn += dw * dw;
    }
    if qn == 0.0 || dn == 0.0 {
        return 0.0;
    }
    dot / (qn.sqrt() * dn.sqrt())
}

fn tokenize(text: &str) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    for raw in text.split(|c: char| !c.is_alphanumeric()) {
        let t = raw.to_lowercase();
        if t.len() < 2 {
            continue;
        }
        *map.entry(t).or_insert(0.0) += 1.0;
    }
    for v in map.values_mut() {
        *v = 1.0 + f64::ln(*v);
    }
    map
}

/// Fuse heads into a single ranked list (logits → gate).
pub fn fuse_hits(heads: &[AttentionHeadResult], top_k: usize) -> Vec<MemoryHit> {
    let mut best: HashMap<String, MemoryHit> = HashMap::new();
    for head in heads {
        for hit in &head.hits {
            best.entry(hit.id.clone())
                .and_modify(|existing| {
                    if hit.score > existing.score {
                        *existing = hit.clone();
                    } else {
                        existing.score = (existing.score + hit.score * 0.25).min(existing.score * 1.15);
                    }
                })
                .or_insert_with(|| hit.clone());
        }
    }
    let mut all: Vec<MemoryHit> = best.into_values().collect();
    all.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    all.truncate(top_k);
    all
}

