use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::db;
use crate::docker;
use crate::models::PortMapping;
use crate::server::error::ApiError;
use crate::state::AppState;

pub async fn status() -> Result<Json<serde_json::Value>, ApiError> {
    let available = docker::check_docker().await.map_err(ApiError)?;
    Ok(Json(serde_json::json!({ "available": available })))
}

pub async fn container_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let container_id = {
        let conn = state.db.lock().unwrap();
        let ws = db::workspaces::get(&conn, &id)?;
        ws.container_id
    };

    let status = match container_id {
        Some(id) => docker::get_container_status(&id).await.map_err(ApiError)?,
        None => "none".to_string(),
    };

    Ok(Json(serde_json::json!({ "status": status })))
}

pub async fn container_ports(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<PortMapping>>, ApiError> {
    let container_id = {
        let conn = state.db.lock().unwrap();
        let ws = db::workspaces::get(&conn, &id)?;
        ws.container_id
    };

    let ports = match container_id {
        Some(id) => docker::get_container_ports(&id).await.map_err(ApiError)?,
        None => vec![],
    };

    Ok(Json(ports))
}
