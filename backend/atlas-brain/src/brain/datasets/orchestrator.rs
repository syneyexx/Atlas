use crate::brain::datasets::engine::{
    build_datamap, compile_mix, propose_recipe, run_pipeline_pass, verify_recipe,
};
use crate::brain::datasets::hub::{acquire_dataset, create_pinned_dataset};
use crate::config::BrainConfig;
use crate::error::{AtlasError, AtlasResult};
use crate::store::{
    ArtifactRecord, CostEntry, DatasetJob, DatasetMix, DatasetRecipe, Store, TaskRecord,
};
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TickResult {
    pub job: DatasetJob,
    pub advanced: bool,
    pub done: bool,
    pub message: String,
}

pub fn create_job(
    store: &Store,
    cfg: &BrainConfig,
    dataset_id: &str,
    kind: &str,
    recipe_id: Option<String>,
) -> AtlasResult<DatasetJob> {
    let ds = store
        .get_dataset(dataset_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("Dataset {dataset_id}")))?;
    let kind = match kind {
        "sample" | "full" => kind,
        _ => {
            return Err(AtlasError::BadRequest(
                "kind must be sample or full".into(),
            ))
        }
    };
    if kind == "full" {
        let rid = recipe_id
            .as_deref()
            .ok_or_else(|| AtlasError::BadRequest("full job requires recipe_id".into()))?;
        let recipe = store
            .get_recipe(rid)?
            .ok_or_else(|| AtlasError::NotFound(format!("recipe {rid}")))?;
        if recipe.status != "approved" {
            return Err(AtlasError::BadRequest(
                "Full run vereist goedgekeurd recept (human gate na sample).".into(),
            ));
        }
    }

    let now = Utc::now().to_rfc3339();
    let job_id = Uuid::new_v4().to_string();
    let task = TaskRecord {
        id: Uuid::new_v4().to_string(),
        title: format!("datasets · {kind} · {}", ds.hub_id),
        status: "queued".into(),
        conversation_id: None,
        parent_id: None,
        plan: json!({ "kind": "datasets", "job_kind": kind }),
        state: json!({ "job_id": job_id, "dataset_id": dataset_id }),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    store.create_task(&task)?;

    let trace_id = store.insert_trace(
        "datasets.job",
        None,
        if cfg.model_id.is_empty() {
            None
        } else {
            Some(cfg.model_id.as_str())
        },
        &json!({ "job_id": job_id, "dataset_id": dataset_id, "kind": kind }),
    )?;

    let phase = if kind == "sample" {
        "sampling"
    } else {
        "running"
    };

    let job = DatasetJob {
        id: job_id,
        dataset_id: dataset_id.to_string(),
        recipe_id,
        kind: kind.to_string(),
        status: "running".into(),
        phase: phase.into(),
        checkpoint: json!({ "step_idx": 0 }),
        budget: json!({
            "max_ticks": cfg.datasets_max_ticks,
            "ticks_used": 0,
            "max_cost_usd": cfg.datasets_max_cost_usd,
            "cost_usd": 0.0,
            "max_disk_bytes": cfg.datasets_disk_budget_bytes
        }),
        metrics: json!({}),
        paused: false,
        task_id: Some(task.id.clone()),
        trace_id: Some(trace_id),
        created_at: now.clone(),
        updated_at: now,
    };
    store.create_dataset_job(&job)?;
    store.append_dataset_event(
        &job.id,
        "QUEUE",
        &[
            format!("datasets.{} · {kind}", short_id(&job.id)),
            format!("dataset: {}", ds.hub_id),
            format!("revision: {}", ds.revision),
        ],
        Some(&format!("task_id={}", task.id)),
    )?;
    store.update_task_status(
        &task.id,
        "running",
        Some(&json!({ "job_id": job.id, "phase": job.phase })),
    )?;
    let _ = store.enqueue_outbox(
        "dataset.job_created",
        &json!({ "job_id": job.id, "kind": kind, "dataset_id": dataset_id }),
    );
    Ok(job)
}

pub fn set_paused(store: &Store, job_id: &str, paused: bool) -> AtlasResult<DatasetJob> {
    let mut job = store
        .get_dataset_job(job_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("job {job_id}")))?;
    job.paused = paused;
    if paused {
        job.status = "paused".into();
    } else if job.status == "paused" {
        job.status = "running".into();
    }
    job.updated_at = Utc::now().to_rfc3339();
    store.update_dataset_job(&job)?;
    store.append_dataset_event(
        &job.id,
        if paused { "PAUSE" } else { "RESUME" },
        &[if paused {
            "Gepauzeerd".into()
        } else {
            "Hervat".into()
        }],
        None,
    )?;
    Ok(job)
}

pub fn approve_recipe(
    store: &Store,
    recipe_id: &str,
    approve: bool,
) -> AtlasResult<DatasetRecipe> {
    let mut recipe = store
        .get_recipe(recipe_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("recipe {recipe_id}")))?;
    let ver = verify_recipe(&recipe)?;
    if approve && !ver.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Err(AtlasError::BadRequest(format!(
            "Recipe verify failed: {ver}"
        )));
    }
    recipe.status = if approve {
        "approved".into()
    } else {
        "rejected".into()
    };
    recipe.verified = approve;
    recipe.updated_at = Utc::now().to_rfc3339();
    store.upsert_recipe(&recipe)?;

    // Close any await_approval sample jobs for this dataset
    for mut job in store.list_dataset_jobs(50)? {
        if job.dataset_id == recipe.dataset_id && job.phase == "await_approval" {
            if approve {
                job.status = "completed".into();
                job.phase = "done".into();
                store.append_dataset_event(
                    &job.id,
                    "APPROVE",
                    &["Human gate: recept goedgekeurd".into()],
                    Some(recipe_id),
                )?;
            } else {
                job.status = "failed".into();
                job.phase = "done".into();
                store.append_dataset_event(
                    &job.id,
                    "REJECT",
                    &["Human gate: recept afgewezen".into()],
                    Some(recipe_id),
                )?;
            }
            job.updated_at = Utc::now().to_rfc3339();
            store.update_dataset_job(&job)?;
            if let Some(tid) = &job.task_id {
                let st = if approve { "completed" } else { "failed" };
                let _ = store.update_task_status(tid, st, None);
            }
        }
    }

    let _ = store.enqueue_outbox(
        if approve {
            "dataset.recipe_approved"
        } else {
            "dataset.recipe_rejected"
        },
        &json!({ "recipe_id": recipe.id, "dataset_id": recipe.dataset_id }),
    );
    Ok(recipe)
}

pub async fn tick_job(
    store: &Store,
    cfg: &BrainConfig,
    job_id: &str,
) -> AtlasResult<TickResult> {
    let mut job = store
        .get_dataset_job(job_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("job {job_id}")))?;

    if job.paused {
        return Ok(TickResult {
            job,
            advanced: false,
            done: false,
            message: "Gepauzeerd".into(),
        });
    }
    if matches!(job.status.as_str(), "completed" | "failed") || job.phase == "done" {
        return Ok(TickResult {
            job,
            advanced: false,
            done: true,
            message: "Klaar".into(),
        });
    }
    if job.phase == "await_approval" {
        return Ok(TickResult {
            job,
            advanced: false,
            done: false,
            message: "Wacht op human gate (approve recipe)".into(),
        });
    }

    let ticks_used = job
        .budget
        .get("ticks_used")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        + 1;
    let max_ticks = job
        .budget
        .get("max_ticks")
        .and_then(|v| v.as_u64())
        .unwrap_or(cfg.datasets_max_ticks as u64);
    if let Some(obj) = job.budget.as_object_mut() {
        obj.insert("ticks_used".into(), json!(ticks_used));
    }
    if ticks_used > max_ticks {
        job.status = "failed".into();
        job.phase = "done".into();
        job.updated_at = Utc::now().to_rfc3339();
        store.update_dataset_job(&job)?;
        store.append_dataset_event(
            &job.id,
            "WAIT",
            &["Budget uitgeput · max ticks".into()],
            None,
        )?;
        return Ok(TickResult {
            job,
            advanced: false,
            done: true,
            message: "Tick budget op".into(),
        });
    }

    let ds = store
        .get_dataset(&job.dataset_id)?
        .ok_or_else(|| AtlasError::NotFound("dataset".into()))?;

    let message = match job.kind.as_str() {
        "sample" => match job.phase.as_str() {
            "sampling" => {
                // Ensure acquired
                if ds.status != "acquired" && ds.root_hash.is_empty() {
                    let _ = acquire_dataset(store, cfg, &ds.id, &cfg.secrets_key)?;
                }
                store.append_dataset_event(
                    &job.id,
                    "EXEC",
                    &["Schema-agent: features + stratified sample".into()],
                    None,
                )?;
                let (recipe, report) = propose_recipe(store, &store.get_dataset(&ds.id)?.unwrap())?;
                let ver = verify_recipe(&recipe)?;
                store.append_dataset_event(
                    &job.id,
                    "EXEC",
                    &[
                        format!("Recipe draft {}", recipe.id),
                        format!("verify={}", ver.get("ok").unwrap_or(&json!(false))),
                    ],
                    Some(&report.to_string()),
                )?;

                // artifact snapshot
                let raw = serde_json::to_vec_pretty(&recipe.body)?;
                let (hash, _, _) = store.put_object(&store.data_dir, &raw)?;
                let _ = store.insert_artifact(&ArtifactRecord {
                    id: Uuid::new_v4().to_string(),
                    project_id: ds.project_id.clone(),
                    conversation_id: None,
                    task_id: job.task_id.clone(),
                    kind: "recipe.draft".into(),
                    title: format!("recipe · {}", ds.hub_id),
                    object_hash: Some(hash),
                    content: None,
                    meta: json!({ "recipe_id": recipe.id, "dataset_id": ds.id }),
                    created_at: Utc::now().to_rfc3339(),
                });

                job.recipe_id = Some(recipe.id.clone());
                job.metrics = json!({ "sample_report": report, "verify": ver });
                job.phase = "await_approval".into();
                job.status = "blocked".into();
                job.updated_at = Utc::now().to_rfc3339();
                store.update_dataset_job(&job)?;
                store.append_dataset_event(
                    &job.id,
                    "WAIT",
                    &["Human gate: keur recept goed vóór full run".into()],
                    Some(&recipe.id),
                )?;
                let _ = store.enqueue_outbox(
                    "dataset.await_approval",
                    &json!({ "job_id": job.id, "recipe_id": recipe.id }),
                );
                if let Some(tid) = &job.task_id {
                    let _ = store.update_task_status(
                        tid,
                        "blocked",
                        Some(&json!({ "phase": "await_approval", "recipe_id": recipe.id })),
                    );
                }
                // cost stub for sample
                let _ = store.insert_cost(&CostEntry {
                    id: Uuid::new_v4().to_string(),
                    trace_id: job.trace_id.clone(),
                    conversation_id: None,
                    model_id: Some("schema-agent-stub".into()),
                    token_in: 256,
                    token_out: 128,
                    latency_ms: 5,
                    cost_usd: 0.0,
                    created_at: Utc::now().to_rfc3339(),
                });
                "Sample + recipe draft klaar — wacht op approve".into()
            }
            _ => {
                job.status = "completed".into();
                job.phase = "done".into();
                job.updated_at = Utc::now().to_rfc3339();
                store.update_dataset_job(&job)?;
                "Sample job afgerond".into()
            }
        },
        "full" => {
            let rid = job
                .recipe_id
                .clone()
                .ok_or_else(|| AtlasError::BadRequest("full job missing recipe_id".into()))?;
            let recipe = store
                .get_recipe(&rid)?
                .ok_or_else(|| AtlasError::NotFound(format!("recipe {rid}")))?;
            if recipe.status != "approved" {
                return Err(AtlasError::BadRequest(
                    "Recept niet goedgekeurd — human gate verplicht.".into(),
                ));
            }

            match job.phase.as_str() {
                "running" => {
                    store.append_dataset_event(
                        &job.id,
                        "EXEC",
                        &[
                            "Pipeline: normalize → lang → exact dedupe → MinHash → enrich → quality → decontam"
                                .into(),
                        ],
                        None,
                    )?;
                    let result = run_pipeline_pass(store, &ds, &recipe, false)?;
                    job.metrics = result.metrics.clone();
                    job.checkpoint = json!({ "rows_out": result.rows_out, "cache_hits": result.cache_hits });
                    job.phase = "datamap".into();
                    job.updated_at = Utc::now().to_rfc3339();
                    store.update_dataset_job(&job)?;
                    store.append_dataset_event(
                        &job.id,
                        "EXEC",
                        &[format!(
                            "rows_out={} cache_hits={}",
                            result.rows_out, result.cache_hits
                        )],
                        Some(&result.metrics.to_string()),
                    )?;
                    let _ = store.insert_cost(&CostEntry {
                        id: Uuid::new_v4().to_string(),
                        trace_id: job.trace_id.clone(),
                        conversation_id: None,
                        model_id: Some("enrich-ladder-stub".into()),
                        token_in: result.rows_out as i64 * 40,
                        token_out: result.rows_out as i64 * 8,
                        latency_ms: 20,
                        cost_usd: 0.0,
                        created_at: Utc::now().to_rfc3339(),
                    });
                    if let Some(tid) = &job.task_id {
                        let _ = store.add_checkpoint(
                            tid,
                            "pipeline",
                            &json!({ "metrics": result.metrics }),
                        );
                    }
                    format!("Pipeline pass · {} rows", result.rows_out)
                }
                "datamap" => {
                    let map = build_datamap(store, &ds.id)?;
                    let raw = serde_json::to_vec_pretty(&map)?;
                    let (hash, _, _) = store.put_object(&store.data_dir, &raw)?;
                    let _ = store.insert_artifact(&ArtifactRecord {
                        id: Uuid::new_v4().to_string(),
                        project_id: ds.project_id.clone(),
                        conversation_id: None,
                        task_id: job.task_id.clone(),
                        kind: "datamap".into(),
                        title: format!("datamap · {}", ds.hub_id),
                        object_hash: Some(hash.clone()),
                        content: None,
                        meta: json!({ "dataset_id": ds.id }),
                        created_at: Utc::now().to_rfc3339(),
                    });
                    job.metrics
                        .as_object_mut()
                        .map(|m| m.insert("datamap_hash".into(), json!(hash)));
                    job.status = "completed".into();
                    job.phase = "done".into();
                    job.updated_at = Utc::now().to_rfc3339();
                    store.update_dataset_job(&job)?;
                    store.append_dataset_event(
                        &job.id,
                        "DONE",
                        &["Full run voltooid · datakaart geschreven".into()],
                        Some(&hash),
                    )?;
                    if let Some(tid) = &job.task_id {
                        let _ = store.update_task_status(tid, "completed", None);
                    }
                    let _ = store.enqueue_outbox(
                        "dataset.job_completed",
                        &json!({ "job_id": job.id, "dataset_id": ds.id }),
                    );
                    "Datakaart klaar · job completed".into()
                }
                _ => {
                    job.status = "completed".into();
                    job.phase = "done".into();
                    job.updated_at = Utc::now().to_rfc3339();
                    store.update_dataset_job(&job)?;
                    "Full job afgerond".into()
                }
            }
        }
        _ => {
            return Err(AtlasError::BadRequest(format!(
                "unknown job kind {}",
                job.kind
            )))
        }
    };

    let done = job.phase == "done" || matches!(job.status.as_str(), "completed" | "failed");
    let advanced = true;
    Ok(TickResult {
        job,
        advanced,
        done,
        message,
    })
}

pub fn pin_and_optionally_acquire(
    store: &Store,
    cfg: &BrainConfig,
    source: &str,
    hub_id: &str,
    revision: Option<&str>,
    project_id: Option<String>,
    acquire: bool,
) -> AtlasResult<Value> {
    let mut ds = create_pinned_dataset(store, cfg, source, hub_id, revision, project_id)?;
    if acquire {
        ds = acquire_dataset(store, cfg, &ds.id, &cfg.secrets_key)?;
    }
    let shards = store.list_shards(&ds.id)?;
    Ok(json!({ "dataset": ds, "shards": shards }))
}

pub fn create_mix_def(
    store: &Store,
    name: &str,
    definition: Value,
) -> AtlasResult<DatasetMix> {
    let now = Utc::now().to_rfc3339();
    let mix = DatasetMix {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        definition,
        status: "draft".into(),
        export_manifest: json!({}),
        created_at: now.clone(),
        updated_at: now,
    };
    store.upsert_mix(&mix)?;
    Ok(mix)
}

pub fn compile_mix_job(store: &Store, mix_id: &str) -> AtlasResult<Value> {
    compile_mix(store, mix_id)
}

fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}
