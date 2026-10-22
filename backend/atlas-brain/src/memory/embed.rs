use crate::config::BrainConfig;
use crate::error::{AtlasError, AtlasResult};
use serde::Deserialize;
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub const EMBED_DIM: usize = 64;

#[derive(Clone)]
pub struct EmbedClient {
    http: reqwest::Client,
}

impl EmbedClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("embed http client"),
        }
    }

    pub async fn embed(&self, cfg: &BrainConfig, text: &str) -> AtlasResult<Vec<f32>> {
        match cfg.embed_provider.as_str() {
            "local_http" => self.embed_http(cfg, text).await,
            // Deterministic local hash embedding for offline / stub pipelines.
            "stub" | "local_hash" => Ok(hash_embed(text, EMBED_DIM)),
            other => Err(AtlasError::BadRequest(format!(
                "Onbekende embed_provider: {other}"
            ))),
        }
    }
}

async fn embed_http_inner(
    http: &reqwest::Client,
    cfg: &BrainConfig,
    text: &str,
) -> AtlasResult<Vec<f32>> {
    let base = cfg.embed_base_url.trim_end_matches('/');
    let url = format!("{base}/embeddings");
    let model = if cfg.embed_model.is_empty() {
        cfg.model_id.clone()
    } else {
        cfg.embed_model.clone()
    };
    if model.is_empty() {
        return Err(AtlasError::BadRequest(
            "embed_model (of model_id) is verplicht voor local_http embeddings.".into(),
        ));
    }
    let response = http
        .post(&url)
        .json(&json!({ "model": model, "input": text }))
        .send()
        .await
        .map_err(|e| AtlasError::Upstream(format!("Embedder niet bereikbaar op {url}: {e}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AtlasError::Upstream(format!(
            "Embedder HTTP {status}: {body}"
        )));
    }
    let body: EmbedResponse = response.json().await?;
    let vector = body
        .data
        .into_iter()
        .next()
        .map(|d| d.embedding)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AtlasError::Upstream("Embedder gaf geen vector terug.".into()))?;
    Ok(l2_normalize(&vector))
}

impl EmbedClient {
    async fn embed_http(&self, cfg: &BrainConfig, text: &str) -> AtlasResult<Vec<f32>> {
        embed_http_inner(&self.http, cfg, text).await
    }
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    #[serde(default)]
    data: Vec<EmbedData>,
}

#[derive(Debug, Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}

/// Stable bag-of-hashed-tokens embedding (no background jobs, fully on-demand).
pub fn hash_embed(text: &str, dim: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; dim];
    for token in text.split(|c: char| !c.is_alphanumeric()) {
        let t = token.to_lowercase();
        if t.len() < 2 {
            continue;
        }
        let mut hasher = DefaultHasher::new();
        t.hash(&mut hasher);
        let h = hasher.finish();
        let idx = (h as usize) % dim;
        let sign = if (h >> 63) & 1 == 1 { 1.0 } else { -1.0 };
        v[idx] += sign;
        // Bigram boost
        if t.len() >= 3 {
            let mut h2 = DefaultHasher::new();
            (t.as_bytes()[0], t.as_bytes()[t.len() - 1]).hash(&mut h2);
            let hh = h2.finish();
            v[(hh as usize) % dim] += 0.5 * if (hh >> 62) & 1 == 1 { 1.0 } else { -1.0 };
        }
    }
    l2_normalize(&v)
}

pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1e-8 {
        return v.to_vec();
    }
    v.iter().map(|x| x / norm).collect()
}

pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    for i in 0..a.len() {
        dot += f64::from(a[i]) * f64::from(b[i]);
    }
    dot.clamp(-1.0, 1.0)
}

pub fn encode_vector(v: &[f32]) -> AtlasResult<Vec<u8>> {
    Ok(serde_json::to_vec(v)?)
}

pub fn decode_vector(bytes: &[u8]) -> AtlasResult<Vec<f32>> {
    Ok(serde_json::from_slice(bytes)?)
}
