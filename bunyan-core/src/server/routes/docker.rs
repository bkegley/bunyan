use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::db;
use crate::docker;
use crate::models::{ContainerStatusResponse, DockerStatusResponse, ErrorResponse, PortMapping};
use crate::server::error::ApiError;
use crate::state::AppState;

#[utoipa::path(get, path = "/docker/status", responses((status = 200, body = DockerStatusResponse), (status = 500, body = ErrorResponse)), operation_id = "docker_status", tag = "docker")]
pub async fn status() -> Result<Json<DockerStatusResponse>, ApiError> {
    let available = docker::check_docker().await.map_err(ApiError)?;
    Ok(Json(DockerStatusResponse { available }))
}

#[utoipa::path(get, path = "/workspaces/{id}/container/status", params(("id" = String, Path, description = "Workspace ID")), responses((status = 200, body = ContainerStatusResponse), (status = 404, body = ErrorResponse)), tag = "docker")]
pub async fn container_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ContainerStatusResponse>, ApiError> {
    let container_id = {
        let conn = state.db.lock().unwrap();
        let ws = db::workspaces::get(&conn, &id)?;
        ws.container_id
    };

    let status = match container_id {
        Some(id) => docker::get_container_status(&id).await.map_err(ApiError)?,
        None => "none".to_string(),
    };

    Ok(Json(ContainerStatusResponse { status }))
}

#[utoipa::path(get, path = "/workspaces/{id}/container/ports", params(("id" = String, Path, description = "Workspace ID")), responses((status = 200, body = Vec<PortMapping>), (status = 404, body = ErrorResponse)), tag = "docker")]
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
