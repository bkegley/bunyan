use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use crate::db;
use crate::models::Setting;
use crate::server::error::ApiError;
use crate::state::AppState;

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Setting>>, ApiError> {
    let conn = state.db.lock().unwrap();
    let settings = db::settings::get_all(&conn)?;
    Ok(Json(settings))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Result<Json<Setting>, ApiError> {
    let conn = state.db.lock().unwrap();
    let setting = db::settings::get(&conn, &key)?;
    Ok(Json(setting))
}

#[derive(Deserialize)]
pub struct SetSettingInput {
    pub value: String,
}

pub async fn set(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Json(input): Json<SetSettingInput>,
) -> Result<Json<Setting>, ApiError> {
    let conn = state.db.lock().unwrap();
    let setting = db::settings::set(&conn, &key, &input.value)?;
    Ok(Json(setting))
}
