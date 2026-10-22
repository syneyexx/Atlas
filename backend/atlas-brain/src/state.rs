use crate::config::BrainConfig;
use crate::lm::LocalLmClient;
use crate::memory::EmbedClient;
use crate::store::Store;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<BrainConfig>>,
    pub store: Arc<Store>,
    pub lm: LocalLmClient,
    pub embed: EmbedClient,
}
