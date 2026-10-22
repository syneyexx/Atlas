use crate::error::{AtlasError, AtlasResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Runtime settings — nothing about prompts or models is hardcoded in code paths.
/// Empty strings mean “not configured yet”; the operator fills them via settings / config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrainConfig {
    pub bind: String,
    /// Fully editable system prompt. Never injected from source defaults as policy text.
    pub system_prompt: String,
    pub model_id: String,
    /// Local LM OpenAI-compatible base, e.g. http://127.0.0.1:11434/v1
    pub lm_base_url: String,
    /// `local_http` talks to a local LM server. `stub` is for offline pipeline tests only.
    pub lm_provider: String,
    pub data_dir: PathBuf,
    pub cors_origins: Vec<String>,
    /// How many knowledge hits to inject into context (0 disables recall injection).
    pub knowledge_top_k: usize,
    /// Max characters of recalled knowledge block (legacy; token budget preferred).
    pub knowledge_max_chars: usize,

    // --- Memory Cortex ---
    /// `local_http` | `stub` | `local_hash`
    pub embed_provider: String,
    pub embed_base_url: String,
    pub embed_model: String,
    pub memory_token_budget: usize,
    pub memory_chunk_chars: usize,
    pub memory_dedupe_threshold: f64,
    pub memory_decay_lambda: f64,
    pub memory_min_salience: f64,
    pub memory_min_hit_score: f64,
    pub memory_hops: usize,
    pub memory_include_router_brief: bool,
}

impl Default for BrainConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080".into(),
            system_prompt: String::new(),
            model_id: String::new(),
            lm_base_url: "http://127.0.0.1:11434/v1".into(),
            lm_provider: "local_http".into(),
            data_dir: PathBuf::from("./data"),
            cors_origins: vec![
                "http://127.0.0.1:5173".into(),
                "http://localhost:5173".into(),
                "http://127.0.0.1:4173".into(),
                "http://localhost:4173".into(),
            ],
            knowledge_top_k: 6,
            knowledge_max_chars: 6000,
            embed_provider: "stub".into(),
            embed_base_url: "http://127.0.0.1:11434/v1".into(),
            embed_model: String::new(),
            memory_token_budget: 1800,
            memory_chunk_chars: 1800,
            memory_dedupe_threshold: 0.92,
            memory_decay_lambda: 0.002,
            memory_min_salience: 0.02,
            memory_min_hit_score: 0.04,
            memory_hops: 2,
            memory_include_router_brief: false,
        }
    }
}

/// Public settings surface for the UI — excludes bind/paths the browser must not rewrite blindly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicSettings {
    pub system_prompt: String,
    pub model_id: String,
    pub lm_base_url: String,
    pub lm_provider: String,
    pub knowledge_top_k: usize,
    pub knowledge_max_chars: usize,
    pub embed_provider: String,
    pub embed_base_url: String,
    pub embed_model: String,
    pub memory_token_budget: usize,
    pub memory_hops: usize,
}

impl From<&BrainConfig> for PublicSettings {
    fn from(c: &BrainConfig) -> Self {
        Self {
            system_prompt: c.system_prompt.clone(),
            model_id: c.model_id.clone(),
            lm_base_url: c.lm_base_url.clone(),
            lm_provider: c.lm_provider.clone(),
            knowledge_top_k: c.knowledge_top_k,
            knowledge_max_chars: c.knowledge_max_chars,
            embed_provider: c.embed_provider.clone(),
            embed_base_url: c.embed_base_url.clone(),
            embed_model: c.embed_model.clone(),
            memory_token_budget: c.memory_token_budget,
            memory_hops: c.memory_hops,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublicSettingsPatch {
    pub system_prompt: Option<String>,
    pub model_id: Option<String>,
    pub lm_base_url: Option<String>,
    pub lm_provider: Option<String>,
    pub knowledge_top_k: Option<usize>,
    pub knowledge_max_chars: Option<usize>,
    pub embed_provider: Option<String>,
    pub embed_base_url: Option<String>,
    pub embed_model: Option<String>,
    pub memory_token_budget: Option<usize>,
    pub memory_hops: Option<usize>,
}

impl BrainConfig {
    pub fn config_path(data_dir: &Path) -> PathBuf {
        data_dir.join("brain-config.json")
    }

    pub fn load_or_init(data_dir: PathBuf) -> AtlasResult<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let path = Self::config_path(&data_dir);
        if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            let mut cfg: BrainConfig = serde_json::from_str(&raw)?;
            cfg.data_dir = data_dir;
            return Ok(cfg);
        }
        let mut cfg = BrainConfig::default();
        cfg.data_dir = data_dir;
        cfg.save()?;
        Ok(cfg)
    }

    pub fn save(&self) -> AtlasResult<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        let path = Self::config_path(&self.data_dir);
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }

    pub fn apply_patch(&mut self, patch: PublicSettingsPatch) -> AtlasResult<()> {
        if let Some(v) = patch.system_prompt {
            self.system_prompt = v;
        }
        if let Some(v) = patch.model_id {
            self.model_id = v;
        }
        if let Some(v) = patch.lm_base_url {
            if !(v.starts_with("http://") || v.starts_with("https://")) {
                return Err(AtlasError::BadRequest(
                    "lm_base_url must be an HTTP(S) URL for a local LM.".into(),
                ));
            }
            self.lm_base_url = v;
        }
        if let Some(v) = patch.lm_provider {
            if v != "local_http" && v != "stub" {
                return Err(AtlasError::BadRequest(
                    "lm_provider must be local_http or stub.".into(),
                ));
            }
            self.lm_provider = v;
        }
        if let Some(v) = patch.knowledge_top_k {
            self.knowledge_top_k = v;
        }
        if let Some(v) = patch.knowledge_max_chars {
            self.knowledge_max_chars = v.max(256);
        }
        if let Some(v) = patch.embed_provider {
            if !matches!(v.as_str(), "local_http" | "stub" | "local_hash") {
                return Err(AtlasError::BadRequest(
                    "embed_provider must be local_http, stub, or local_hash.".into(),
                ));
            }
            self.embed_provider = v;
        }
        if let Some(v) = patch.embed_base_url {
            if !(v.starts_with("http://") || v.starts_with("https://")) {
                return Err(AtlasError::BadRequest(
                    "embed_base_url must be an HTTP(S) URL.".into(),
                ));
            }
            self.embed_base_url = v;
        }
        if let Some(v) = patch.embed_model {
            self.embed_model = v;
        }
        if let Some(v) = patch.memory_token_budget {
            self.memory_token_budget = v.max(256);
        }
        if let Some(v) = patch.memory_hops {
            self.memory_hops = v.clamp(1, 4);
        }
        self.save()
    }
}
