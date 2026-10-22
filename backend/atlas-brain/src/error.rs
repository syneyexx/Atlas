use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AtlasError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Upstream(String),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl IntoResponse for AtlasError {
    fn into_response(self) -> Response {
        let status = match &self {
            AtlasError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AtlasError::NotFound(_) => StatusCode::NOT_FOUND,
            AtlasError::Upstream(_) => StatusCode::BAD_GATEWAY,
            AtlasError::Db(_) | AtlasError::Io(_) | AtlasError::Json(_) | AtlasError::Http(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        (
            status,
            Json(json!({
                "error": {
                    "message": self.to_string(),
                    "type": "atlas_brain_error"
                }
            })),
        )
            .into_response()
    }
}

pub type AtlasResult<T> = Result<T, AtlasError>;
