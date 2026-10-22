use crate::config::BrainConfig;
use crate::error::AtlasResult;
use crate::memory::embed::{decode_vector, encode_vector};
use crate::memory::types::{MemoryHit, MemoryKind, RouterDecision};
use crate::store::Store;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

/// Session working memory / KV-analog — expiry checked on read only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingMemoryState {
    pub conversation_id: String,
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default)]
    pub recent_hits: Vec<MemoryHit>,
    #[serde(default)]
    pub last_router: RouterDecision,
    #[serde(default)]
    pub last_query: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub expires_at: String,
}

impl WorkingMemoryState {
    pub fn fresh(conversation_id: &str, ttl_secs: i64) -> Self {
        let now = Utc::now();
        Self {
            conversation_id: conversation_id.to_string(),
            pinned: Vec::new(),
            recent_hits: Vec::new(),
            last_router: RouterDecision::default(),
            last_query: String::new(),
            updated_at: now.to_rfc3339(),
            expires_at: (now + Duration::seconds(ttl_secs.max(60))).to_rfc3339(),
        }
    }

    pub fn is_expired(&self) -> bool {
        chrono::DateTime::parse_from_rfc3339(&self.expires_at)
            .map(|dt| Utc::now() > dt.with_timezone(&Utc))
            .unwrap_or(true)
    }
}

pub fn load_working(
    store: &Store,
    cfg: &BrainConfig,
    conversation_id: &str,
) -> AtlasResult<Option<WorkingMemoryState>> {
    let Some(mut state) = store.get_working_memory(conversation_id)? else {
        return Ok(None);
    };
    if state.is_expired() {
        store.delete_working_memory(conversation_id)?;
        return Ok(None);
    }
    // Refresh TTL on read (still request-driven).
    let now = Utc::now();
    state.updated_at = now.to_rfc3339();
    state.expires_at =
        (now + Duration::seconds(cfg.working_ttl_secs.max(60) as i64)).to_rfc3339();
    store.upsert_working_memory(&state, None)?;
    Ok(Some(state))
}

pub fn save_working(
    store: &Store,
    cfg: &BrainConfig,
    conversation_id: &str,
    query: &str,
    router: &RouterDecision,
    hits: &[MemoryHit],
    query_embedding: Option<&[f32]>,
    pin_snippets: &[String],
) -> AtlasResult<WorkingMemoryState> {
    let mut state = store
        .get_working_memory(conversation_id)?
        .filter(|s| !s.is_expired())
        .unwrap_or_else(|| WorkingMemoryState::fresh(conversation_id, cfg.working_ttl_secs as i64));

    state.last_query = query.to_string();
    state.last_router = router.clone();
    state.recent_hits = hits.iter().take(8).cloned().collect();
    for pin in pin_snippets {
        let p = pin.trim();
        if p.is_empty() {
            continue;
        }
        if !state.pinned.iter().any(|x| x == p) {
            state.pinned.push(p.to_string());
        }
    }
    if state.pinned.len() > cfg.working_max_pins {
        let drop_n = state.pinned.len() - cfg.working_max_pins;
        state.pinned.drain(0..drop_n);
    }
    let now = Utc::now();
    state.updated_at = now.to_rfc3339();
    state.expires_at =
        (now + Duration::seconds(cfg.working_ttl_secs.max(60) as i64)).to_rfc3339();

    let emb_bytes = query_embedding
        .map(encode_vector)
        .transpose()?;
    store.upsert_working_memory(&state, emb_bytes.as_deref())?;
    Ok(state)
}

/// Materialize working KV as synthetic MemoryHit list for the working head.
pub fn working_hits(state: &WorkingMemoryState, router_weight: f64) -> Vec<MemoryHit> {
    let mut hits = Vec::new();
    for (i, pin) in state.pinned.iter().enumerate() {
        hits.push(MemoryHit {
            id: format!("working-pin-{}", i),
            kind: MemoryKind::Working,
            title: "working pin".into(),
            content: pin.clone(),
            score: 0.55 * router_weight,
            head: "working".into(),
            confidence: 0.9,
            salience: 1.2,
        });
    }
    for hit in state.recent_hits.iter().take(4) {
        let mut h = hit.clone();
        h.kind = MemoryKind::Working;
        h.head = "working".into();
        h.score = (hit.score * 0.65 * router_weight).max(0.05);
        hits.push(h);
    }
    if !state.last_query.is_empty() {
        hits.push(MemoryHit {
            id: "working-last-query".into(),
            kind: MemoryKind::Working,
            title: "last query".into(),
            content: state.last_query.clone(),
            score: 0.25 * router_weight,
            head: "working".into(),
            confidence: 0.7,
            salience: 1.0,
        });
    }
    hits
}

pub fn load_working_embedding(store: &Store, conversation_id: &str) -> AtlasResult<Option<Vec<f32>>> {
    match store.get_working_embedding(conversation_id)? {
        Some(bytes) => Ok(Some(decode_vector(&bytes)?)),
        None => Ok(None),
    }
}
