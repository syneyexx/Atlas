use crate::config::BrainConfig;
use crate::error::{AtlasError, AtlasResult};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LmMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<LmMessage>,
}

#[derive(Clone)]
pub struct LocalLmClient {
    http: reqwest::Client,
}

impl LocalLmClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .expect("reqwest client"),
        }
    }

    /// List models from the local LM — only called when the UI/API asks, never on a timer.
    pub async fn list_models(&self, cfg: &BrainConfig) -> AtlasResult<Vec<String>> {
        if cfg.lm_provider == "stub" {
            let id = if cfg.model_id.is_empty() {
                "atlas-stub".to_string()
            } else {
                cfg.model_id.clone()
            };
            return Ok(vec![id]);
        }
        let base = cfg.lm_base_url.trim_end_matches('/');
        let url = format!("{base}/models");
        let response = self.http.get(&url).send().await.map_err(|e| {
            AtlasError::Upstream(format!("Local LM niet bereikbaar op {url}: {e}"))
        })?;
        if !response.status().is_success() {
            return Err(AtlasError::Upstream(format!(
                "Local LM /models gaf HTTP {}",
                response.status()
            )));
        }
        let body: ModelsResponse = response.json().await?;
        Ok(body
            .data
            .into_iter()
            .map(|m| m.id)
            .filter(|id| !id.is_empty())
            .collect())
    }

    pub async fn complete(
        &self,
        cfg: &BrainConfig,
        request: CompletionRequest,
    ) -> AtlasResult<String> {
        if cfg.lm_provider == "stub" {
            return Ok(stub_complete(&request));
        }
        let base = cfg.lm_base_url.trim_end_matches('/');
        let url = format!("{base}/chat/completions");
        let payload = json!({
            "model": request.model,
            "messages": request.messages,
            "stream": false
        });
        let response = self
            .http
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AtlasError::Upstream(format!("Local LM completion faalde: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AtlasError::Upstream(format!(
                "Local LM completion HTTP {status}: {body}"
            )));
        }
        let body: CompletionResponse = response.json().await?;
        body.choices
            .into_iter()
            .next()
            .and_then(|c| c.message)
            .and_then(|m| m.content)
            .filter(|c| !c.is_empty())
            .ok_or_else(|| AtlasError::Upstream("Local LM gaf geen content terug.".into()))
    }
}

fn stub_complete(request: &CompletionRequest) -> String {
    let last_user = request
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");
    let has_knowledge = request
        .messages
        .iter()
        .any(|m| m.role == "system" && m.content.contains("[ATLAS_KNOWLEDGE]"));
    let has_system = request
        .messages
        .iter()
        .any(|m| m.role == "system" && !m.content.contains("[ATLAS_KNOWLEDGE]"));
    format!(
        "Atlas Brain stub-antwoord (lokale pipeline).\n\nSystem prompt actief: {}\nKennis geïnjecteerd: {}\n\nJouw vraag:\n{}",
        if has_system { "ja" } else { "nee" },
        if has_knowledge { "ja" } else { "nee" },
        last_user
    )
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelCard>,
}

#[derive(Debug, Deserialize)]
struct ModelCard {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CompletionResponse {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Option<MessageContent>,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    content: Option<String>,
}
