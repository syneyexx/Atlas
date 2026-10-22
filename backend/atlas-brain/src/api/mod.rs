mod agent_api;
mod chat;
mod config_api;
mod conversations;
mod documents;
mod eval_api;
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
        // Document RAG (M2)
        .route(
            "/v1/atlas/documents",
            get(documents::list).post(documents::ingest),
        )
        .route("/v1/atlas/documents/search", get(documents::search))
        .route(
            "/v1/atlas/documents/{id}",
            get(documents::get).delete(documents::delete),
        )
        // Agent OS (M3)
        .route(
            "/v1/atlas/tools",
            get(agent_api::list_tools).post(agent_api::upsert_tool),
        )
        .route(
            "/v1/atlas/tools/{name}",
            get(agent_api::get_tool).put(agent_api::patch_tool),
        )
        .route(
            "/v1/atlas/tasks",
            get(agent_api::list_tasks).post(agent_api::create_task),
        )
        .route("/v1/atlas/tasks/{id}", get(agent_api::get_task))
        .route(
            "/v1/atlas/tasks/{id}/checkpoint",
            post(agent_api::checkpoint_task),
        )
        .route(
            "/v1/atlas/tasks/{id}/status",
            post(agent_api::set_task_status),
        )
        .route(
            "/v1/atlas/prompts",
            get(agent_api::list_prompts).post(agent_api::create_prompt),
        )
        .route("/v1/atlas/prompts/{id}", get(agent_api::get_prompt))
        .route(
            "/v1/atlas/prompts/{id}/activate",
            post(agent_api::activate_prompt),
        )
        .route(
            "/v1/atlas/models/registry",
            get(agent_api::list_models_registry).post(agent_api::upsert_model_registry),
        )
        // Improvement loop (M4)
        .route(
            "/v1/atlas/evals/cases",
            get(eval_api::list_cases).post(eval_api::upsert_case),
        )
        .route("/v1/atlas/evals/run", post(eval_api::run_suite))
        .route("/v1/atlas/evals/runs", get(eval_api::list_runs))
        .route(
            "/v1/atlas/feedback",
            get(eval_api::list_feedback).post(eval_api::post_feedback),
        )
        .route("/v1/atlas/training", get(eval_api::list_training))
        .route(
            "/v1/atlas/cache",
            get(eval_api::cache_lookup)
                .post(eval_api::cache_put)
                .delete(eval_api::cache_clear),
        )
}
