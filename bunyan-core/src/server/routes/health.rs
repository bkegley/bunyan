use axum::Json;
use crate::models::StatusResponse;

#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, body = StatusResponse)),
    operation_id = "health_check",
    tag = "health"
)]
pub async fn health() -> Json<StatusResponse> {
    Json(StatusResponse { status: "ok".into() })
}
