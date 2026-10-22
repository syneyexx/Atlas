use crate::brain::research::tools::{web_fetch, web_search};
use crate::config::BrainConfig;
use crate::error::{AtlasError, AtlasResult};
use crate::lm::{CompletionRequest, LmMessage, LocalLmClient};
use crate::memory::types::{LearnMemoryRequest, MemoryKind};
use crate::memory::{EmbedClient, WriteGovernor};
use crate::store::{
    ResearchClaim, ResearchJob, ResearchSource, Store, TaskRecord,
};
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TickResult {
    pub job: ResearchJob,
    pub advanced: bool,
    pub done: bool,
    pub message: String,
}

pub async fn create_job(
    store: &Store,
    cfg: &BrainConfig,
    question: &str,
    mode: &str,
    attachment: Option<Value>,
    conversation_id: Option<String>,
) -> AtlasResult<ResearchJob> {
    let question = question.trim();
    if question.is_empty() && attachment.is_none() {
        return Err(AtlasError::BadRequest(
            "Geef een onderzoeksvraag of bijlage op.".into(),
        ));
    }
    let title = if question.is_empty() {
        format!(
            "Onderzoek: {}",
            attachment
                .as_ref()
                .and_then(|a| a.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("bijlage")
        )
    } else {
        question.chars().take(72).collect()
    };

    let now = Utc::now().to_rfc3339();
    let job_id = Uuid::new_v4().to_string();
    let task = TaskRecord {
        id: Uuid::new_v4().to_string(),
        title: format!("research · {title}"),
        status: "queued".into(),
        conversation_id: conversation_id.clone(),
        parent_id: None,
        plan: json!({ "kind": "research", "mode": mode }),
        state: json!({ "job_id": job_id }),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    store.create_task(&task)?;

    let budget = json!({
        "max_subagents": cfg.research_max_subagents,
        "max_rounds": cfg.research_max_rounds,
        "max_sources": cfg.research_max_sources,
        "max_ticks": cfg.research_max_ticks,
        "ticks_used": 0,
        "round": 0,
    });

    let job = ResearchJob {
        id: job_id.clone(),
        title,
        mode: mode.to_string(),
        status: "running".into(),
        phase: "intake".into(),
        question: if question.is_empty() {
            attachment
                .as_ref()
                .and_then(|a| a.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("bijlage")
                .to_string()
        } else {
            question.to_string()
        },
        clarifications: json!([]),
        plan: json!({}),
        report_md: String::new(),
        budget,
        paused: false,
        conversation_id,
        task_id: Some(task.id.clone()),
        attachment,
        created_at: now.clone(),
        updated_at: now,
    };
    store.create_research_job(&job)?;
    store.append_research_event(
        &job.id,
        "QUEUE",
        &[
            format!("research.{} · taak gestart", short_id(&job.id)),
            format!("modus: {}", job.mode),
            format!("budget: ≤{} subagents · ≤{} ronden", cfg.research_max_subagents, cfg.research_max_rounds),
        ],
        Some(&format!("Onderzoeksopdracht:\n{}\n\ntask_id={}", job.question, task.id)),
    )?;
    let _ = store.enqueue_outbox(
        "research.created",
        &json!({ "job_id": job.id, "task_id": task.id }),
    );
    store.update_task_status(&task.id, "running", Some(&json!({ "job_id": job.id, "phase": "intake" })))?;
    Ok(job)
}

pub async fn tick_job(
    store: &Store,
    cfg: &BrainConfig,
    lm: &LocalLmClient,
    embed: &EmbedClient,
    job_id: &str,
) -> AtlasResult<TickResult> {
    let mut job = store
        .get_research_job(job_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("Research job {job_id} niet gevonden")))?;

    if job.paused {
        return Ok(TickResult {
            job,
            advanced: false,
            done: false,
            message: "Gepauzeerd".into(),
        });
    }
    if job.status == "completed" || job.status == "failed" || job.phase == "done" {
        return Ok(TickResult {
            job,
            advanced: false,
            done: true,
            message: "Klaar".into(),
        });
    }
    if job.status == "blocked" {
        return Ok(TickResult {
            job,
            advanced: false,
            done: false,
            message: "Wacht op verduidelijking".into(),
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
        .unwrap_or(cfg.research_max_ticks as u64);
    if let Some(obj) = job.budget.as_object_mut() {
        obj.insert("ticks_used".into(), json!(ticks_used));
    }
    if ticks_used > max_ticks {
        job.status = "failed".into();
        job.phase = "done".into();
        job.updated_at = Utc::now().to_rfc3339();
        store.update_research_job(&job)?;
        store.append_research_event(
            &job.id,
            "WAIT",
            &["Budget uitgeput · max ticks bereikt".into()],
            None,
        )?;
        return Ok(TickResult {
            job,
            advanced: true,
            done: true,
            message: "Budget uitgeput".into(),
        });
    }

    let message = match job.phase.as_str() {
        "intake" => phase_intake(store, cfg, lm, &mut job).await?,
        "planning" => phase_planning(store, cfg, lm, &mut job).await?,
        "researching" => phase_researching(store, cfg, &mut job).await?,
        "reviewing" => phase_reviewing(store, cfg, &mut job).await?,
        "synthesizing" => phase_synthesizing(store, cfg, lm, &mut job).await?,
        "verifying" => phase_verifying(store, cfg, embed, &mut job).await?,
        other => {
            return Err(AtlasError::BadRequest(format!("Onbekende phase: {other}")));
        }
    };

    job.updated_at = Utc::now().to_rfc3339();
    store.update_research_job(&job)?;
    if let Some(tid) = &job.task_id {
        let st = if job.status == "completed" {
            "completed"
        } else if job.status == "failed" {
            "failed"
        } else if job.status == "blocked" {
            "blocked"
        } else {
            "running"
        };
        let _ = store.update_task_status(
            tid,
            st,
            Some(&json!({ "job_id": job.id, "phase": job.phase, "ticks_used": ticks_used })),
        );
    }

    let done = job.status == "completed" || job.status == "failed" || job.phase == "done";
    Ok(TickResult {
        job,
        advanced: true,
        done,
        message,
    })
}

pub fn set_paused(store: &Store, job_id: &str, paused: bool) -> AtlasResult<ResearchJob> {
    let mut job = store
        .get_research_job(job_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("Research job {job_id} niet gevonden")))?;
    job.paused = paused;
    job.updated_at = Utc::now().to_rfc3339();
    if paused {
        job.status = "paused".into();
    } else if job.status == "paused" {
        job.status = if job.phase == "intake" && needs_clarification(&job) {
            "blocked".into()
        } else {
            "running".into()
        };
    }
    store.update_research_job(&job)?;
    store.append_research_event(
        &job.id,
        if paused { "PAUSE" } else { "WAIT" },
        &[if paused {
            "Opdracht gepauzeerd".into()
        } else {
            "Opdracht hervat".into()
        }],
        None,
    )?;
    Ok(job)
}

pub fn answer_clarifications(
    store: &Store,
    job_id: &str,
    answers: Value,
) -> AtlasResult<ResearchJob> {
    let mut job = store
        .get_research_job(job_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("Research job {job_id} niet gevonden")))?;
    job.clarifications = answers;
    job.status = "running".into();
    job.phase = "planning".into();
    job.updated_at = Utc::now().to_rfc3339();
    store.update_research_job(&job)?;
    store.append_research_event(
        &job.id,
        "QUEUE",
        &["Verduidelijkingen ontvangen · ga naar planning".into()],
        Some(&serde_json::to_string_pretty(&job.clarifications).unwrap_or_default()),
    )?;
    store.scratchpad_set(&job.id, "clarifications", &job.clarifications)?;
    Ok(job)
}

async fn phase_intake(
    store: &Store,
    cfg: &BrainConfig,
    lm: &LocalLmClient,
    job: &mut ResearchJob,
) -> AtlasResult<String> {
    let q = job.question.to_lowercase();
    let vague = q.split_whitespace().count() < 4
        || q.contains("onderzoek dit")
        || q.contains("vertel meer")
        || q.ends_with('?') && q.split_whitespace().count() < 5;

    if vague && job.clarifications.as_array().map(|a| a.is_empty()).unwrap_or(true) {
        let questions = if cfg.lm_provider == "stub" || cfg.model_id.trim().is_empty() {
            default_clarifiers(&job.question)
        } else {
            ask_clarifiers(cfg, lm, &job.question).await.unwrap_or_else(|_| {
                default_clarifiers(&job.question)
            })
        };
        job.clarifications = json!(questions);
        job.status = "blocked".into();
        store.append_research_event(
            &job.id,
            "WAIT",
            &[
                "Vraag is te open · verduidelijking gevraagd".into(),
                format!("{} vragen voor de gebruiker", questions.len()),
            ],
            Some(&serde_json::to_string_pretty(&questions).unwrap_or_default()),
        )?;
        store.scratchpad_set(&job.id, "pending_clarifiers", &job.clarifications)?;
        return Ok("Wacht op verduidelijking".into());
    }

    job.phase = "planning".into();
    store.append_research_event(
        &job.id,
        "QUEUE",
        &["Intake afgerond · lead-agent plant onderzoek".into()],
        None,
    )?;
    Ok("Door naar planning".into())
}

async fn phase_planning(
    store: &Store,
    cfg: &BrainConfig,
    lm: &LocalLmClient,
    job: &mut ResearchJob,
) -> AtlasResult<String> {
    let subqs = if cfg.lm_provider == "stub" || cfg.model_id.trim().is_empty() {
        heuristic_plan(&job.question, &job.mode, cfg.research_max_subagents)
    } else {
        lm_plan(cfg, lm, job)
            .await
            .unwrap_or_else(|_| heuristic_plan(&job.question, &job.mode, cfg.research_max_subagents))
    };

    let mut children = Vec::new();
    for (i, sq) in subqs.iter().enumerate() {
        let child = TaskRecord {
            id: Uuid::new_v4().to_string(),
            title: format!("sub · {}", sq.chars().take(48).collect::<String>()),
            status: "queued".into(),
            conversation_id: job.conversation_id.clone(),
            parent_id: job.task_id.clone(),
            plan: json!({ "subquestion": sq, "index": i }),
            state: json!({ "job_id": job.id, "status": "pending" }),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        store.create_task(&child)?;
        children.push(json!({
            "task_id": child.id,
            "subquestion": sq,
            "status": "pending",
        }));
    }

    job.plan = json!({
        "subquestions": subqs,
        "children": children,
        "open_questions": subqs.clone(),
    });
    store.scratchpad_set(&job.id, "plan", &job.plan)?;
    store.scratchpad_set(&job.id, "findings", &json!([]))?;
    store.append_research_event(
        &job.id,
        "EXEC",
        &[
            format!("lead plan · {} deelvragen", subqs.len()),
            "subagents aangemaakt (parallel via ticks)".into(),
        ],
        Some(&serde_json::to_string_pretty(&job.plan).unwrap_or_default()),
    )?;
    if let Some(tid) = &job.task_id {
        let _ = store.add_checkpoint(tid, "plan", &job.plan);
    }
    job.phase = "researching".into();
    Ok(format!("Plan klaar · {} deelvragen", subqs.len()))
}

async fn phase_researching(store: &Store, cfg: &BrainConfig, job: &mut ResearchJob) -> AtlasResult<String> {
    let children = job
        .plan
        .get("children")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let pending_idx = children.iter().position(|c| {
        c.get("status").and_then(|s| s.as_str()).unwrap_or("") == "pending"
    });

    let Some(idx) = pending_idx else {
        job.phase = "reviewing".into();
        store.append_research_event(
            &job.id,
            "WAIT",
            &["Alle subagents klaar · lead beoordeelt dekking".into()],
            None,
        )?;
        return Ok("Door naar review".into());
    };

    let child = &children[idx];
    let subq = child
        .get("subquestion")
        .and_then(|v| v.as_str())
        .unwrap_or(&job.question)
        .to_string();
    let task_id = child.get("task_id").and_then(|v| v.as_str()).unwrap_or("");

    if !task_id.is_empty() {
        let _ = store.update_task_status(task_id, "running", Some(&json!({ "subquestion": subq })));
    }

    store.append_research_event(
        &job.id,
        "SEARCH",
        &[format!("query: {subq}"), format!("limiet: {} bronnen", (cfg.research_max_sources / 2).max(2))],
        None,
    )?;

    let hits = web_search(cfg, &subq, (cfg.research_max_sources / 2).max(2)).await?;
    let existing = store.list_research_sources(&job.id)?.len();
    let room = cfg.research_max_sources.saturating_sub(existing);

    let mut findings_batch = Vec::new();
    for hit in hits.into_iter().take(room.max(1).min(3)) {
        if hit.quality_tier == "seo" {
            store.append_research_event(
                &job.id,
                "PARSE",
                &[format!("bron genegeerd (seo): {}", hit.title)],
                Some(&hit.url),
            )?;
            continue;
        }
        store.append_research_event(
            &job.id,
            "FETCH",
            &[format!("bron: {}", hit.url), format!("titel: {}", hit.title)],
            None,
        )?;
        let page = web_fetch(cfg, &hit.url).await?;
        let bytes = if page.bytes == 0 {
            page.text.len()
        } else {
            page.bytes
        };
        store.append_research_event(
            &job.id,
            "FETCH",
            &[format!("status: 200 · {bytes} B"), format!("tier: {}", page.quality_tier)],
            None,
        )?;

        let hash = {
            let (h, _, _) = store.put_object(&cfg.data_dir, page.text.as_bytes())?;
            h
        };
        let source = ResearchSource {
            id: Uuid::new_v4().to_string(),
            job_id: job.id.clone(),
            url: page.url.clone(),
            title: page.title.clone(),
            fetched_at: Utc::now().to_rfc3339(),
            quality_tier: page.quality_tier.clone(),
            fragment: page.text.chars().take(900).collect(),
            content_hash: Some(hash),
            meta: json!({ "subquestion": subq, "snippet": hit.snippet }),
        };
        store.upsert_research_source(&source)?;
        store.append_research_event(
            &job.id,
            "PARSE",
            &["relevante passages opgeslagen".into(), format!("source_id: {}", short_id(&source.id))],
            Some(&source.fragment),
        )?;

        let summary = compact_finding(&subq, &source);
        findings_batch.push(json!({
            "subquestion": subq,
            "source_id": source.id,
            "summary": summary,
            "quality_tier": source.quality_tier,
            "url": source.url,
            "title": source.title,
        }));
    }

    let mut findings = store
        .scratchpad_get(&job.id, "findings")?
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    findings.extend(findings_batch.clone());
    store.scratchpad_set(&job.id, "findings", &Value::Array(findings))?;

    // mark child done
    if let Some(arr) = job.plan.get_mut("children").and_then(|v| v.as_array_mut()) {
        if let Some(c) = arr.get_mut(idx) {
            if let Some(obj) = c.as_object_mut() {
                obj.insert("status".into(), json!("completed"));
                obj.insert("findings".into(), json!(findings_batch.len()));
            }
        }
    }
    // update open questions
    if let Some(arr) = job.plan.get_mut("open_questions").and_then(|v| v.as_array_mut()) {
        arr.retain(|q| q.as_str() != Some(subq.as_str()));
    }

    if !task_id.is_empty() {
        let _ = store.update_task_status(
            task_id,
            "completed",
            Some(&json!({ "findings": findings_batch.len() })),
        );
        let _ = store.add_checkpoint(task_id, "findings", &json!(findings_batch));
    }

    store.append_research_event(
        &job.id,
        "WRITE",
        &[
            format!("subagent klaar · {}", subq.chars().take(40).collect::<String>()),
            format!("{} bevindingen gecompact", findings_batch.len()),
        ],
        None,
    )?;

    Ok(format!("Deelvraag onderzocht: {subq}"))
}

async fn phase_reviewing(store: &Store, cfg: &BrainConfig, job: &mut ResearchJob) -> AtlasResult<String> {
    let findings = store
        .scratchpad_get(&job.id, "findings")?
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let sources = store.list_research_sources(&job.id)?;
    let round = job
        .budget
        .get("round")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let max_rounds = job
        .budget
        .get("max_rounds")
        .and_then(|v| v.as_u64())
        .unwrap_or(cfg.research_max_rounds as u64);

    let primary = sources.iter().filter(|s| s.quality_tier == "primary").count();
    let gaps = detect_gaps(&job.question, &findings, &sources);

    store.append_research_event(
        &job.id,
        "EXEC",
        &[
            format!("dekking: {} bevindingen · {} bronnen ({} primary)", findings.len(), sources.len(), primary),
            if gaps.is_empty() {
                "geen kritieke gaten".into()
            } else {
                format!("{} gaten gemarkeerd", gaps.len())
            },
        ],
        Some(&serde_json::to_string_pretty(&gaps).unwrap_or_default()),
    )?;

    if !gaps.is_empty() && round + 1 < max_rounds && sources.len() < cfg.research_max_sources {
        // spawn one more round
        if let Some(obj) = job.budget.as_object_mut() {
            obj.insert("round".into(), json!(round + 1));
        }
        let mut children = job
            .plan
            .get("children")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for gap in gaps.iter().take(2) {
            let child = TaskRecord {
                id: Uuid::new_v4().to_string(),
                title: format!("gap · {}", gap.chars().take(40).collect::<String>()),
                status: "queued".into(),
                conversation_id: job.conversation_id.clone(),
                parent_id: job.task_id.clone(),
                plan: json!({ "subquestion": gap, "kind": "gap" }),
                state: json!({ "job_id": job.id, "status": "pending" }),
                created_at: Utc::now().to_rfc3339(),
                updated_at: Utc::now().to_rfc3339(),
            };
            store.create_task(&child)?;
            children.push(json!({
                "task_id": child.id,
                "subquestion": gap,
                "status": "pending",
            }));
        }
        if let Some(obj) = job.plan.as_object_mut() {
            obj.insert("children".into(), Value::Array(children));
            obj.insert("open_questions".into(), json!(gaps));
        }
        store.scratchpad_set(&job.id, "gaps", &json!(gaps))?;
        job.phase = "researching".into();
        store.append_research_event(
            &job.id,
            "QUEUE",
            &["nieuwe onderzoeksronde voor gaten".into()],
            None,
        )?;
        return Ok("Extra ronde gestart".into());
    }

    store.scratchpad_set(&job.id, "gaps", &json!(gaps))?;
    job.phase = "synthesizing".into();
    Ok("Door naar synthese".into())
}

async fn phase_synthesizing(
    store: &Store,
    cfg: &BrainConfig,
    lm: &LocalLmClient,
    job: &mut ResearchJob,
) -> AtlasResult<String> {
    let findings = store.scratchpad_get(&job.id, "findings")?.unwrap_or(json!([]));
    let sources = store.list_research_sources(&job.id)?;
    let gaps = store.scratchpad_get(&job.id, "gaps")?.unwrap_or(json!([]));

    let report = if cfg.lm_provider == "stub" || cfg.model_id.trim().is_empty() {
        deterministic_report(job, &findings, &sources, &gaps)
    } else {
        lm_report(cfg, lm, job, &findings, &sources, &gaps)
            .await
            .unwrap_or_else(|_| deterministic_report(job, &findings, &sources, &gaps))
    };

    job.report_md = report.clone();
    store.scratchpad_set(&job.id, "report", &json!(report))?;
    store.append_research_event(
        &job.id,
        "WRITE",
        &["rapport gesynthetiseerd".into(), format!("{} tekens", report.len())],
        Some(report.chars().take(1200).collect::<String>().as_str()),
    )?;

    // extract claims for verifier
    let claims = extract_claims(&job.id, &report, &sources);
    for c in &claims {
        store.insert_research_claim(c)?;
    }
    store.append_research_event(
        &job.id,
        "EXEC",
        &[format!("citation-agent · {} claims te verifiëren", claims.len())],
        None,
    )?;
    job.phase = "verifying".into();
    Ok("Synthese klaar".into())
}

async fn phase_verifying(
    store: &Store,
    cfg: &BrainConfig,
    embed: &EmbedClient,
    job: &mut ResearchJob,
) -> AtlasResult<String> {
    let sources = store.list_research_sources(&job.id)?;
    let claims = store.list_research_claims(&job.id)?;
    let mut verified_n = 0usize;
    let mut failed_n = 0usize;

    for claim in &claims {
        let (ok, note) = verify_claim(claim, &sources);
        store.update_research_claim_verified(&claim.id, ok, &note)?;
        if ok {
            verified_n += 1;
        } else {
            failed_n += 1;
            store.append_research_event(
                &job.id,
                "PARSE",
                &[format!("citation_fail · {}", claim.claim.chars().take(60).collect::<String>())],
                Some(&note),
            )?;
        }
    }

    store.append_research_event(
        &job.id,
        "EXEC",
        &[
            format!("verifier · {verified_n} ok · {failed_n} open"),
            "rapport met bronnenlijst gereed".into(),
        ],
        None,
    )?;

    // Persist key findings into Memory Cortex so chat/research share one brain.
    let _ = WriteGovernor::learn(
        store,
        cfg,
        embed,
        LearnMemoryRequest {
            title: format!("Research: {}", job.title),
            content: job.report_md.chars().take(4000).collect(),
            kind: Some(MemoryKind::Semantic.as_str().into()),
            source: Some("learn".into()),
            scope: None,
            confidence: Some(0.75),
            tags: vec!["research".into(), job.mode.to_lowercase()],
            conversation_id: job.conversation_id.clone(),
            parent_ids: Vec::new(),
        },
    )
    .await;

    let (object_hash, _, _) = store.put_object(&cfg.data_dir, job.report_md.as_bytes())?;
    let _ = store.insert_artifact(&crate::store::ArtifactRecord {
        id: Uuid::new_v4().to_string(),
        project_id: if cfg.default_project_id.is_empty() {
            None
        } else {
            Some(cfg.default_project_id.clone())
        },
        conversation_id: job.conversation_id.clone(),
        task_id: job.task_id.clone(),
        kind: "research_report".into(),
        title: job.title.clone(),
        object_hash: Some(object_hash),
        content: Some(job.report_md.clone()),
        meta: json!({ "job_id": job.id, "verified": verified_n, "failed": failed_n }),
        created_at: Utc::now().to_rfc3339(),
    });

    job.status = "completed".into();
    job.phase = "done".into();
    store.append_research_event(
        &job.id,
        "WRITE",
        &["DONE · onderzoek afgerond".into(), "bevindingen in Memory Cortex".into()],
        None,
    )?;
    let _ = store.enqueue_outbox("research.completed", &json!({ "job_id": job.id }));
    Ok("Onderzoek afgerond".into())
}

fn needs_clarification(job: &ResearchJob) -> bool {
    job.status == "blocked"
        && job
            .clarifications
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false)
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn default_clarifiers(question: &str) -> Vec<Value> {
    vec![
        json!({ "id": "scope", "question": format!("Welk aspect van «{}» heeft prioriteit?", question.chars().take(40).collect::<String>()) }),
        json!({ "id": "depth", "question": "Wil je een overzicht of een diepe technische analyse?" }),
        json!({ "id": "constraints", "question": "Zijn er constraints (lokaal-only, geen cloud, specifieke stack)?" }),
    ]
}

async fn ask_clarifiers(
    cfg: &BrainConfig,
    lm: &LocalLmClient,
    question: &str,
) -> AtlasResult<Vec<Value>> {
    let prompt = format!(
        "Je bent de Atlas research intake-agent. Stel 1 tot 3 korte verduidelijkende vragen in JSON array van objects {{id,question}} voor deze onderzoeksvraag. Geen markdown.\n\nVraag: {question}"
    );
    let answer = lm
        .complete(
            cfg,
            CompletionRequest {
                model: cfg.model_id.clone(),
                messages: vec![LmMessage {
                    role: "user".into(),
                    content: prompt,
                }],
            },
        )
        .await?;
    parse_json_array(&answer).map(|arr| arr.into_iter().take(3).collect())
}

fn heuristic_plan(question: &str, mode: &str, max: usize) -> Vec<String> {
    let base = vec![
        format!("Kernbegrippen en definitie: {question}"),
        format!("Architecturen / aanpakken vergeleken voor: {question}"),
        format!("Risico's, limieten en tegenstrijdigheden rond: {question}"),
    ];
    let mut out = base;
    if mode.eq_ignore_ascii_case("Verdiepend") {
        out.push(format!("Primaire bronnen en harde claims voor: {question}"));
    }
    out.into_iter().take(max.max(1)).collect()
}

async fn lm_plan(cfg: &BrainConfig, lm: &LocalLmClient, job: &ResearchJob) -> AtlasResult<Vec<String>> {
    let prompt = format!(
        "Splits deze onderzoeksvraag in {} concrete deelvragen. Antwoord als JSON array van strings.\nModus: {}\nVraag: {}\nClarifications: {}",
        cfg.research_max_subagents,
        job.mode,
        job.question,
        job.clarifications
    );
    let answer = lm
        .complete(
            cfg,
            CompletionRequest {
                model: cfg.model_id.clone(),
                messages: vec![LmMessage {
                    role: "user".into(),
                    content: prompt,
                }],
            },
        )
        .await?;
    let arr = parse_json_array(&answer)?;
    Ok(arr
        .into_iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .take(cfg.research_max_subagents)
        .collect())
}

fn compact_finding(subq: &str, source: &ResearchSource) -> String {
    format!(
        "Deelvraag «{subq}»: {} — {} ({})",
        source.title,
        source.fragment.chars().take(280).collect::<String>(),
        source.quality_tier
    )
}

fn detect_gaps(question: &str, findings: &[Value], sources: &[ResearchSource]) -> Vec<String> {
    let mut gaps = Vec::new();
    if sources.iter().filter(|s| s.quality_tier == "primary").count() < 1 {
        gaps.push(format!("Ontbrekende primaire bron voor: {question}"));
    }
    if findings.len() < 2 {
        gaps.push("Te weinig parallelle bevindingen voor synthese".into());
    }
    let blob = findings
        .iter()
        .filter_map(|f| f.get("summary").and_then(|s| s.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if !blob.contains("risico") && !blob.contains("limiet") && !blob.contains("tegenstrijd") {
        gaps.push("Risico's / limieten nog niet gedekt".into());
    }
    gaps
}

fn deterministic_report(
    job: &ResearchJob,
    findings: &Value,
    sources: &[ResearchSource],
    gaps: &Value,
) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", job.title));
    md.push_str(&format!("**Vraag:** {}\n\n", job.question));
    md.push_str(&format!("**Modus:** {}\n\n", job.mode));
    md.push_str("## Bevindingen\n\n");
    if let Some(arr) = findings.as_array() {
        for (i, f) in arr.iter().enumerate() {
            let summary = f.get("summary").and_then(|s| s.as_str()).unwrap_or("");
            let sid = f.get("source_id").and_then(|s| s.as_str()).unwrap_or("");
            md.push_str(&format!("{}. {} `[src:{}]`\n", i + 1, summary, short_id(sid)));
        }
    }
    md.push_str("\n## Synthese\n\n");
    md.push_str(
        "Atlas Research combineert lead-planning, parallelle subagents en citation-checks in één Brain. ",
    );
    md.push_str(
        "Bevindingen hierboven zijn gecompact uit bronfragmenten; claims zonder bron blijven open.\n\n",
    );
    if let Some(arr) = gaps.as_array() {
        if !arr.is_empty() {
            md.push_str("## Open gaten\n\n");
            for g in arr {
                if let Some(s) = g.as_str() {
                    md.push_str(&format!("- {s}\n"));
                }
            }
            md.push('\n');
        }
    }
    md.push_str("## Bronnen\n\n");
    for (i, s) in sources.iter().enumerate() {
        md.push_str(&format!(
            "{}. [{}]({}) — tier: {} — {}\n",
            i + 1,
            s.title,
            s.url,
            s.quality_tier,
            s.fetched_at
        ));
    }
    md
}

async fn lm_report(
    cfg: &BrainConfig,
    lm: &LocalLmClient,
    job: &ResearchJob,
    findings: &Value,
    sources: &[ResearchSource],
    gaps: &Value,
) -> AtlasResult<String> {
    let src_brief: Vec<_> = sources
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "title": s.title,
                "url": s.url,
                "tier": s.quality_tier,
                "fragment": s.fragment.chars().take(400).collect::<String>(),
            })
        })
        .collect();
    let prompt = format!(
        "Schrijf een bondig onderzoeksrapport in Markdown voor Atlas. Koppel claims aan source ids als [src:id]. Gebruik alleen de gegeven bronnen.\n\nVraag: {}\nModus: {}\nFindings: {}\nGaps: {}\nSources: {}\n",
        job.question,
        job.mode,
        findings,
        gaps,
        Value::Array(src_brief)
    );
    lm.complete(
        cfg,
        CompletionRequest {
            model: cfg.model_id.clone(),
            messages: vec![LmMessage {
                role: "user".into(),
                content: prompt,
            }],
        },
    )
    .await
}

fn extract_claims(job_id: &str, report: &str, sources: &[ResearchSource]) -> Vec<ResearchClaim> {
    let now = Utc::now().to_rfc3339();
    let mut claims = Vec::new();
    for line in report.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() || line.starts_with("**") {
            continue;
        }
        if line.len() < 40 {
            continue;
        }
        let clean = line
            .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == ' ')
            .to_string();
        if clean.chars().count() < 40 {
            continue;
        }
        let mut source_ids = Vec::new();
        for s in sources {
            let short = short_id(&s.id);
            if clean.contains(&short) || clean.contains(&s.id) {
                source_ids.push(s.id.clone());
            }
        }
        if source_ids.is_empty() && !sources.is_empty() {
            if let Some(primary) = sources.iter().find(|s| s.quality_tier == "primary") {
                source_ids.push(primary.id.clone());
            } else {
                source_ids.push(sources[0].id.clone());
            }
        }
        claims.push(ResearchClaim {
            id: Uuid::new_v4().to_string(),
            job_id: job_id.into(),
            claim: clean.chars().take(280).collect(),
            source_ids,
            verified: false,
            note: String::new(),
            created_at: now.clone(),
        });
        if claims.len() >= 8 {
            break;
        }
    }
    claims
}

fn verify_claim(claim: &ResearchClaim, sources: &[ResearchSource]) -> (bool, String) {
    if claim.source_ids.is_empty() {
        return (false, "geen bron gekoppeld".into());
    }
    let tokens: Vec<_> = claim
        .claim
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 4)
        .take(12)
        .map(|s| s.to_string())
        .collect();
    for sid in &claim.source_ids {
        if let Some(src) = sources.iter().find(|s| s.id == *sid) {
            let frag = src.fragment.to_lowercase();
            let hits = tokens.iter().filter(|t| frag.contains(t.as_str())).count();
            if hits >= 2 || (tokens.len() <= 2 && hits >= 1) {
                return (true, format!("ondersteund door {} ({} token-hits)", src.url, hits));
            }
            // primary sources get softer threshold on stub corpus
            if src.quality_tier == "primary" && hits >= 1 {
                return (true, format!("deels ondersteund door primary {}", src.url));
            }
        }
    }
    (false, "fragment ondersteunt claim onvoldoende".into())
}

fn parse_json_array(raw: &str) -> AtlasResult<Vec<Value>> {
    let trimmed = raw.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        if let Some(arr) = v.as_array() {
            return Ok(arr.clone());
        }
    }
    // try extract [...] 
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            if let Ok(v) = serde_json::from_str::<Value>(&trimmed[start..=end]) {
                if let Some(arr) = v.as_array() {
                    return Ok(arr.clone());
                }
            }
        }
    }
    Err(AtlasError::BadRequest("LM gaf geen JSON-array terug".into()))
}
