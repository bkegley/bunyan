//! POST /delegate — the agent-first endpoint.
//!
//! See `delegation::delegate` for the actual flow. This module is a thin
//! HTTP adapter so the value-prop endpoint is discoverable from the routes
//! index instead of buried in workspaces.rs.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::delegation::{self, DelegateInput, DelegateResponse};
use crate::models::ErrorResponse;
use crate::server::error::ApiError;
use crate::state::AppState;

#[utoipa::path(
    post,
    path = "/delegate",
    request_body = DelegateInput,
    responses(
        (status = 201, body = DelegateResponse),
        (status = 404, body = ErrorResponse),
        (status = 500, body = ErrorResponse),
    ),
    tag = "delegate"
)]
pub async fn delegate(
    State(state): State<Arc<AppState>>,
    Json(input): Json<DelegateInput>,
) -> Result<(StatusCode, Json<DelegateResponse>), ApiError> {
    let origin = state.server_origin();
    let resp = delegation::delegate(&state, input, &origin)
        .await
        .map_err(ApiError)?;
    Ok((StatusCode::CREATED, Json(resp)))
}
