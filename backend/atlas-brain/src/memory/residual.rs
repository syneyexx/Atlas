use crate::config::BrainConfig;
use crate::memory::types::{
    AttentionHeadResult, ContextFrame, MemoryHit, MemoryKind, MemoryTrace, RouterDecision,
};

pub struct EmitResult {
    pub frames: Vec<ContextFrame>,
    pub messages: Vec<(String, String)>,
    pub trace: MemoryTrace,
}

/// Approx tokens without external tokenizer (chars/4) — documented estimate.
pub fn approx_tokens(text: &str) -> usize {
    (text.chars().count() + 3) / 4
}

pub fn emit_context(
    cfg: &BrainConfig,
    system_prompt: &str,
    router: &RouterDecision,
    heads: &[AttentionHeadResult],
    fused: &[MemoryHit],
    conversation: &[(String, String)],
) -> EmitResult {
    let budget = cfg.memory_token_budget.max(256);
    let mut frames: Vec<ContextFrame> = Vec::new();
    let mut cut = Vec::new();

    if !system_prompt.trim().is_empty() {
        push_frame(
            &mut frames,
            "system",
            "system_prompt",
            system_prompt,
            1000.0,
        );
    }

    // Self/profile first among memories.
    let mut self_hits: Vec<&MemoryHit> = fused
        .iter()
        .filter(|h| h.kind == MemoryKind::SelfProfile)
        .collect();
    self_hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for hit in self_hits.into_iter().take(3) {
        push_frame(
            &mut frames,
            "system",
            "self_profile",
            &format_hit(hit),
            900.0 + hit.score,
        );
    }

    if cfg.memory_include_router_brief {
        let brief = format!(
            "[ATLAS_ROUTER] semantic={:.2} episodic={:.2} self={:.2} working={:.2} procedural={:.2} [/ATLAS_ROUTER]",
            router.semantic, router.episodic, router.self_profile, router.working, router.procedural
        );
        push_frame(&mut frames, "system", "router", &brief, 100.0);
    }

    for hit in fused.iter().filter(|h| h.kind == MemoryKind::Semantic) {
        push_frame(
            &mut frames,
            "system",
            "semantic",
            &format_hit(hit),
            500.0 + hit.score,
        );
    }
    for hit in fused.iter().filter(|h| h.kind == MemoryKind::Procedural) {
        push_frame(
            &mut frames,
            "system",
            "procedural",
            &format_hit(hit),
            480.0 + hit.score,
        );
    }
    for hit in fused.iter().filter(|h| h.kind == MemoryKind::Episodic) {
        push_frame(
            &mut frames,
            "system",
            "episodic",
            &format_hit(hit),
            400.0 + hit.score,
        );
    }
    for hit in fused
        .iter()
        .filter(|h| h.kind == MemoryKind::Working)
    {
        push_frame(
            &mut frames,
            "system",
            "working",
            &format_hit(hit),
            350.0 + hit.score,
        );
    }

    for (role, content) in conversation {
        push_frame(
            &mut frames,
            role,
            "conversation",
            content,
            if role == "user" { 300.0 } else { 250.0 },
        );
    }

    // Token budget: drop lowest priority frames first, never drop the last user turn.
    frames.sort_by(|a, b| {
        b.priority
            .partial_cmp(&a.priority)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut kept: Vec<ContextFrame> = Vec::new();
    let mut used = 0usize;
    let last_user = conversation
        .iter()
        .rev()
        .find(|(r, _)| r == "user")
        .map(|(_, c)| c.clone());

    for frame in frames {
        let is_last_user = frame.label == "conversation"
            && frame.role == "user"
            && last_user.as_deref() == Some(frame.content.as_str());
        if used + frame.approx_tokens > budget && !is_last_user && frame.label != "system_prompt" {
            cut.push(frame.label.clone());
            continue;
        }
        used += frame.approx_tokens;
        kept.push(frame);
    }

    // Restore stable emission order: system_prompt, self, router, semantic, procedural, episodic, working, conversation.
    let order = |label: &str| match label {
        "system_prompt" => 0,
        "self_profile" => 1,
        "router" => 2,
        "semantic" => 3,
        "procedural" => 4,
        "episodic" => 5,
        "working" => 6,
        _ => 7,
    };
    kept.sort_by(|a, b| {
        order(&a.label)
            .cmp(&order(&b.label))
            .then(b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal))
    });

    // Collapse memory system frames into fewer messages where possible.
    let mut messages: Vec<(String, String)> = Vec::new();
    let mut memory_block = String::new();
    for frame in &kept {
        if frame.label == "system_prompt" {
            messages.push(("system".into(), frame.content.clone()));
        } else if frame.label == "conversation" {
            if !memory_block.is_empty() {
                messages.push((
                    "system".into(),
                    format!("[ATLAS_KNOWLEDGE]\n{memory_block}[/ATLAS_KNOWLEDGE]"),
                ));
                memory_block.clear();
            }
            messages.push((frame.role.clone(), frame.content.clone()));
        } else {
            if !memory_block.is_empty() {
                memory_block.push_str("\n\n");
            }
            memory_block.push_str(&frame.content);
        }
    }
    if !memory_block.is_empty() {
        // Insert memory block before conversation if conversation already started.
        let insert_at = messages
            .iter()
            .position(|(r, _)| r == "user" || r == "assistant")
            .unwrap_or(messages.len());
        messages.insert(
            insert_at,
            (
                "system".into(),
                format!("[ATLAS_KNOWLEDGE]\n{memory_block}[/ATLAS_KNOWLEDGE]"),
            ),
        );
    }

    let used_tokens = kept.iter().map(|f| f.approx_tokens).sum();
    EmitResult {
        frames: kept,
        messages,
        trace: MemoryTrace {
            router: router.clone(),
            heads: heads.to_vec(),
            frames: Vec::new(), // filled below for forensics
            budget_tokens: budget,
            used_tokens,
            cut_labels: cut,
        },
    }
}

fn push_frame(
    frames: &mut Vec<ContextFrame>,
    role: &str,
    label: &str,
    content: &str,
    priority: f64,
) {
    let content = content.trim();
    if content.is_empty() {
        return;
    }
    frames.push(ContextFrame {
        role: role.into(),
        label: label.into(),
        content: content.into(),
        approx_tokens: approx_tokens(content),
        priority,
    });
}

fn format_hit(hit: &MemoryHit) -> String {
    let title = if hit.title.is_empty() {
        hit.kind.as_str().to_string()
    } else {
        hit.title.clone()
    };
    format!(
        "### {title}\n{}\n(head={} score={:.3} conf={:.2})",
        hit.content.trim(),
        hit.head,
        hit.score,
        hit.confidence
    )
}

pub fn finalize_trace(mut result: EmitResult) -> EmitResult {
    result.trace.frames = result.frames.clone();
    result
}
