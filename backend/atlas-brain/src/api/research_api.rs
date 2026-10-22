use crate::brain::research::{answer_clarifications, create_job, set_paused, tick_job};
use crate::error::{AtlasError, AtlasResult};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct CreateResearchBody {
    pub question: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub attachment: Option<Value>,
    #[serde(default)]
    pub conversation_id: Option<String>,
    /// When true, run ticks until blocked/done (still request-scoped, no background loop).
    #[serde(default)]
    pub auto_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct JobsQuery {
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct PauseBody {
    pub paused: bool,
}

#[derive(Debug, Deserialize)]
pub struct ClarifyBody {
    pub answers: Value,
}

#[derive(Debug, Deserialize)]
pub struct TickBody {
    /// Run up to N ticks in this request (default 1, max 32).
    #[serde(default)]
    pub steps: Option<u32>,
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateResearchBody>,
) -> AtlasResult<Json<Value>> {
    let cfg = state.config.read().await.clone();
    let mode = body.mode.unwrap_or_else(|| "Verdiepend".into());
    let job = create_job(
        &state.store,
        &cfg,
        &body.question,
        &mode,
        body.attachment,
        body.conversation_id,
    )
    .await?;

    let mut job = job;
    if body.auto_run.unwrap_or(true) {
        // Drive the pipeline in-request until blocked, done, or step budget.
        let max_steps = cfg.research_max_ticks.min(24);
        for _ in 0..max_steps {
            let result = tick_job(&state.store, &cfg, &state.lm, &state.embed, &job.id).await?;
            job = result.job;
            if result.done || job.status == "blocked" || job.paused {
                break;
            }
        }
    }

    Ok(Json(job_payload(&state, &job.id)?))
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<JobsQuery>,
) -> AtlasResult<Json<Value>> {
    let jobs = state.store.list_research_jobs(q.limit.unwrap_or(50))?;
    Ok(Json(json!({ "jobs": jobs })))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    Ok(Json(job_payload(&state, &id)?))
}

pub async fn tick(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<TickBody>>,
) -> AtlasResult<Json<Value>> {
    let cfg = state.config.read().await.clone();
    let steps = body
        .and_then(|b| b.0.steps)
        .unwrap_or(1)
        .clamp(1, 32);
    let mut last_msg = String::new();
    let mut done = false;
    for _ in 0..steps {
        let result = tick_job(&state.store, &cfg, &state.lm, &state.embed, &id).await?;
        last_msg = result.message;
        done = result.done;
        if result.done || result.job.status == "blocked" || result.job.paused || !result.advanced {
            break;
        }
    }
    let mut payload = job_payload(&state, &id)?;
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("tick_message".into(), json!(last_msg));
        obj.insert("done".into(), json!(done));
    }
    Ok(Json(payload))
}

pub async fn pause(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PauseBody>,
) -> AtlasResult<Json<Value>> {
    let _ = set_paused(&state.store, &id, body.paused)?;
    Ok(Json(job_payload(&state, &id)?))
}

pub async fn clarify(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ClarifyBody>,
) -> AtlasResult<Json<Value>> {
    let cfg = state.config.read().await.clone();
    let _ = answer_clarifications(&state.store, &id, body.answers)?;
    // Continue after clarify
    let max_steps = cfg.research_max_ticks.min(24);
    for _ in 0..max_steps {
        let result = tick_job(&state.store, &cfg, &state.lm, &state.embed, &id).await?;
        if result.done || result.job.status == "blocked" || result.job.paused {
            break;
        }
    }
    Ok(Json(job_payload(&state, &id)?))
}

fn job_payload(state: &AppState, id: &str) -> AtlasResult<Value> {
    let job = state
        .store
        .get_research_job(id)?
        .ok_or_else(|| AtlasError::NotFound(format!("Research job {id} niet gevonden")))?;
    let events = state.store.list_research_events(id)?;
    let sources = state.store.list_research_sources(id)?;
    let claims = state.store.list_research_claims(id)?;
    let scratchpad = state.store.scratchpad_all(id).unwrap_or(json!({}));
    let findings = scratchpad
        .get("findings")
        .cloned()
        .unwrap_or(json!([]));
    let open_questions = job
        .plan
        .get("open_questions")
        .cloned()
        .unwrap_or(json!([]));
    Ok(json!({
        "job": job,
        "events": events,
        "sources": sources,
        "claims": claims,
        "findings": findings,
        "open_questions": open_questions,
        "scratchpad": scratchpad,
    }))
}
