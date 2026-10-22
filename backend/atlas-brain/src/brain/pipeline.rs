use crate::config::BrainConfig;
use crate::error::{AtlasError, AtlasResult};
use crate::lm::{CompletionRequest, LmMessage, LocalLmClient};
use crate::memory::types::MemoryTrace;
use crate::memory::{recall_into_context, EmbedClient};
use crate::store::{ChatMessage, Store};
use serde_json::json;

pub struct CompletionOutcome {
    pub answer: String,
    pub trace_id: Option<String>,
    #[allow(dead_code)]
    pub recall_trace: Option<MemoryTrace>,
    pub hit_ids: Vec<String>,
}

/// One-brain completion: Memory Cortex residual → local LM (+ optional durable trace).
pub async fn run_completion(
    cfg: &BrainConfig,
    store: &Store,
    lm: &LocalLmClient,
    embed: &EmbedClient,
    conversation: &[ChatMessage],
    conversation_id: Option<&str>,
) -> AtlasResult<CompletionOutcome> {
    if cfg.model_id.trim().is_empty() {
        return Err(AtlasError::BadRequest(
            "Geen model_id geconfigureerd. Stel het model in via Settings.".into(),
        ));
    }
    if !conversation.iter().any(|m| m.role == "user") {
        return Err(AtlasError::BadRequest(
            "Chatvereiste: minstens één user-bericht.".into(),
        ));
    }

    let query = conversation
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    let mut recall_trace = None;
    let mut hit_ids = Vec::new();

    let messages = if cfg.knowledge_top_k == 0 {
        // Memory injection disabled — still honor SYSTEM_PROMPT from settings.
        let mut out = Vec::new();
        if !cfg.system_prompt.trim().is_empty() {
            out.push(LmMessage {
                role: "system".into(),
                content: cfg.system_prompt.clone(),
            });
        }
        for msg in conversation {
            if msg.role != "user" && msg.role != "assistant" && msg.role != "system" {
                continue;
            }
            let content = match &msg.callout {
                Some(c) => format!("{}\n\n{}\n{}", msg.content, c.title, c.content),
                None => msg.content.clone(),
            };
            out.push(LmMessage {
                role: msg.role.clone(),
                content,
            });
        }
        out
    } else {
        let recalled = recall_into_context(
            store,
            cfg,
            embed,
            query,
            conversation,
            conversation_id,
            true, // always build forensic trace for durable store
        )
        .await?;
        hit_ids = recalled.hits.iter().map(|h| h.id.clone()).collect();
        recall_trace = Some(recalled.emit.trace);
        recalled
            .emit
            .messages
            .into_iter()
            .map(|(role, content)| LmMessage { role, content })
            .collect()
    };

    let answer = lm
        .complete(
            cfg,
            CompletionRequest {
                model: cfg.model_id.clone(),
                messages,
            },
        )
        .await?;

    let trace_id = {
        let payload = json!({
            "query": query,
            "hit_ids": hit_ids,
            "recall": recall_trace.clone(),
            "answer_chars": answer.chars().count(),
            "model_id": cfg.model_id,
            "embed_model": crate::memory::write::embed_model_key(cfg),
        });
        match store.insert_trace("chat", conversation_id, Some(cfg.model_id.as_str()), &payload) {
            Ok(id) => Some(id),
            Err(err) => {
                tracing::warn!("trace persist failed: {err}");
                None
            }
        }
    };

    Ok(CompletionOutcome {
        answer,
        trace_id,
        recall_trace,
        hit_ids,
    })
}
