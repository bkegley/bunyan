use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::editor;
use crate::models::{ErrorResponse, OpenEditorInput, StatusResponse};
use crate::server::error::ApiError;
use crate::state::AppState;
use crate::workspace;

#[utoipa::path(get, path = "/editors", responses((status = 200, body = Vec<String>), (status = 500, body = ErrorResponse)), operation_id = "detect_editors", tag = "editors")]
pub async fn detect() -> Result<Json<Vec<String>>, ApiError> {
    let editors = tokio::task::spawn_blocking(|| editor::detect_installed_editors())
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?;

    Ok(Json(editors.iter().map(|e| e.id().to_string()).collect()))
}

#[utoipa::path(post, path = "/workspaces/{id}/editor", params(("id" = String, Path, description = "Workspace ID")), request_body = OpenEditorInput, responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)), operation_id = "open_editor", tag = "editors")]
pub async fn open(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<OpenEditorInput>,
) -> Result<Json<StatusResponse>, ApiError> {
    let ed = editor::Editor::from_id(&input.editor_id).ok_or_else(|| {
        ApiError(crate::error::BunyanError::NotFound(format!(
            "Unknown editor: {}",
            input.editor_id
        )))
    })?;

    let (ws, repo, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    if ed == editor::Editor::Iterm {
        let rn = repo.name.clone();
        let wn = ws.directory_name.clone();
        let wp = ws_path.clone();
        tokio::task::spawn_blocking(move || crate::tmux::ensure_workspace_window(&rn, &wn, &wp))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?;

        let rn = repo.name.clone();
        let wn = ws.directory_name.clone();
        tokio::task::spawn_blocking(move || crate::terminal::attach_iterm(&rn, &wn))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?;
    } else {
        let wp = ws_path.clone();
        tokio::task::spawn_blocking(move || editor::open_in_editor(&ed, &wp))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?;
    }

    Ok(Json(StatusResponse { status: "opened".into() }))
}
