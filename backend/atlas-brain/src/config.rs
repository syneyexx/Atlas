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
    /// Similarity floor for conflict detection (below dedupe, above noise).
    pub memory_conflict_sim_low: f64,
    pub memory_decay_lambda: f64,
    pub memory_min_salience: f64,
    pub memory_min_hit_score: f64,
    pub memory_hops: usize,
    pub memory_include_router_brief: bool,
    /// Working KV TTL (seconds). Expiry checked on read only.
    pub working_ttl_secs: u64,
    pub working_max_pins: usize,
    /// Editable consolidate instruction (not a SYSTEM_PROMPT ethics layer).
    pub consolidate_prompt: String,
    pub memory_auto_consolidate: bool,
    pub memory_auto_consolidate_min: usize,
    pub memory_write_episodic_on_chat: bool,
    /// Opt-in semantic answer cache (invalidated via API / model change).
    pub semantic_cache_enabled: bool,

    // --- M5 engines ---
    /// `sqlite` | `postgres` | `dual` (sqlite active + postgres durable mirror)
    pub db_backend: String,
    /// postgres://… when db_backend is postgres|dual
    pub database_url: String,
    /// redis://… for working memory + semantic cache hot path
    pub redis_url: String,
    /// `filesystem` | `s3`
    pub object_backend: String,
    pub s3_endpoint: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    /// Hex or raw key material for secrets store (prefer ATLAS_SECRETS_KEY env).
    pub secrets_key: String,
    /// Default project scope for new memories/docs when unset.
    pub default_project_id: String,
    /// Semantic cache TTL when using Redis (seconds).
    pub semantic_cache_ttl_secs: u64,

    // --- Research plane ---
    /// `stub` (offline corpus) | `http` (optional search endpoint)
    pub research_search_provider: String,
    /// Optional search HTTP endpoint (`?q=&limit=`) when provider=http.
    pub research_search_url: String,
    pub research_max_subagents: usize,
    pub research_max_rounds: usize,
    pub research_max_sources: usize,
    /// Max request-driven ticks per job (budget guard).
    pub research_max_ticks: usize,
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
            memory_conflict_sim_low: 0.78,
            memory_decay_lambda: 0.002,
            memory_min_salience: 0.02,
            memory_min_hit_score: 0.04,
            memory_hops: 2,
            memory_include_router_brief: false,
            working_ttl_secs: 3600,
            working_max_pins: 8,
            consolidate_prompt: String::new(),
            memory_auto_consolidate: false,
            memory_auto_consolidate_min: 3,
            memory_write_episodic_on_chat: true,
            semantic_cache_enabled: false,
            db_backend: "sqlite".into(),
            database_url: String::new(),
            redis_url: String::new(),
            object_backend: "filesystem".into(),
            s3_endpoint: String::new(),
            s3_bucket: String::new(),
            s3_region: "us-east-1".into(),
            s3_access_key: String::new(),
            s3_secret_key: String::new(),
            secrets_key: String::new(),
            default_project_id: String::new(),
            semantic_cache_ttl_secs: 3600,
            research_search_provider: "stub".into(),
            research_search_url: String::new(),
            research_max_subagents: 4,
            research_max_rounds: 2,
            research_max_sources: 10,
            research_max_ticks: 32,
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
    pub working_ttl_secs: u64,
    pub memory_auto_consolidate: bool,
    pub consolidate_prompt: String,
    pub semantic_cache_enabled: bool,
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
            working_ttl_secs: c.working_ttl_secs,
            memory_auto_consolidate: c.memory_auto_consolidate,
            consolidate_prompt: c.consolidate_prompt.clone(),
            semantic_cache_enabled: c.semantic_cache_enabled,
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
    pub working_ttl_secs: Option<u64>,
    pub memory_auto_consolidate: Option<bool>,
    pub consolidate_prompt: Option<String>,
    pub semantic_cache_enabled: Option<bool>,
}

impl BrainConfig {
    pub fn config_path(data_dir: &Path) -> PathBuf {
        data_dir.join("brain-config.json")
    }

    pub fn load_or_init(data_dir: PathBuf) -> AtlasResult<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let path = Self::config_path(&data_dir);
        let mut cfg = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            let mut cfg: BrainConfig = serde_json::from_str(&raw)?;
            cfg.data_dir = data_dir;
            cfg
        } else {
            let mut cfg = BrainConfig::default();
            cfg.data_dir = data_dir;
            cfg.save()?;
            cfg
        };
        // Local dignity: always have a secrets key so encrypted store works out of the box.
        if cfg.secrets_key.trim().is_empty() {
            use rand::RngCore;
            let mut bytes = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut bytes);
            cfg.secrets_key = hex::encode(bytes);
            let _ = cfg.save();
        }
        // Environment overrides (never log secret values).
        if let Ok(v) = std::env::var("ATLAS_DB_BACKEND") {
            cfg.db_backend = v;
        }
        if let Ok(v) = std::env::var("ATLAS_DATABASE_URL") {
            cfg.database_url = v;
        }
        if let Ok(v) = std::env::var("ATLAS_REDIS_URL") {
            cfg.redis_url = v;
        }
        if let Ok(v) = std::env::var("ATLAS_OBJECT_BACKEND") {
            cfg.object_backend = v;
        }
        if let Ok(v) = std::env::var("ATLAS_S3_ENDPOINT") {
            cfg.s3_endpoint = v;
        }
        if let Ok(v) = std::env::var("ATLAS_S3_BUCKET") {
            cfg.s3_bucket = v;
        }
        if let Ok(v) = std::env::var("ATLAS_S3_REGION") {
            cfg.s3_region = v;
        }
        if let Ok(v) = std::env::var("ATLAS_S3_ACCESS_KEY") {
            cfg.s3_access_key = v;
        }
        if let Ok(v) = std::env::var("ATLAS_S3_SECRET_KEY") {
            cfg.s3_secret_key = v;
        }
        if let Ok(v) = std::env::var("ATLAS_SECRETS_KEY") {
            cfg.secrets_key = v;
        }
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
        if let Some(v) = patch.working_ttl_secs {
            self.working_ttl_secs = v.max(60);
        }
        if let Some(v) = patch.memory_auto_consolidate {
            self.memory_auto_consolidate = v;
        }
        if let Some(v) = patch.consolidate_prompt {
            self.consolidate_prompt = v;
        }
        if let Some(v) = patch.semantic_cache_enabled {
            self.semantic_cache_enabled = v;
            if !v {
                // Turning cache off does not require wipe; explicit clear API exists.
            }
        }
        self.save()
    }
}
