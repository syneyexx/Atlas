use crate::error::{AtlasError, AtlasResult};
use crate::lm::{CompletionRequest, LmMessage, LocalLmClient};
use crate::memory::{recall_into_context, EmbedClient};
use crate::config::BrainConfig;
use crate::store::{ChatMessage, Store};

/// One-brain completion: Memory Cortex residual → local LM.
pub async fn run_completion(
    cfg: &BrainConfig,
    store: &Store,
    lm: &LocalLmClient,
    embed: &EmbedClient,
    conversation: &[ChatMessage],
    conversation_id: Option<&str>,
) -> AtlasResult<String> {
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
            false,
        )
        .await?;
        recalled
            .emit
            .messages
            .into_iter()
            .map(|(role, content)| LmMessage { role, content })
            .collect()
    };

    lm.complete(
        cfg,
        CompletionRequest {
            model: cfg.model_id.clone(),
            messages,
        },
    )
    .await
}
