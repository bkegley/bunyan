use tauri::State;

use crate::db;
use crate::docker;
use crate::models::PortMapping;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn check_docker_available() -> Result<bool, String> {
    docker::check_docker().await.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_container_status(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<String, String> {
    let container_id = {
        let conn = state.db.lock().unwrap();
        let ws = db::workspaces::get(&conn, &workspace_id).map_err(|e| e.to_string())?;
        ws.container_id
    };

    match container_id {
        Some(id) => docker::get_container_status(&id)
            .await
            .map_err(|e| e.to_string()),
        None => Ok("none".to_string()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_container_ports(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Vec<PortMapping>, String> {
    let container_id = {
        let conn = state.db.lock().unwrap();
        let ws = db::workspaces::get(&conn, &workspace_id).map_err(|e| e.to_string())?;
        ws.container_id
    };

    match container_id {
        Some(id) => docker::get_container_ports(&id)
            .await
            .map_err(|e| e.to_string()),
        None => Ok(vec![]),
    }
}
