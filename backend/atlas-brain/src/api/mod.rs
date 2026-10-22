mod chat;
mod config_api;
mod conversations;
mod knowledge;
mod memory_api;
mod models;
mod store_api;

use crate::state::AppState;
use axum::routing::{delete, get, post};
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/models", get(models::list_models))
        .route("/v1/chat/completions", post(chat::chat_completions))
        .route(
            "/v1/atlas/settings",
            get(config_api::get_settings).put(config_api::put_settings),
        )
        .route(
            "/v1/atlas/conversations",
            get(conversations::list).post(conversations::upsert),
        )
        .route(
            "/v1/atlas/conversations/{id}",
            get(conversations::get).delete(conversations::delete),
        )
        .route(
            "/v1/atlas/knowledge",
            get(knowledge::list).post(knowledge::learn),
        )
        .route("/v1/atlas/knowledge/recall", post(knowledge::recall))
        .route("/v1/atlas/knowledge/{id}", delete(knowledge::delete))
        .route("/v1/atlas/memory", get(memory_api::list).post(memory_api::learn))
        .route("/v1/atlas/memory/recall", post(memory_api::recall))
        .route("/v1/atlas/memory/migrate", post(memory_api::migrate))
        .route("/v1/atlas/memory/consolidate", post(memory_api::consolidate))
        .route(
            "/v1/atlas/memory/profile",
            get(memory_api::list_profile).post(memory_api::upsert_profile),
        )
        .route("/v1/atlas/memory/working/{id}", get(memory_api::get_working))
        .route("/v1/atlas/memory/{id}", delete(memory_api::forget))
        // Store Plane M0
        .route("/v1/atlas/traces", get(store_api::list_traces))
        .route("/v1/atlas/traces/{id}", get(store_api::get_trace))
        .route(
            "/v1/atlas/outbox",
            get(store_api::list_outbox).post(store_api::drain_outbox),
        )
        .route("/v1/atlas/edges", post(store_api::upsert_edge))
        .route("/v1/atlas/edges/{id}", get(store_api::list_edges))
}
