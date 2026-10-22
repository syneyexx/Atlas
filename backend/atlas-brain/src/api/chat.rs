use crate::brain::run_completion;
use crate::error::{AtlasError, AtlasResult};
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

    let conversation: Vec<ChatMessage> = body
        .messages
        .into_iter()
        .map(|m| ChatMessage {
            role: m.role,
            content: m.content,
            created_at: None,
            callout: None,
        })
        .collect();

    let answer = run_completion(&cfg, &state.store, &state.lm, &conversation).await?;
    let created = Utc::now().timestamp();
    let persist = body.persist.unwrap_or(true);

    if persist {
        let id = body
            .conversation_id
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let title = body
            .conversation_title
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
            role: "assistant".into(),
            content: answer.clone(),
            created_at: Some(Utc::now().to_rfc3339()),
            callout: None,
        });
        state.store.upsert_conversation(&ConversationRecord {
            id,
            title,
            messages,
            updated_at: Utc::now().to_rfc3339(),
        })?;
    }

    Ok(Json(json!({
        "id": format!("chatcmpl-{}", Uuid::new_v4()),
        "object": "chat.completion",
        "created": created,
        "model": cfg.model_id,
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": answer },
            "finish_reason": "stop"
        }]
    })))
}
