use crate::config::BrainConfig;
use crate::memory::types::MemoryNode;
use chrono::{DateTime, Utc};

/// On-read effective score modifier — no background sweeper.
pub fn effective_salience(node: &MemoryNode, cfg: &BrainConfig, now: DateTime<Utc>) -> f64 {
    if node.archived_at.is_some() {
        return 0.0;
    }
    let age_hours = age_hours(&node.provenance.created_at, now);
    let decay_factor = (-cfg.memory_decay_lambda * age_hours).exp();
    let base = (node.salience * node.decay * decay_factor).clamp(0.0, 8.0);
    base * node.provenance.confidence.clamp(0.05, 1.0)
}

pub fn should_skip(node: &MemoryNode, cfg: &BrainConfig, now: DateTime<Utc>) -> bool {
    if node.archived_at.is_some() {
        return true;
    }
    effective_salience(node, cfg, now) < cfg.memory_min_salience
}

fn age_hours(created_at: &str, now: DateTime<Utc>) -> f64 {
    DateTime::parse_from_rfc3339(created_at)
        .ok()
        .map(|dt| {
            let created = dt.with_timezone(&Utc);
            (now - created).num_seconds().max(0) as f64 / 3600.0
        })
        .unwrap_or(0.0)
}

/// Bump salience after a node was useful in residual (write-back, still request-driven).
pub fn bump_salience(current: f64) -> f64 {
    (current * 1.08 + 0.05).min(5.0)
}

pub fn soften_salience(current: f64) -> f64 {
    (current * 0.97).max(0.05)
}
