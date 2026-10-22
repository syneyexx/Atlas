use crate::brain::datasets::{
    acquire_dataset, approve_recipe, build_datamap, compile_mix_job, create_job, create_mix_def,
    pin_and_optionally_acquire, propose_recipe, search_hub, set_paused, tick_job, verify_recipe,
};
use crate::error::{AtlasError, AtlasResult};
use crate::state::AppState;
use crate::store::EvalGuard;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CatalogSearchQuery {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub modality: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDatasetBody {
    pub source: String,
    pub id: String,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub acquire: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateJobBody {
    pub dataset_id: String,
    pub kind: String,
    #[serde(default)]
    pub recipe_id: Option<String>,
    #[serde(default)]
    pub auto_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct TickBody {
    #[serde(default)]
    pub steps: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct PauseBody {
    pub paused: bool,
}

#[derive(Debug, Deserialize)]
pub struct ApproveBody {
    pub approve: bool,
}

#[derive(Debug, Deserialize)]
pub struct PutRecipeBody {
    pub body: Value,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecordsQuery {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EdgesQuery {
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub rel: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMixBody {
    pub name: String,
    pub definition: Value,
}

#[derive(Debug, Deserialize)]
pub struct EvalGuardBody {
    pub name: String,
    pub text: String,
    #[serde(default)]
    pub meta: Option<Value>,
}

pub async fn catalog_search(
    State(state): State<AppState>,
    Query(q): Query<CatalogSearchQuery>,
) -> AtlasResult<Json<Value>> {
    let cfg = state.config.read().await.clone();
    let cards = search_hub(
        &state.store,
        &cfg,
        q.q.as_deref().unwrap_or(""),
        q.modality.as_deref(),
        q.license.as_deref(),
        q.limit.unwrap_or(20),
    )?;
    Ok(Json(json!({ "results": cards })))
}

pub async fn catalog_get(
    State(state): State<AppState>,
    Path((source, id)): Path<(String, String)>,
) -> AtlasResult<Json<Value>> {
    let _ = crate::brain::datasets::seed_stub_catalog(&state.store)?;
    let card = state
        .store
        .get_catalog_card(&source, &id)?
        .ok_or_else(|| AtlasError::NotFound(format!("{source}/{id}")))?;
    Ok(Json(card))
}

pub async fn create_dataset(
    State(state): State<AppState>,
    Json(body): Json<CreateDatasetBody>,
) -> AtlasResult<Json<Value>> {
    let cfg = state.config.read().await.clone();
    let payload = pin_and_optionally_acquire(
        &state.store,
        &cfg,
        &body.source,
        &body.id,
        body.revision.as_deref(),
        body.project_id,
        body.acquire.unwrap_or(false),
    )?;
    Ok(Json(payload))
}

pub async fn list_datasets(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> AtlasResult<Json<Value>> {
    let datasets = state.store.list_datasets(q.limit.unwrap_or(50))?;
    Ok(Json(json!({ "datasets": datasets })))
}

pub async fn get_dataset(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    let ds = state
        .store
        .get_dataset(&id)?
        .ok_or_else(|| AtlasError::NotFound(format!("dataset {id}")))?;
    let shards = state.store.list_shards(&id)?;
    let recipes = state.store.list_recipes(&id)?;
    Ok(Json(json!({ "dataset": ds, "shards": shards, "recipes": recipes })))
}

pub async fn acquire(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    let cfg = state.config.read().await.clone();
    let ds = acquire_dataset(&state.store, &cfg, &id, &cfg.secrets_key)?;
    let shards = state.store.list_shards(&id)?;
    Ok(Json(json!({ "dataset": ds, "shards": shards })))
}

pub async fn sample_propose(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    let cfg = state.config.read().await.clone();
    let ds = state
        .store
        .get_dataset(&id)?
        .ok_or_else(|| AtlasError::NotFound(format!("dataset {id}")))?;
    if ds.root_hash.is_empty() {
        let _ = acquire_dataset(&state.store, &cfg, &id, &cfg.secrets_key)?;
    }
    let ds = state.store.get_dataset(&id)?.unwrap();
    let (recipe, report) = propose_recipe(&state.store, &ds)?;
    let verify = verify_recipe(&recipe)?;
    Ok(Json(json!({ "recipe": recipe, "sample_report": report, "verify": verify })))
}

pub async fn list_recipes(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    Ok(Json(json!({ "recipes": state.store.list_recipes(&id)? })))
}

pub async fn get_recipe(
    State(state): State<AppState>,
    Path(recipe_id): Path<String>,
) -> AtlasResult<Json<Value>> {
    let recipe = state
        .store
        .get_recipe(&recipe_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("recipe {recipe_id}")))?;
    Ok(Json(json!({ "recipe": recipe })))
}

pub async fn put_recipe(
    State(state): State<AppState>,
    Path(recipe_id): Path<String>,
    Json(body): Json<PutRecipeBody>,
) -> AtlasResult<Json<Value>> {
    let mut recipe = state
        .store
        .get_recipe(&recipe_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("recipe {recipe_id}")))?;
    recipe.body = body.body;
    if let Some(v) = body.version {
        recipe.version = v;
    }
    recipe.status = "draft".into();
    recipe.verified = false;
    recipe.updated_at = Utc::now().to_rfc3339();
    let verify = verify_recipe(&recipe)?;
    state.store.upsert_recipe(&recipe)?;
    Ok(Json(json!({ "recipe": recipe, "verify": verify })))
}

pub async fn verify_recipe_api(
    State(state): State<AppState>,
    Path(recipe_id): Path<String>,
) -> AtlasResult<Json<Value>> {
    let recipe = state
        .store
        .get_recipe(&recipe_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("recipe {recipe_id}")))?;
    Ok(Json(verify_recipe(&recipe)?))
}

pub async fn approve(
    State(state): State<AppState>,
    Path(recipe_id): Path<String>,
    Json(body): Json<ApproveBody>,
) -> AtlasResult<Json<Value>> {
    let recipe = approve_recipe(&state.store, &recipe_id, body.approve)?;
    Ok(Json(json!({ "recipe": recipe })))
}

pub async fn create_job_api(
    State(state): State<AppState>,
    Json(body): Json<CreateJobBody>,
) -> AtlasResult<Json<Value>> {
    let cfg = state.config.read().await.clone();
    let job = create_job(
        &state.store,
        &cfg,
        &body.dataset_id,
        &body.kind,
        body.recipe_id,
    )?;
    let mut job = job;
    if body.auto_run.unwrap_or(true) {
        let max_steps = cfg.datasets_max_ticks.min(24);
        for _ in 0..max_steps {
            let result = tick_job(&state.store, &cfg, &job.id).await?;
            job = result.job;
            if result.done || job.status == "blocked" || job.paused || job.phase == "await_approval"
            {
                break;
            }
        }
    }
    Ok(Json(job_payload(&state, &job.id)?))
}

pub async fn list_jobs(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> AtlasResult<Json<Value>> {
    Ok(Json(json!({
        "jobs": state.store.list_dataset_jobs(q.limit.unwrap_or(50))?
    })))
}

pub async fn get_job(
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
    let steps = body.and_then(|b| b.0.steps).unwrap_or(1).clamp(1, 32);
    let mut last_msg = String::new();
    let mut done = false;
    for _ in 0..steps {
        let result = tick_job(&state.store, &cfg, &id).await?;
        last_msg = result.message;
        done = result.done;
        if result.done
            || result.job.status == "blocked"
            || result.job.paused
            || result.job.phase == "await_approval"
            || !result.advanced
        {
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

pub async fn records(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<RecordsQuery>,
) -> AtlasResult<Json<Value>> {
    if let Some(query) = q.q.as_deref().filter(|s| !s.trim().is_empty()) {
        let rows = state.store.search_dataset_records(
            Some(&id),
            query,
            q.limit.unwrap_or(50),
        )?;
        return Ok(Json(json!({ "records": rows })));
    }
    let rows = state.store.list_dataset_records(
        &id,
        q.limit.unwrap_or(50),
        q.offset.unwrap_or(0),
    )?;
    Ok(Json(json!({ "records": rows })))
}

pub async fn facets(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    Ok(Json(state.store.record_facets(&id)?))
}

pub async fn edges(
    State(state): State<AppState>,
    Query(q): Query<EdgesQuery>,
) -> AtlasResult<Json<Value>> {
    let edges = state.store.list_dataset_edges(
        q.node.as_deref(),
        q.rel.as_deref(),
        q.limit.unwrap_or(100),
    )?;
    Ok(Json(json!({ "edges": edges })))
}

pub async fn neighbors(
    State(state): State<AppState>,
    Path(node): Path<String>,
) -> AtlasResult<Json<Value>> {
    let edges = state
        .store
        .list_dataset_edges(Some(&node), None, 100)?;
    Ok(Json(json!({ "node": node, "edges": edges })))
}

pub async fn subgraph(
    State(state): State<AppState>,
    Path(node): Path<String>,
) -> AtlasResult<Json<Value>> {
    let edges = state
        .store
        .list_dataset_edges(Some(&node), None, 200)?;
    let mut nodes = vec![node.clone()];
    for e in &edges {
        if !nodes.contains(&e.src) {
            nodes.push(e.src.clone());
        }
        if !nodes.contains(&e.dst) {
            nodes.push(e.dst.clone());
        }
    }
    Ok(Json(json!({ "nodes": nodes, "edges": edges })))
}

pub async fn search_records(
    State(state): State<AppState>,
    Query(q): Query<RecordsQuery>,
) -> AtlasResult<Json<Value>> {
    let query = q.q.as_deref().unwrap_or("").trim();
    if query.is_empty() {
        return Err(AtlasError::BadRequest("q verplicht".into()));
    }
    let rows = state
        .store
        .search_dataset_records(None, query, q.limit.unwrap_or(50))?;
    Ok(Json(json!({ "records": rows })))
}

pub async fn datamap(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    Ok(Json(build_datamap(&state.store, &id)?))
}

pub async fn create_mix(
    State(state): State<AppState>,
    Json(body): Json<CreateMixBody>,
) -> AtlasResult<Json<Value>> {
    let mix = create_mix_def(&state.store, &body.name, body.definition)?;
    Ok(Json(json!({ "mix": mix })))
}

pub async fn list_mixes(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> AtlasResult<Json<Value>> {
    Ok(Json(json!({ "mixes": state.store.list_mixes(q.limit.unwrap_or(50))? })))
}

pub async fn get_mix(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    let mix = state
        .store
        .get_mix(&id)?
        .ok_or_else(|| AtlasError::NotFound(format!("mix {id}")))?;
    Ok(Json(json!({ "mix": mix })))
}

pub async fn compile_mix_api(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AtlasResult<Json<Value>> {
    let manifest = compile_mix_job(&state.store, &id)?;
    let mix = state.store.get_mix(&id)?;
    Ok(Json(json!({ "manifest": manifest, "mix": mix })))
}

pub async fn list_eval_guards(
    State(state): State<AppState>,
) -> AtlasResult<Json<Value>> {
    Ok(Json(json!({ "guards": state.store.list_eval_guards(None)? })))
}

pub async fn put_eval_guard(
    State(state): State<AppState>,
    Json(body): Json<EvalGuardBody>,
) -> AtlasResult<Json<Value>> {
    let hash = crate::store::Store::content_hash(body.text.as_bytes());
    let g = EvalGuard {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        text_hash: hash,
        text: body.text,
        meta: body.meta.unwrap_or(json!({})),
        created_at: Utc::now().to_rfc3339(),
    };
    state.store.upsert_eval_guard(&g)?;
    Ok(Json(json!({ "guard": g })))
}

fn job_payload(state: &AppState, id: &str) -> AtlasResult<Value> {
    let job = state
        .store
        .get_dataset_job(id)?
        .ok_or_else(|| AtlasError::NotFound(format!("job {id}")))?;
    let events = state.store.list_dataset_events(id)?;
    Ok(json!({
        "job": job,
        "events": events
    }))
}
