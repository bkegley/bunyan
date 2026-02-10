use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use crate::db;
use crate::models::{TmuxPane, WorkspacePaneInfo};
use crate::server::error::ApiError;
use crate::state::AppState;
use crate::tmux;

pub async fn active(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<WorkspacePaneInfo>>, ApiError> {
    let all_panes = tokio::task::spawn_blocking(|| tmux::list_all_panes())
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?;

    if all_panes.is_empty() {
        return Ok(Json(vec![]));
    }

    let mut grouped: std::collections::HashMap<(String, String), Vec<TmuxPane>> =
        std::collections::HashMap::new();
    for (session_name, window_name, pane) in all_panes {
        grouped
            .entry((session_name, window_name))
            .or_default()
            .push(pane);
    }

    let (workspaces, repos) = {
        let conn = state.db.lock().unwrap();
        let ws = db::workspaces::list(&conn, None)?;
        let rp = db::repos::list(&conn)?;
        (ws, rp)
    };

    let mut results = Vec::new();
    for ((session_name, window_name), panes) in grouped {
        let workspace = workspaces.iter().find(|ws| {
            ws.directory_name == window_name
                && repos
                    .iter()
                    .any(|r| r.id == ws.repository_id && r.name == session_name)
        });

        if let Some(ws) = workspace {
            results.push(WorkspacePaneInfo {
                workspace_id: ws.id.clone(),
                repo_name: session_name,
                workspace_name: window_name,
                panes,
            });
        }
    }

    Ok(Json(results))
}
