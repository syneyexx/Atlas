mod api;
mod brain;
mod config;
mod error;
mod lm;
mod memory;
mod state;
mod store;

use crate::config::BrainConfig;
use crate::lm::LocalLmClient;
use crate::memory::EmbedClient;
use crate::state::AppState;
use crate::store::Store;
use axum::http::{HeaderValue, Method};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("atlas_brain=info".parse().unwrap()))
        .init();

    let data_dir = std::env::var("ATLAS_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./data"));

    let config = BrainConfig::load_or_init(data_dir).expect("brain config");
    let bind = config.bind.clone();
    let cors_origins = config.cors_origins.clone();
    let store = Store::open(&config).expect("open atlas store");

    let state = AppState {
        config: Arc::new(RwLock::new(config)),
        store: Arc::new(store),
        lm: LocalLmClient::new(),
        embed: EmbedClient::new(),
    };

    let cors = build_cors(&cors_origins);
    let app = api::router().layer(cors).with_state(state);

    let addr: SocketAddr = bind.parse().expect("bind address");
    tracing::info!("Atlas Brain listening on http://{addr} (request-driven, no health loops)");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind listener");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server");
}

fn build_cors(origins: &[String]) -> CorsLayer {
    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(parsed))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(tower_http::cors::Any)
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
