use axum::Json;

use crate::models::SystemInfo;
use crate::server::error::ApiError;

#[utoipa::path(get, path = "/system/info", responses((status = 200, body = SystemInfo), (status = 500, body = crate::models::ErrorResponse)), operation_id = "system_info", tag = "system")]
pub async fn info() -> Result<Json<SystemInfo>, ApiError> {
    let home = dirs::home_dir()
        .ok_or_else(|| {
            ApiError(crate::error::BunyanError::Process(
                "Cannot determine home directory".to_string(),
            ))
        })?
        .to_string_lossy()
        .to_string();

    Ok(Json(SystemInfo { home_dir: home }))
}
