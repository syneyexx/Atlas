use crate::config::BrainConfig;
use crate::store::{KnowledgeHit, KnowledgeNode, Store};
use crate::error::AtlasResult;
use std::collections::HashMap;

/// Lightweight on-demand lexical neural scoring.
/// No background embedding jobs — scoring runs only when recall is requested.
pub fn recall_knowledge(
    store: &Store,
    cfg: &BrainConfig,
    query: &str,
    top_k: Option<usize>,
) -> AtlasResult<Vec<KnowledgeHit>> {
    let k = top_k.unwrap_or(cfg.knowledge_top_k).max(1);
    if k == 0 || query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let nodes = store.list_knowledge()?;
    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return Ok(Vec::new());
    }
    let mut scored: Vec<KnowledgeHit> = nodes
        .into_iter()
        .filter_map(|node| score_node(&node, &query_tokens))
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(k);
    Ok(scored)
}

fn score_node(node: &KnowledgeNode, query_tokens: &HashMap<String, f64>) -> Option<KnowledgeHit> {
    let blob = format!("{} {} {}", node.title, node.content, node.tags.join(" "));
    let doc = tokenize(&blob);
    if doc.is_empty() {
        return None;
    }
    let mut dot = 0.0;
    let mut qn = 0.0;
    let mut dn = 0.0;
    for (t, qw) in query_tokens {
        qn += qw * qw;
        if let Some(dw) = doc.get(t) {
            dot += qw * dw;
        }
    }
    for dw in doc.values() {
        dn += dw * dw;
    }
    if qn == 0.0 || dn == 0.0 {
        return None;
    }
    let score = dot / (qn.sqrt() * dn.sqrt());
    if score <= 0.02 {
        return None;
    }
    Some(KnowledgeHit {
        id: node.id.clone(),
        title: node.title.clone(),
        content: node.content.clone(),
        score,
    })
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
    // tf log-scale
    for v in map.values_mut() {
        *v = 1.0 + f64::ln(*v);
    }
    map
}

pub fn format_knowledge_block(hits: &[KnowledgeHit], max_chars: usize) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from("[ATLAS_KNOWLEDGE]\nGebruik onderstaande opgeslagen kennis waar relevant:\n");
    for (i, hit) in hits.iter().enumerate() {
        let piece = format!(
            "\n### {}\n{}\n(score {:.3})\n",
            if hit.title.is_empty() {
                format!("kennis-{}", i + 1)
            } else {
                hit.title.clone()
            },
            hit.content.trim(),
            hit.score
        );
        if out.len() + piece.len() > max_chars {
            break;
        }
        out.push_str(&piece);
    }
    out.push_str("[/ATLAS_KNOWLEDGE]");
    out
}
