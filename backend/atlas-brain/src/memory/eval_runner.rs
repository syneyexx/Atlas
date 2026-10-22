use crate::config::BrainConfig;
use crate::error::AtlasResult;
use crate::memory::embed::EmbedClient;
use crate::memory::recall_into_context;
use crate::store::{EvalCase, EvalRun, Store};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct RunEvalRequest {
    #[serde(default)]
    pub suite: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunEvalResult {
    pub suite: String,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub avg_score: f64,
    pub runs: Vec<EvalRun>,
}

/// Request-driven eval: for each case, recall and score against expect.contains / expect.kinds.
pub async fn run_eval_suite(
    store: &Store,
    cfg: &BrainConfig,
    embed: &EmbedClient,
    req: RunEvalRequest,
) -> AtlasResult<RunEvalResult> {
    let suite = req.suite.unwrap_or_else(|| "default".into());
    let cases = store.list_eval_cases(Some(&suite))?;
    let mut runs = Vec::new();
    let mut passed = 0usize;
    let mut score_sum = 0.0;

    for case in &cases {
        let (score, detail, trace_id) = score_case(store, cfg, embed, case).await?;
        let ok = score >= 0.5;
        if ok {
            passed += 1;
        }
        score_sum += score;
        let run = EvalRun {
            id: Uuid::new_v4().to_string(),
            suite: suite.clone(),
            case_id: case.id.clone(),
            score,
            passed: ok,
            trace_id,
            detail,
            created_at: Utc::now().to_rfc3339(),
        };
        store.insert_eval_run(&run)?;
        runs.push(run);
    }

    let total = cases.len();
    Ok(RunEvalResult {
        suite,
        total,
        passed,
        failed: total.saturating_sub(passed),
        avg_score: if total == 0 {
            0.0
        } else {
            score_sum / total as f64
        },
        runs,
    })
}

async fn score_case(
    store: &Store,
    cfg: &BrainConfig,
    embed: &EmbedClient,
    case: &EvalCase,
) -> AtlasResult<(f64, serde_json::Value, Option<String>)> {
    let recalled = recall_into_context(
        store,
        cfg,
        embed,
        &case.query,
        &[],
        None,
        true,
    )
    .await?;

    let hit_ids: Vec<String> = recalled.hits.iter().map(|h| h.id.clone()).collect();
    let blob = recalled
        .hits
        .iter()
        .map(|h| format!("{} {}", h.title, h.content))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();

    let mut checks = Vec::new();
    let mut scores = Vec::new();

    if let Some(needles) = case.expect.get("contains").and_then(|v| v.as_array()) {
        for n in needles {
            let s = n.as_str().unwrap_or("").to_lowercase();
            let ok = !s.is_empty() && blob.contains(&s);
            scores.push(if ok { 1.0 } else { 0.0 });
            checks.push(json!({ "contains": s, "ok": ok }));
        }
    }
    if let Some(kinds) = case.expect.get("kinds").and_then(|v| v.as_array()) {
        let hit_kinds: Vec<String> = recalled
            .hits
            .iter()
            .map(|h| h.kind.as_str().to_string())
            .collect();
        for k in kinds {
            let want = k.as_str().unwrap_or("");
            let ok = hit_kinds.iter().any(|h| h == want);
            scores.push(if ok { 1.0 } else { 0.0 });
            checks.push(json!({ "kind": want, "ok": ok }));
        }
    }
    if let Some(min_hits) = case.expect.get("min_hits").and_then(|v| v.as_u64()) {
        let ok = recalled.hits.len() as u64 >= min_hits;
        scores.push(if ok { 1.0 } else { 0.0 });
        checks.push(json!({ "min_hits": min_hits, "got": recalled.hits.len(), "ok": ok }));
    }

    let score = if scores.is_empty() {
        // No expectations → score by having any hit.
        if recalled.hits.is_empty() {
            0.0
        } else {
            1.0
        }
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    };

    let payload = json!({
        "query": case.query,
        "hit_ids": hit_ids,
        "recall": recalled.emit.trace,
        "checks": checks,
        "score": score,
    });
    let trace_id = store
        .insert_trace("eval", None, Some(cfg.model_id.as_str()), &payload)
        .ok();

    Ok((
        score,
        json!({ "checks": checks, "hits": recalled.hits.len() }),
        trace_id,
    ))
}

/// Seed a small default suite if empty (request-driven, on first run).
pub fn ensure_default_eval_cases(store: &Store) -> AtlasResult<usize> {
    if !store.list_eval_cases(Some("default"))?.is_empty() {
        return Ok(0);
    }
    let now = Utc::now().to_rfc3339();
    let cases = [
        EvalCase {
            id: Uuid::new_v4().to_string(),
            suite: "default".into(),
            name: "hybrid_bm25_term".into(),
            query: "BM25 keyword hybrid search".into(),
            expect: json!({ "contains": ["bm25"], "min_hits": 1 }),
            tags: vec!["rag".into()],
            created_at: now.clone(),
        },
        EvalCase {
            id: Uuid::new_v4().to_string(),
            suite: "default".into(),
            name: "procedural_howto".into(),
            query: "hoe workflow stappen procedure".into(),
            expect: json!({ "kinds": ["procedural"], "min_hits": 0 }),
            tags: vec!["procedural".into()],
            created_at: now,
        },
    ];
    let mut n = 0usize;
    for c in cases {
        store.upsert_eval_case(&c)?;
        n += 1;
    }
    Ok(n)
}
