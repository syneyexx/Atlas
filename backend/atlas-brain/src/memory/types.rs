use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Working,
    Episodic,
    Semantic,
    SelfProfile,
    Procedural,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryKind::Working => "working",
            MemoryKind::Episodic => "episodic",
            MemoryKind::Semantic => "semantic",
            MemoryKind::SelfProfile => "self_profile",
            MemoryKind::Procedural => "procedural",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "working" => Some(Self::Working),
            "episodic" => Some(Self::Episodic),
            "semantic" => Some(Self::Semantic),
            "self_profile" | "self" | "profile" => Some(Self::SelfProfile),
            "procedural" => Some(Self::Procedural),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    Chat,
    Learn,
    Distill,
    Attachment,
    Profile,
    Migrate,
}

impl MemorySource {
    pub fn as_str(self) -> &'static str {
        match self {
            MemorySource::Chat => "chat",
            MemorySource::Learn => "learn",
            MemorySource::Distill => "distill",
            MemorySource::Attachment => "attachment",
            MemorySource::Profile => "profile",
            MemorySource::Migrate => "migrate",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "chat" => Self::Chat,
            "distill" => Self::Distill,
            "attachment" => Self::Attachment,
            "profile" => Self::Profile,
            "migrate" => Self::Migrate,
            _ => Self::Learn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Conversation,
    Project,
    Global,
}

impl MemoryScope {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryScope::Conversation => "conversation",
            MemoryScope::Project => "project",
            MemoryScope::Global => "global",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "conversation" => Self::Conversation,
            "project" => Self::Project,
            _ => Self::Global,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub source: MemorySource,
    pub created_at: String,
    #[serde(default)]
    pub parent_ids: Vec<String>,
    pub confidence: f64,
    pub scope: MemoryScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryNode {
    pub id: String,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub provenance: Provenance,
    pub salience: f64,
    pub decay: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHit {
    pub id: String,
    pub kind: MemoryKind,
    pub title: String,
    pub content: String,
    pub score: f64,
    pub head: String,
    pub confidence: f64,
    pub salience: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFrame {
    pub role: String,
    pub label: String,
    pub content: String,
    pub approx_tokens: usize,
    pub priority: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionHeadResult {
    pub head: String,
    pub hits: Vec<MemoryHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterDecision {
    pub working: f64,
    pub episodic: f64,
    pub semantic: f64,
    pub self_profile: f64,
    /// How-to / workflow / skill memories.
    #[serde(default)]
    pub procedural: f64,
}

impl Default for RouterDecision {
    fn default() -> Self {
        Self {
            working: 0.15,
            episodic: 0.25,
            semantic: 0.40,
            self_profile: 0.30,
            procedural: 0.25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTrace {
    pub router: RouterDecision,
    pub heads: Vec<AttentionHeadResult>,
    pub frames: Vec<ContextFrame>,
    pub budget_tokens: usize,
    pub used_tokens: usize,
    pub cut_labels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LearnMemoryRequest {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub parent_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecallMemoryRequest {
    pub query: String,
    #[serde(default)]
    pub top_k: Option<usize>,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub trace: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecallMemoryResponse {
    pub hits: Vec<MemoryHit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<MemoryTrace>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileUpsertRequest {
    pub key: String,
    pub content: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteAction {
    Inserted,
    Merged,
    Deduped,
    /// Near-topic but divergent content — both kept, linked via conflicts_with.
    Conflicted,
    Discarded,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteDecision {
    pub action: WriteAction,
    pub node: Option<MemoryNode>,
    pub reason: String,
}
