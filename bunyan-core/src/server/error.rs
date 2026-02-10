use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::error::BunyanError;

pub struct ApiError(pub BunyanError);

impl From<BunyanError> for ApiError {
    fn from(err: BunyanError) -> Self {
        ApiError(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self.0 {
            BunyanError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            BunyanError::Database(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Database error: {}", e))
            }
            BunyanError::Serialization(e) => {
                (StatusCode::BAD_REQUEST, format!("Serialization error: {}", e))
            }
            BunyanError::Git(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Git error: {}", msg))
            }
            BunyanError::Process(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Process error: {}", msg))
            }
            BunyanError::Docker(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Docker error: {}", msg))
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}
