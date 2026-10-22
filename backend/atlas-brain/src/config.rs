use crate::error::{AtlasError, AtlasResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Runtime settings — nothing about prompts or models is hardcoded in code paths.
/// Empty strings mean “not configured yet”; the operator fills them via settings / config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Max characters of recalled knowledge block.
    pub knowledge_max_chars: usize,
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
        self.save()
    }
}
