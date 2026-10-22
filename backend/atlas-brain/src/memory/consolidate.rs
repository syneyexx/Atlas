use crate::config::BrainConfig;
use crate::error::{AtlasError, AtlasResult};
use crate::lm::{CompletionRequest, LmMessage, LocalLmClient};
use crate::memory::embed::EmbedClient;
use crate::memory::types::{
    LearnMemoryRequest, MemoryKind, MemoryNode, WriteAction, WriteDecision,
};
use crate::memory::WriteGovernor;
use crate::store::Store;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct ConsolidateRequest {
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub node_ids: Option<Vec<String>>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsolidateResult {
    pub source_ids: Vec<String>,
    pub decision: WriteDecision,
    pub distill_preview: String,
}

/// On-demand episode→semantic distill via local LM (or stub). No background jobs.
pub async fn consolidate_episodic(
    store: &Store,
    cfg: &BrainConfig,
    lm: &LocalLmClient,
    embed: &EmbedClient,
    req: ConsolidateRequest,
) -> AtlasResult<ConsolidateResult> {
    let mut sources: Vec<MemoryNode> = Vec::new();
    if let Some(ids) = &req.node_ids {
        for id in ids {
            if let Some(n) = store.get_memory_node(id)? {
                if n.kind == MemoryKind::Episodic || n.kind == MemoryKind::Working {
                    sources.push(n);
                }
            }
        }
    } else {
        let all = store.list_memory_nodes(Some(MemoryKind::Episodic))?;
        sources = match &req.conversation_id {
            Some(cid) => all
                .into_iter()
                .filter(|n| n.provenance.conversation_id.as_deref() == Some(cid.as_str()))
                .take(12)
                .collect(),
            None => all.into_iter().take(12).collect(),
        };
    }

    if sources.is_empty() {
        return Err(AtlasError::BadRequest(
            "Geen episodische bronnen om te consolideren.".into(),
        ));
    }

    let source_ids: Vec<String> = sources.iter().map(|s| s.id.clone()).collect();
    let corpus = sources
        .iter()
        .map(|s| format!("- [{}] {}\n{}", s.title, s.id, s.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    let distill_instruction = if cfg.consolidate_prompt.trim().is_empty() {
        "Distilleer de episodische notities tot 1-3 stabiele feiten. Geen chat-stijl. Alleen duurzame kennis."
            .to_string()
    } else {
        cfg.consolidate_prompt.clone()
    };

    let content = if cfg.lm_provider == "stub" {
        // Deterministic offline distill — no fake LM prose.
        sources
            .iter()
            .map(|s| {
                let body = s.content.lines().next().unwrap_or(s.content.as_str());
                format!("• {}: {}", s.title, body.trim())
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let distill = lm
            .complete(
                cfg,
                CompletionRequest {
                    model: cfg.model_id.clone(),
                    messages: vec![
                        LmMessage {
                            role: "system".into(),
                            content: distill_instruction,
                        },
                        LmMessage {
                            role: "user".into(),
                            content: format!(
                                "Bronmateriaal:\n{corpus}\n\nSchrijf de gedistilleerde kennis als platte tekst."
                            ),
                        },
                    ],
                },
            )
            .await?;
        clean_distill(&distill)
    };
    if content.chars().count() < 12 {
        return Err(AtlasError::BadRequest(
            "Distillate te kort — consolidatie afgebroken.".into(),
        ));
    }

    let title = req
        .title
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| {
            req.conversation_id
                .as_ref()
                .map(|c| format!("distill:{c}"))
                .unwrap_or_else(|| "distill:episodic".into())
        });

    let mut decision = WriteGovernor::learn(
        store,
        cfg,
        embed,
        LearnMemoryRequest {
            title,
            content: content.clone(),
            tags: vec!["distill".into(), "consolidated".into()],
            kind: Some("semantic".into()),
            source: Some("distill".into()),
            scope: Some("global".into()),
            confidence: Some(0.75),
            conversation_id: req.conversation_id.clone(),
            parent_ids: source_ids.clone(),
        },
    )
    .await?;

    // Attach parent provenance when inserted/merged/conflicted.
    if let Some(node) = decision.node.as_mut() {
        if matches!(
            decision.action,
            WriteAction::Inserted | WriteAction::Merged | WriteAction::Conflicted
        ) {
            node.provenance.parent_ids = source_ids.clone();
            node.updated_at = chrono::Utc::now().to_rfc3339();
            store.upsert_memory_node(node)?;
        }
    }

    Ok(ConsolidateResult {
        source_ids,
        decision,
        distill_preview: content.chars().take(280).collect(),
    })
}

fn clean_distill(raw: &str) -> String {
    let mut text = raw.trim().to_string();
    // Stub LM returns diagnostic prose — extract the "vraag" payload if present, else strip headers.
    if text.contains("Jouw vraag:") {
        if let Some(idx) = text.rfind("Jouw vraag:") {
            text = text[idx + "Jouw vraag:".len()..].trim().to_string();
        }
    }
    // If still stub-ish, fall back to a structured extract from the user payload section.
    if text.contains("Atlas Brain stub-antwoord") {
        text = raw
            .lines()
            .filter(|l| !l.contains("stub-antwoord") && !l.contains("System prompt") && !l.contains("Kennis geïnjecteerd"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
    }
    text
}

/// Heuristic: enough episodic mass for this conversation to justify an optional distill.
pub fn should_auto_consolidate(store: &Store, conversation_id: &str, min_nodes: usize) -> AtlasResult<bool> {
    let n = store
        .list_memory_nodes(Some(MemoryKind::Episodic))?
        .into_iter()
        .filter(|node| node.provenance.conversation_id.as_deref() == Some(conversation_id))
        .count();
    Ok(n >= min_nodes)
}
