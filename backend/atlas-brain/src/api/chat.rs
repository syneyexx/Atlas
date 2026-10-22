use crate::brain::run_completion;
use crate::error::{AtlasError, AtlasResult};
use crate::memory::consolidate::{consolidate_episodic, should_auto_consolidate, ConsolidateRequest};
use crate::memory::{LearnMemoryRequest, WriteGovernor};
use crate::state::AppState;
use crate::store::{ChatMessage, ConversationRecord};
use axum::extract::State;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ChatCompletionBody {
    #[serde(default)]
    pub model: Option<String>,
    pub messages: Vec<IncomingMessage>,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub conversation_title: Option<String>,
    #[serde(default)]
    pub persist: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct IncomingMessage {
    pub role: String,
    pub content: String,
}

pub async fn chat_completions(
    State(state): State<AppState>,
    Json(body): Json<ChatCompletionBody>,
) -> AtlasResult<Json<Value>> {
    if body.messages.is_empty() {
        return Err(AtlasError::BadRequest("messages mag niet leeg zijn.".into()));
    }

    let mut cfg = state.config.read().await.clone();
    if let Some(model) = body.model.as_ref().filter(|m| !m.is_empty()) {
        // Request model overrides for this call only; does not rewrite settings.
        cfg.model_id = model.clone();
    }

    // Request-driven: mark vectors from other embed models stale (no sweeper).
    let embed_key = crate::memory::write::embed_model_key(&cfg);
    let _ = state.store.mark_embeddings_stale_except(&embed_key);

    let conversation: Vec<ChatMessage> = body
        .messages
        .into_iter()
        .map(|m| ChatMessage {
            id: Some(Uuid::new_v4().to_string()),
            role: m.role,
            content: m.content,
            created_at: None,
            callout: None,
            trace_id: None,
            model_id: None,
        })
        .collect();

    let conversation_id = body
        .conversation_id
        .clone()
        .filter(|s| !s.is_empty());
    let outcome = run_completion(
        &cfg,
        &state.store,
        &state.lm,
        &state.embed,
        &conversation,
        conversation_id.as_deref(),
    )
    .await?;
    let created = Utc::now().timestamp();
    let persist = body.persist.unwrap_or(true);

    let id = conversation_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    if persist {
        let title = body
            .conversation_title
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                conversation
                    .iter()
                    .find(|m| m.role == "user")
                    .map(|m| m.content.chars().take(32).collect::<String>())
                    .unwrap_or_else(|| "Gesprek".into())
            });
        let mut messages = conversation.clone();
        messages.push(ChatMessage {
            id: Some(Uuid::new_v4().to_string()),
            role: "assistant".into(),
            content: outcome.answer.clone(),
            created_at: Some(Utc::now().to_rfc3339()),
            callout: None,
            trace_id: outcome.trace_id.clone(),
            model_id: Some(cfg.model_id.clone()),
        });
        state.store.upsert_conversation(&ConversationRecord {
            id: id.clone(),
            title,
            messages,
            updated_at: Utc::now().to_rfc3339(),
        })?;
    }

    // Episodic write of latest user turn (request-driven).
    if cfg.memory_write_episodic_on_chat {
        if let Some(user) = conversation.iter().rev().find(|m| m.role == "user") {
            let _ = WriteGovernor::learn(
                &state.store,
                &cfg,
                &state.embed,
                LearnMemoryRequest {
                    title: format!("turn:{}", id.chars().take(8).collect::<String>()),
                    content: user.content.clone(),
                    tags: vec!["chat".into(), "episodic".into()],
                    kind: Some("episodic".into()),
                    source: Some("chat".into()),
                    scope: Some("conversation".into()),
                    confidence: Some(0.65),
                    conversation_id: Some(id.clone()),
                },
            )
            .await;
        }
    }

    // Optional on-demand consolidate when enough episodic mass exists.
    if cfg.memory_auto_consolidate {
        if should_auto_consolidate(&state.store, &id, cfg.memory_auto_consolidate_min)? {
            let _ = consolidate_episodic(
                &state.store,
                &cfg,
                &state.lm,
                &state.embed,
                ConsolidateRequest {
                    conversation_id: Some(id.clone()),
                    node_ids: None,
                    title: Some(format!("distill:{id}")),
                },
            )
            .await;
        }
    }

    // Request-driven outbox drain (no background sweeper).
    let drain = state.store.drain_outbox(32).unwrap_or_else(|_| {
        crate::store::DrainResult {
            seen: 0,
            processed: 0,
            by_type: Default::default(),
        }
    });

    Ok(Json(json!({
        "id": format!("chatcmpl-{}", Uuid::new_v4()),
        "object": "chat.completion",
        "created": created,
        "model": cfg.model_id,
        "atlas": {
            "conversation_id": id,
            "trace_id": outcome.trace_id,
            "hit_ids": outcome.hit_ids,
            "outbox_drained": drain.processed,
        },
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": outcome.answer },
            "finish_reason": "stop"
        }]
    })))
}
