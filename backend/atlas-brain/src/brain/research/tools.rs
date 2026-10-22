use crate::config::BrainConfig;
use crate::error::AtlasResult;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub quality_tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedPage {
    pub url: String,
    pub title: String,
    pub text: String,
    pub quality_tier: String,
    pub bytes: usize,
}

/// Web search — stub corpus offline; optional HTTP provider later via config.
pub async fn web_search(cfg: &BrainConfig, query: &str, limit: usize) -> AtlasResult<Vec<SearchHit>> {
    let limit = limit.clamp(1, 8);
    let provider = cfg.research_search_provider.as_str();
    if provider == "http" && !cfg.research_search_url.trim().is_empty() {
        // Best-effort HTTP search adapter (OpenAI-style or custom JSON). Falls back to stub.
        if let Ok(hits) = http_search(cfg, query, limit).await {
            if !hits.is_empty() {
                return Ok(hits);
            }
        }
    }
    Ok(stub_search(query, limit))
}

pub async fn web_fetch(cfg: &BrainConfig, url: &str) -> AtlasResult<FetchedPage> {
    let provider = cfg.research_search_provider.as_str();
    if provider == "http" && (url.starts_with("http://") || url.starts_with("https://")) {
        if let Ok(page) = http_fetch(cfg, url).await {
            return Ok(page);
        }
    }
    Ok(stub_fetch(url))
}

async fn http_search(cfg: &BrainConfig, query: &str, limit: usize) -> AtlasResult<Vec<SearchHit>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let url = cfg.research_search_url.trim_end_matches('/');
    let resp = client
        .get(url)
        .query(&[("q", query), ("limit", &limit.to_string())])
        .send()
        .await?;
    if !resp.status().is_success() {
        return Ok(Vec::new());
    }
    let body: serde_json::Value = resp.json().await?;
    let mut hits = Vec::new();
    if let Some(arr) = body.get("results").and_then(|v| v.as_array()) {
        for item in arr.iter().take(limit) {
            hits.push(SearchHit {
                url: item
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .into(),
                title: item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled")
                    .into(),
                snippet: item
                    .get("snippet")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .into(),
                quality_tier: item
                    .get("quality_tier")
                    .and_then(|v| v.as_str())
                    .unwrap_or("secondary")
                    .into(),
            });
        }
    }
    Ok(hits
        .into_iter()
        .filter(|h| !h.url.is_empty())
        .collect())
}

async fn http_fetch(cfg: &BrainConfig, url: &str) -> AtlasResult<FetchedPage> {
    let _ = cfg;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .user_agent("Atlas999-Research/0.1")
        .build()?;
    let resp = client.get(url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Ok(stub_fetch(url));
    }
    let raw = resp.text().await.unwrap_or_default();
    let text = html_to_text(&raw);
    let title = extract_title(&raw).unwrap_or_else(|| url.to_string());
    let tier = quality_for_url(url);
    Ok(FetchedPage {
        url: url.into(),
        title,
        text: text.chars().take(12_000).collect(),
        quality_tier: tier.into(),
        bytes: raw.len(),
    })
}

fn stub_search(query: &str, limit: usize) -> Vec<SearchHit> {
    let q = query.to_lowercase();
    let corpus: Vec<SearchHit> = vec![
        SearchHit {
            url: "atlas://corpus/local-llm-architecture".into(),
            title: "Lokale LLM-architectuur: orchestratie en geheugen".into(),
            snippet: "Betrouwbare lokale AI combineert een request-driven brain, durable memory en tool policy.".into(),
            quality_tier: "primary".into(),
        },
        SearchHit {
            url: "atlas://corpus/tool-calling-reliability".into(),
            title: "Toolcalling betrouwbaarheid in lokale runtimes".into(),
            snippet: "Schema-validatie, ask/allow/deny policy en traces voorkomen hallucinerende tool-uitvoer.".into(),
            quality_tier: "primary".into(),
        },
        SearchHit {
            url: "atlas://corpus/memory-vs-training".into(),
            title: "Geheugen versus modeltraining".into(),
            snippet: "Episodic/semantic memory is runtime knowledge; training wijzigt gewichten — niet verwisselbaar.".into(),
            quality_tier: "primary".into(),
        },
        SearchHit {
            url: "atlas://corpus/citation-verification".into(),
            title: "Citation verification patterns".into(),
            snippet: "Elke claim koppelt aan een bronfragment; unverified claims blijven gemarkeerd.".into(),
            quality_tier: "secondary".into(),
        },
        SearchHit {
            url: "atlas://corpus/hardware-limits".into(),
            title: "Grenzen van lokale hardware".into(),
            snippet: "Contextlengte, VRAM en latency bepalen subagent-budget en parallelisatie.".into(),
            quality_tier: "secondary".into(),
        },
        SearchHit {
            url: "atlas://corpus/seo-noise".into(),
            title: "10 Best AI Tools 2026 (affiliate)".into(),
            snippet: "Koop nu de beste AI toolkit — geen primaire bron.".into(),
            quality_tier: "seo".into(),
        },
    ];
    let mut scored: Vec<(i32, SearchHit)> = corpus
        .into_iter()
        .map(|h| {
            let hay = format!("{} {} {}", h.title, h.snippet, h.url).to_lowercase();
            let mut score = 0;
            for token in q.split_whitespace() {
                if token.len() > 2 && hay.contains(token) {
                    score += 2;
                }
            }
            if h.quality_tier == "primary" {
                score += 1;
            }
            if h.quality_tier == "seo" {
                score -= 3;
            }
            (score, h)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
        .into_iter()
        .filter(|(s, _)| *s > 0)
        .take(limit)
        .map(|(_, h)| h)
        .chain(
            // Always return at least something useful for empty queries
            stub_search_fallback(limit),
        )
        .take(limit)
        .collect()
}

fn stub_search_fallback(limit: usize) -> impl Iterator<Item = SearchHit> {
    let base = vec![
        SearchHit {
            url: "atlas://corpus/local-llm-architecture".into(),
            title: "Lokale LLM-architectuur: orchestratie en geheugen".into(),
            snippet: "Request-driven brain, durable stores, tool policy.".into(),
            quality_tier: "primary".into(),
        },
        SearchHit {
            url: "atlas://corpus/citation-verification".into(),
            title: "Citation verification patterns".into(),
            snippet: "Claims moeten bronfragmenten hebben.".into(),
            quality_tier: "secondary".into(),
        },
    ];
    base.into_iter().take(limit)
}

fn stub_fetch(url: &str) -> FetchedPage {
    let now = Utc::now().to_rfc3339();
    let (title, text, tier) = match url {
        "atlas://corpus/local-llm-architecture" => (
            "Lokale LLM-architectuur: orchestratie en geheugen",
            format!(
                "Een betrouwbaar lokaal AI-framework scheidt UI van Brain. De Brain is request-driven: geen background sweepers. Memory Cortex levert residual context (semantic, episodic, working, procedural). Agent-OS beheert tools met policy allow|deny|ask en tasks met parent/child voor subagents. Traces zijn append-only voor replay. Bijgewerkt: {now}."
            ),
            "primary",
        ),
        "atlas://corpus/tool-calling-reliability" => (
            "Toolcalling betrouwbaarheid in lokale runtimes",
            "Toolcalling faalt vaak door ontbrekende schema-validatie of ontbrekende traces. Atlas registreert tools versioned, logt I/O in traces, en gebruikt budgets (max subagents, tokens, tijd) zodat orchestration niet ontspoort. EXEC-events mogen alleen van de backend komen.".into(),
            "primary",
        ),
        "atlas://corpus/memory-vs-training" => (
            "Geheugen versus modeltraining",
            "Geheugen (RAG / Memory Cortex) voegt kennis toe zonder gewichten te wijzigen. Training/fine-tune verandert het model. Conflicterende claims krijgen conflicts_with edges. Consolidatie (episodic→semantic) is on-demand, nooit als silent sweeper.".into(),
            "primary",
        ),
        "atlas://corpus/citation-verification" => (
            "Citation verification patterns",
            "Synthese produceert claims. De citation-agent koppelt elke claim aan source_id + fragment. Als het fragment de claim niet ondersteunt, blijft verified=false. SEO-bronnen scoren lager dan primaire documentatie.".into(),
            "secondary",
        ),
        "atlas://corpus/hardware-limits" => (
            "Grenzen van lokale hardware",
            "Lokale hardware beperkt parallelle subagents en contextvensters. Budgetknobs (max_subagents, max_sources, max_rounds) houden onderzoek voorspelbaar. Compaction: subagents leveren samenvattingen, geen raw HTML.".into(),
            "secondary",
        ),
        _ => (
            "Onbekende bron",
            format!("Stub-fetch voor {url}. Geen live netwerkpagina geladen."),
            "secondary",
        ),
    };
    FetchedPage {
        url: url.into(),
        title: title.into(),
        text,
        quality_tier: tier.into(),
        bytes: 0,
    }
}

fn quality_for_url(url: &str) -> &'static str {
    let u = url.to_lowercase();
    if u.contains("docs.") || u.contains("arxiv.org") || u.contains("github.com") || u.contains("rfc")
    {
        "primary"
    } else if u.contains("best-") || u.contains("affiliate") || u.contains("top-10") {
        "seo"
    } else {
        "secondary"
    }
}

fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len().min(16_000));
    let mut in_tag = false;
    let mut in_script = false;
    let lower = html.to_lowercase();
    // crude strip
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if lower[i..].starts_with("<script") || lower[i..].starts_with("<style") {
            in_script = true;
        }
        if in_script && (lower[i..].starts_with("</script>") || lower[i..].starts_with("</style>")) {
            in_script = false;
        }
        let c = bytes[i] as char;
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag && !in_script {
            out.push(c);
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title>")? + 7;
    let end = lower[start..].find("</title>")? + start;
    Some(html[start..end].trim().to_string())
}

#[allow(dead_code)]
pub fn source_meta(hit: &SearchHit) -> serde_json::Value {
    json!({
        "snippet": hit.snippet,
        "provider": "atlas-research",
    })
}
