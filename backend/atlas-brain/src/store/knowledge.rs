use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeNode {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeHit {
    pub id: String,
    pub title: String,
    pub content: String,
    pub score: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LearnRequest {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecallRequest {
    pub query: String,
    #[serde(default)]
    pub top_k: Option<usize>,
}
