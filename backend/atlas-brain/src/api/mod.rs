mod chat;
mod config_api;
mod conversations;
mod knowledge;
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
}
