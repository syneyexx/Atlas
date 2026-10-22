mod chat;
mod config_api;
mod conversations;
mod knowledge;
mod memory_api;
mod models;

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
        .route("/v1/atlas/memory/profile", get(memory_api::list_profile).post(memory_api::upsert_profile))
        .route("/v1/atlas/memory/{id}", delete(memory_api::forget))
}
