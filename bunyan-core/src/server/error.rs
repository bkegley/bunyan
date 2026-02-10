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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    fn status_of(err: BunyanError) -> StatusCode {
        let resp = ApiError(err).into_response();
        resp.status()
    }

    #[test]
    fn not_found_maps_to_404() {
        assert_eq!(status_of(BunyanError::NotFound("x".into())), StatusCode::NOT_FOUND);
    }

    #[test]
    fn database_maps_to_500() {
        let db_err = rusqlite::Connection::open_in_memory()
            .unwrap()
            .execute("INVALID SQL", [])
            .unwrap_err();
        assert_eq!(
            status_of(BunyanError::Database(db_err)),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn serialization_maps_to_400() {
        let ser_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        assert_eq!(
            status_of(BunyanError::Serialization(ser_err)),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn git_maps_to_500() {
        assert_eq!(
            status_of(BunyanError::Git("clone failed".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn process_maps_to_500() {
        assert_eq!(
            status_of(BunyanError::Process("died".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn docker_maps_to_500() {
        assert_eq!(
            status_of(BunyanError::Docker("no daemon".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
