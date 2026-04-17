use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::db;
use crate::models::{ErrorResponse, SetSettingInput, Setting};
use crate::server::error::ApiError;
use crate::state::AppState;

#[utoipa::path(get, path = "/settings", responses((status = 200, body = Vec<Setting>), (status = 500, body = ErrorResponse)), operation_id = "list_settings", tag = "settings")]
pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Setting>>, ApiError> {
    let conn = state.db.lock().unwrap();
    let settings = db::settings::get_all(&conn)?;
    Ok(Json(settings))
}

#[utoipa::path(get, path = "/settings/{key}", params(("key" = String, Path, description = "Setting key")), responses((status = 200, body = Setting), (status = 404, body = ErrorResponse)), operation_id = "get_setting", tag = "settings")]
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Result<Json<Setting>, ApiError> {
    let conn = state.db.lock().unwrap();
    let setting = db::settings::get(&conn, &key)?;
    Ok(Json(setting))
}

#[utoipa::path(put, path = "/settings/{key}", params(("key" = String, Path, description = "Setting key")), request_body = SetSettingInput, responses((status = 200, body = Setting), (status = 500, body = ErrorResponse)), operation_id = "set_setting", tag = "settings")]
pub async fn set(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Json(input): Json<SetSettingInput>,
) -> Result<Json<Setting>, ApiError> {
    let conn = state.db.lock().unwrap();
    let setting = db::settings::set(&conn, &key, &input.value)?;
    Ok(Json(setting))
}
