use crate::error::{AtlasError, AtlasResult};
use crate::memory::eval_runner::{ensure_default_eval_cases, run_eval_suite, RunEvalRequest};
use crate::state::AppState;
use crate::store::{EvalCase, FeedbackRecord, TrainingExample};
use axum::extract::{Query, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct SuiteQuery {
    #[serde(default)]
    pub suite: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct EvalCaseBody {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub suite: Option<String>,
    pub name: String,
    pub query: String,
    #[serde(default)]
    pub expect: Option<Value>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct FeedbackBody {
    #[serde(default)]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub correction: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CachePutBody {
    pub key: String,
    pub response: String,
    #[serde(default)]
    pub model_id: Option<String>,
}

pub async fn list_cases(
    State(state): State<AppState>,
    Query(q): Query<SuiteQuery>,
) -> AtlasResult<Json<Vec<EvalCase>>> {
    Ok(Json(state.store.list_eval_cases(q.suite.as_deref())?))
}

pub async fn upsert_case(
    State(state): State<AppState>,
    Json(body): Json<EvalCaseBody>,
) -> AtlasResult<Json<EvalCase>> {
    if body.name.trim().is_empty() || body.query.trim().is_empty() {
        return Err(AtlasError::BadRequest(
            "eval case name en query zijn verplicht".into(),
        ));
    }
    let case = EvalCase {
        id: body
            .id
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        suite: body.suite.unwrap_or_else(|| "default".into()),
        name: body.name,
        query: body.query,
        expect: body.expect.unwrap_or_else(|| json!({})),
        tags: body.tags,
        created_at: Utc::now().to_rfc3339(),
    };
    state.store.upsert_eval_case(&case)?;
    Ok(Json(case))
}

pub async fn run_suite(
    State(state): State<AppState>,
    Json(body): Json<RunEvalRequest>,
) -> AtlasResult<Json<crate::memory::eval_runner::RunEvalResult>> {
    let cfg = state.config.read().await.clone();
    let _ = ensure_default_eval_cases(&state.store)?;
    let result = run_eval_suite(&state.store, &cfg, &state.embed, body).await?;
    Ok(Json(result))
}

pub async fn list_runs(
    State(state): State<AppState>,
    Query(q): Query<SuiteQuery>,
) -> AtlasResult<Json<Value>> {
    let runs = state
        .store
        .list_eval_runs(q.suite.as_deref(), q.limit.unwrap_or(50))?;
    Ok(Json(json!({ "runs": runs })))
}

pub async fn post_feedback(
    State(state): State<AppState>,
    Json(body): Json<FeedbackBody>,
) -> AtlasResult<Json<Value>> {
    if !matches!(body.kind.as_str(), "up" | "down" | "correction") {
        return Err(AtlasError::BadRequest(
            "kind must be up, down, or correction".into(),
        ));
    }
    let fb = FeedbackRecord {
        id: Uuid::new_v4().to_string(),
        trace_id: body.trace_id.clone(),
        conversation_id: body.conversation_id.clone(),
        kind: body.kind.clone(),
        note: body.note.unwrap_or_default(),
        correction: body.correction.clone(),
        created_at: Utc::now().to_rfc3339(),
    };
    state.store.insert_feedback(&fb)?;

    // Corrections become training examples automatically.
    let mut training = None;
    if body.kind == "correction" {
        if let (Some(input), Some(output)) = (body.input.clone(), body.correction.clone()) {
            let ex = TrainingExample {
                id: Uuid::new_v4().to_string(),
                source: "feedback.correction".into(),
                input,
                output,
                trace_id: body.trace_id.clone(),
                meta: json!({ "conversation_id": body.conversation_id }),
                created_at: Utc::now().to_rfc3339(),
            };
            state.store.insert_training_example(&ex)?;
            training = Some(ex);
        }
    }

    Ok(Json(json!({ "feedback": fb, "training_example": training })))
}

pub async fn list_feedback(
    State(state): State<AppState>,
    Query(q): Query<SuiteQuery>,
) -> AtlasResult<Json<Vec<FeedbackRecord>>> {
    Ok(Json(
        state.store.list_feedback(q.limit.unwrap_or(50))?,
    ))
}

pub async fn list_training(
    State(state): State<AppState>,
    Query(q): Query<SuiteQuery>,
) -> AtlasResult<Json<Vec<TrainingExample>>> {
    Ok(Json(
        state
            .store
            .list_training_examples(q.limit.unwrap_or(100))?,
    ))
}

pub async fn cache_lookup(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> AtlasResult<Json<Value>> {
    let key = q
        .get("key")
        .cloned()
        .ok_or_else(|| AtlasError::BadRequest("key query param verplicht".into()))?;
    Ok(Json(json!({ "entry": state.store.cache_get(&key)? })))
}

pub async fn cache_put(
    State(state): State<AppState>,
    Json(body): Json<CachePutBody>,
) -> AtlasResult<Json<Value>> {
    let model = body.model_id.unwrap_or_else(|| "unknown".into());
    state.store.cache_put(&body.key, &body.response, &model)?;
    Ok(Json(json!({ "ok": true, "key": body.key })))
}

pub async fn cache_clear(State(state): State<AppState>) -> AtlasResult<Json<Value>> {
    let n = state.store.cache_invalidate_all()?;
    Ok(Json(json!({ "deleted": n })))
}

pub fn semantic_cache_key(model: &str, prompt_ver: &str, user_msg: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(model.as_bytes());
    hasher.update(b"|");
    hasher.update(prompt_ver.as_bytes());
    hasher.update(b"|");
    hasher.update(user_msg.as_bytes());
    hex::encode(hasher.finalize())
}
