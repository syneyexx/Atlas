use crate::memory::types::RouterDecision;
use crate::memory::working::WorkingMemoryState;

/// Soft MoE router: keyword prior + optional continuity from working KV, then L1-normalize.
pub fn route_experts(query: &str, prior: Option<&WorkingMemoryState>) -> RouterDecision {
    let q = query.to_lowercase();
    let mut r = RouterDecision::default();

    if q.contains("ik ")
        || q.contains("jij ")
        || q.contains("wie ben")
        || q.contains("onthoud")
        || q.contains("mijn ")
        || q.contains("profile")
    {
        r.self_profile = 0.75;
    }
    if q.contains("gisteren")
        || q.contains("eerder")
        || q.contains("vorige")
        || q.contains("gesprek")
        || q.contains("net ")
        || q.contains("zonet")
    {
        r.episodic = 0.6;
        r.working = 0.45;
    }
    if q.contains("wat is")
        || q.contains("definitie")
        || q.contains("uitleg")
        || q.contains("hoe werkt")
        || q.contains("waarom")
    {
        r.semantic = 0.7;
    }
    if q.contains("hoe ")
        || q.contains("stappen")
        || q.contains("workflow")
        || q.contains("procedure")
        || q.contains("skill")
        || q.contains("werkwijze")
        || q.contains("recipe")
        || q.contains("playbook")
    {
        r.procedural = 0.75;
    }
    if q.contains("samenvat") || q.contains("onthoud dit") || q.contains("pin") {
        r.working = 0.55;
        r.episodic = 0.4;
    }

    if let Some(prev) = prior {
        // Continuity blend — request-scoped, not a background loop.
        r.working = 0.65 * r.working + 0.35 * prev.last_router.working;
        r.episodic = 0.65 * r.episodic + 0.35 * prev.last_router.episodic;
        r.semantic = 0.65 * r.semantic + 0.35 * prev.last_router.semantic;
        r.self_profile = 0.65 * r.self_profile + 0.35 * prev.last_router.self_profile;
        r.procedural = 0.65 * r.procedural + 0.35 * prev.last_router.procedural;
        if !prev.pinned.is_empty() {
            r.working = (r.working + 0.15).min(1.0);
        }
    }

    normalize(r)
}

fn normalize(mut r: RouterDecision) -> RouterDecision {
    r.working = r.working.max(0.05);
    r.episodic = r.episodic.max(0.05);
    r.semantic = r.semantic.max(0.05);
    r.self_profile = r.self_profile.max(0.05);
    r.procedural = r.procedural.max(0.05);
    let sum = r.working + r.episodic + r.semantic + r.self_profile + r.procedural;
    if sum > 0.0 {
        r.working /= sum;
        r.episodic /= sum;
        r.semantic /= sum;
        r.self_profile /= sum;
        r.procedural /= sum;
    }
    r
}

/// Scale head weights so MoE mix actually gates capacity (not just labels).
pub fn expert_top_k(base: usize, weight: f64) -> usize {
    let scaled = ((base as f64) * weight * 2.2).round() as usize;
    scaled.clamp(1, base.saturating_mul(2).max(1))
}
