use crate::brain::recall::{format_knowledge_block, recall_knowledge};
use crate::config::BrainConfig;
use crate::error::{AtlasError, AtlasResult};
use crate::lm::{CompletionRequest, LmMessage, LocalLmClient};
use crate::store::{ChatMessage, Store};

/// One-brain context assembly:
/// SYSTEM_PROMPT (settings) → recalled knowledge → conversation turns → local LM.
pub fn assemble_messages(
    cfg: &BrainConfig,
    store: &Store,
    conversation: &[ChatMessage],
) -> AtlasResult<Vec<LmMessage>> {
    let mut out = Vec::new();

    if !cfg.system_prompt.trim().is_empty() {
        out.push(LmMessage {
            role: "system".into(),
            content: cfg.system_prompt.clone(),
        });
    }

    if cfg.knowledge_top_k > 0 {
        let query = conversation
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or("");
        let hits = recall_knowledge(store, cfg, query, None)?;
        let block = format_knowledge_block(&hits, cfg.knowledge_max_chars);
        if !block.is_empty() {
            out.push(LmMessage {
                role: "system".into(),
                content: block,
            });
        }
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

    Ok(out)
}

pub async fn run_completion(
    cfg: &BrainConfig,
    store: &Store,
    lm: &LocalLmClient,
    conversation: &[ChatMessage],
) -> AtlasResult<String> {
    if cfg.model_id.trim().is_empty() {
        return Err(AtlasError::BadRequest(
            "Geen model_id geconfigureerd. Stel het model in via Settings.".into(),
        ));
    }
    let messages = assemble_messages(cfg, store, conversation)?;
    if !messages.iter().any(|m| m.role == "user") {
        return Err(AtlasError::BadRequest(
            "Chatvereiste: minstens één user-bericht.".into(),
        ));
    }
    lm.complete(
        cfg,
        CompletionRequest {
            model: cfg.model_id.clone(),
            messages,
        },
    )
    .await
}
