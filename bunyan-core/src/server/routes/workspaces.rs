use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::db;
use crate::docker;
use crate::events::{self, names};
use crate::git::{GitOps, RealGit};
use crate::models::{
    ClaudeResumeInput, ClaudeSessionEntry, ContainerMode, CreateWorkspaceInput, ErrorResponse,
    StatusResponse, TmuxPane, Workspace,
};
use crate::server::error::ApiError;
use crate::sessions;
use crate::state::AppState;
use crate::terminal;
use crate::workspace;

#[derive(Deserialize)]
pub struct ListQuery {
    pub repo_id: Option<String>,
    /// "ready" or "archived". Maps to WorkspaceState.
    pub status: Option<String>,
    /// Filter to workspaces spawned by this parent workspace ID.
    pub delegated_by: Option<String>,
    /// ISO-8601 timestamp; only rows with created_at >= since.
    pub since: Option<String>,
}

/// Open the workspace view: fire `workspace.ready_to_view` and let the
/// configured hook surface the workspace.
async fn view_workspace(
    state: &Arc<AppState>,
    ws: &Workspace,
    repo: &crate::models::Repo,
    ws_path: &str,
) -> Result<(), ApiError> {
    let backend = state.backend.clone();
    let rn = repo.name.clone();
    let wn = ws.directory_name.clone();
    let wp = ws_path.to_string();
    let wid = ws.id.clone();
    let rid = repo.id.clone();
    let branch = ws.branch.clone();
    let root = repo.root_path.clone();
    tokio::task::spawn_blocking(move || {
        terminal::open_workspace_view(
            backend.as_ref(),
            &rn,
            &wn,
            Some(&wp),
            Some(&wid),
            Some(&rid),
            Some(&branch),
            Some(&root),
        )
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
    .map_err(ApiError)
}

#[utoipa::path(get, path = "/workspaces",
    params(
        ("repo_id" = Option<String>, Query, description = "Filter by repository ID"),
        ("status" = Option<String>, Query, description = "Filter by state: 'ready' or 'archived'"),
        ("delegated_by" = Option<String>, Query, description = "Filter to workspaces spawned by this parent ID"),
        ("since" = Option<String>, Query, description = "Only workspaces created at or after this ISO-8601 timestamp"),
    ),
    responses((status = 200, body = Vec<Workspace>), (status = 500, body = ErrorResponse)),
    operation_id = "list_workspaces", tag = "workspaces"
)]
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<Workspace>>, ApiError> {
    let state_filter = match query.status.as_deref() {
        Some(s) => Some(
            crate::models::WorkspaceState::from_db(s)
                .map_err(|e| ApiError(crate::error::BunyanError::NotFound(e)))?,
        ),
        None => None,
    };
    let conn = state.db.lock().unwrap();
    let workspaces = db::workspaces::list_filtered(
        &conn,
        &db::workspaces::ListFilters {
            repository_id: query.repo_id,
            state: state_filter,
            parent_workspace_id: query.delegated_by,
            since: query.since,
        },
    )?;
    Ok(Json(workspaces))
}

#[utoipa::path(get, path = "/workspaces/{id}", params(("id" = String, Path, description = "Workspace ID")), responses((status = 200, body = Workspace), (status = 404, body = ErrorResponse)), operation_id = "get_workspace", tag = "workspaces")]
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Workspace>, ApiError> {
    let conn = state.db.lock().unwrap();
    let ws = db::workspaces::get(&conn, &id)?;
    Ok(Json(ws))
}

#[utoipa::path(post, path = "/workspaces", request_body = CreateWorkspaceInput, responses((status = 200, body = Workspace), (status = 500, body = ErrorResponse)), operation_id = "create_workspace", tag = "workspaces")]
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateWorkspaceInput>,
) -> Result<Json<Workspace>, ApiError> {
    let repo = {
        let conn = state.db.lock().unwrap();
        db::repos::get(&conn, &input.repository_id)?
    };

    let wt_path = workspace::workspace_path(&repo.root_path, &repo.name, &input.directory_name)?;
    let repo_root = repo.root_path.clone();
    let branch = input.branch.clone();
    let container_mode = input.container_mode.clone();

    let wt_path_for_git = wt_path.clone();
    tokio::task::spawn_blocking(move || {
        let git = RealGit;
        git.worktree_add(&repo_root, &wt_path_for_git, &branch)
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
    .map_err(ApiError)?;

    let ws = {
        let conn = state.db.lock().unwrap();
        db::workspaces::create(&conn, input)?
    };

    let final_ws = if container_mode == ContainerMode::Container {
        workspace::setup_workspace_container(&state, &ws, &repo)
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e)))?
    } else {
        ws
    };

    let ws_clone = final_ws.clone();
    let repo_clone = repo.clone();
    let wt_clone = wt_path.clone();
    let bus = state.event_bus.clone();
    tokio::task::spawn_blocking(move || {
        events::fire_and_publish(
            &bus,
            names::WORKSPACE_CREATED,
            &ws_clone,
            &repo_clone,
            &wt_clone,
        );
    })
    .await
    .ok();

    Ok(Json(final_ws))
}

#[utoipa::path(post, path = "/workspaces/{id}/archive", params(("id" = String, Path, description = "Workspace ID")), responses((status = 200, body = Workspace), (status = 404, body = ErrorResponse)), operation_id = "archive_workspace", tag = "workspaces")]
pub async fn archive(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Workspace>, ApiError> {
    let (ws, repo) = {
        let conn = state.db.lock().unwrap();
        let ws = db::workspaces::get(&conn, &id)?;
        let rp = db::repos::get(&conn, &ws.repository_id)?;
        (ws, rp)
    };

    let wt_path_for_event =
        workspace::workspace_path(&repo.root_path, &repo.name, &ws.directory_name)?;
    let ws_clone = ws.clone();
    let repo_clone = repo.clone();
    let wt_clone = wt_path_for_event.clone();
    let bus = state.event_bus.clone();
    tokio::task::spawn_blocking(move || {
        events::fire_and_publish(
            &bus,
            names::WORKSPACE_ARCHIVED,
            &ws_clone,
            &repo_clone,
            &wt_clone,
        );
    })
    .await
    .ok();

    workspace::kill_workspace_window(state.backend.as_ref(), &repo.name, &ws.directory_name);

    if ws.container_mode == ContainerMode::Container {
        if let Some(ref container_id) = ws.container_id {
            let _ = docker::remove_container(container_id).await;
        }
        let remaining = {
            let conn = state.db.lock().unwrap();
            db::workspaces::count_container_workspaces(&conn, &repo.id)?
        };
        if remaining <= 1 {
            let _ = docker::remove_network(
                &docker::sanitize_docker_name(&format!("bunyan-{}", repo.name)),
            )
            .await;
        }
    }

    let wt_path = workspace::workspace_path(&repo.root_path, &repo.name, &ws.directory_name)?;
    let repo_root = repo.root_path.clone();

    tokio::task::spawn_blocking(move || {
        let git = RealGit;
        git.worktree_remove(&repo_root, &wt_path, true)
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
    .map_err(ApiError)?;

    let conn = state.db.lock().unwrap();
    let archived = db::workspaces::archive(&conn, &id)?;
    Ok(Json(archived))
}

#[utoipa::path(get, path = "/workspaces/{id}/sessions", params(("id" = String, Path, description = "Workspace ID")), responses((status = 200, body = Vec<ClaudeSessionEntry>), (status = 404, body = ErrorResponse)), tag = "workspaces")]
pub async fn get_sessions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ClaudeSessionEntry>>, ApiError> {
    let (ws, _, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let container_mode = ws.container_mode.clone();
    let dir_name = ws.directory_name.clone();
    let result = tokio::task::spawn_blocking(move || {
        sessions::read_sessions(&ws_path, &container_mode, &dir_name)
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e)))?;

    Ok(Json(result))
}

#[utoipa::path(get, path = "/workspaces/{id}/panes", params(("id" = String, Path, description = "Workspace ID")), responses((status = 200, body = Vec<TmuxPane>), (status = 404, body = ErrorResponse)), tag = "workspaces")]
pub async fn get_panes(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<TmuxPane>>, ApiError> {
    let (ws, repo, _) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let repo_name = repo.name;
    let ws_name = ws.directory_name;

    let backend = state.backend.clone();
    let processes = tokio::task::spawn_blocking(move || {
        backend.list_processes(&repo_name, &ws_name)
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
    .map_err(ApiError)?;

    let panes: Vec<TmuxPane> = processes.iter().map(|p| p.to_pane()).collect();
    Ok(Json(panes))
}

#[utoipa::path(post, path = "/workspaces/{id}/claude", params(("id" = String, Path, description = "Workspace ID")), responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)), tag = "workspaces")]
pub async fn start_claude(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<StatusResponse>, ApiError> {
    let (ws, repo, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let repo_name = repo.name.clone();
    let ws_name = ws.directory_name.clone();
    let ws_path_clone = ws_path.clone();

    let has_claude = {
        let backend = state.backend.clone();
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        tokio::task::spawn_blocking(move || backend.has_claude_running(&rn, &wn))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?
    };

    if has_claude {
        view_workspace(&state, &ws, &repo, &ws_path).await?;
        return Ok(Json(StatusResponse { status: "attached".into() }));
    }

    let has_previous = {
        let cm = ws.container_mode.clone();
        let dn = ws.directory_name.clone();
        let wp = ws_path.clone();
        tokio::task::spawn_blocking(move || sessions::has_existing_session(&wp, &cm, &dn))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
    };

    let skip_perms = ws.container_mode == ContainerMode::Container
        && workspace::should_skip_permissions(&repo);

    let base_cmd = if has_previous {
        workspace::build_claude_cmd("claude --continue", skip_perms)
    } else {
        workspace::build_claude_cmd("claude", skip_perms)
    };

    let claude_cmd = if ws.container_mode == ContainerMode::Container {
        match &ws.container_id {
            Some(cid) => docker::docker_exec_cmd(cid, &base_cmd).map_err(|e| ApiError(e))?,
            None => base_cmd,
        }
    } else {
        base_cmd
    };

    {
        let backend = state.backend.clone();
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        let wp = ws_path_clone.clone();
        let cmd = claude_cmd.clone();
        tokio::task::spawn_blocking(move || backend.spawn(&rn, &wn, &wp, &cmd))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?;
    }

    {
        let ws_clone = ws.clone();
        let repo_clone = repo.clone();
        let wp_clone = ws_path.clone();
        let bus = state.event_bus.clone();
        tokio::task::spawn_blocking(move || {
            events::fire_and_publish(
                &bus,
                names::CLAUDE_STARTED,
                &ws_clone,
                &repo_clone,
                &wp_clone,
            );
        })
        .await
        .ok();
    }

    view_workspace(&state, &ws, &repo, &ws_path).await?;

    Ok(Json(StatusResponse { status: "created".into() }))
}

#[utoipa::path(post, path = "/workspaces/{id}/claude/resume", params(("id" = String, Path, description = "Workspace ID")), request_body = ClaudeResumeInput, responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)), tag = "workspaces")]
pub async fn resume_claude(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<ClaudeResumeInput>,
) -> Result<Json<StatusResponse>, ApiError> {
    workspace::validate_session_id(&input.session_id)
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e)))?;

    let (ws, repo, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let repo_name = repo.name.clone();
    let ws_name = ws.directory_name.clone();

    let existing = {
        let backend = state.backend.clone();
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        let sid = input.session_id.clone();
        tokio::task::spawn_blocking(move || {
            backend.find_slot_running_session(&rn, &wn, &sid)
        })
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?
    };

    if existing.is_some() {
        view_workspace(&state, &ws, &repo, &ws_path).await?;
        return Ok(Json(StatusResponse { status: "attached".into() }));
    }

    let skip_perms = ws.container_mode == ContainerMode::Container
        && workspace::should_skip_permissions(&repo);
    let base_cmd = workspace::build_claude_cmd(
        &format!("claude --resume {}", input.session_id),
        skip_perms,
    );
    let claude_cmd = if ws.container_mode == ContainerMode::Container {
        match &ws.container_id {
            Some(cid) => docker::docker_exec_cmd(cid, &base_cmd).map_err(|e| ApiError(e))?,
            None => base_cmd,
        }
    } else {
        base_cmd
    };

    let idle = {
        let backend = state.backend.clone();
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        tokio::task::spawn_blocking(move || backend.find_idle_slot(&rn, &wn))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?
    };

    if let Some(slot_index) = idle {
        let backend = state.backend.clone();
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        let cmd = claude_cmd.clone();
        tokio::task::spawn_blocking(move || backend.send_to_slot(&rn, &wn, slot_index, &cmd))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?;
    } else {
        let backend = state.backend.clone();
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        let wp = ws_path.clone();
        let cmd = claude_cmd.clone();
        tokio::task::spawn_blocking(move || backend.spawn(&rn, &wn, &wp, &cmd))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?;
    }

    {
        let ws_clone = ws.clone();
        let repo_clone = repo.clone();
        let wp_clone = ws_path.clone();
        let session_id = input.session_id.clone();
        let bus = state.event_bus.clone();
        tokio::task::spawn_blocking(move || {
            events::fire_and_publish_with_extras(
                &bus,
                names::CLAUDE_RESUMED,
                &ws_clone,
                &repo_clone,
                &wp_clone,
                &[("session_id", &session_id)],
            );
        })
        .await
        .ok();
    }

    view_workspace(&state, &ws, &repo, &ws_path).await?;

    Ok(Json(StatusResponse { status: "resumed".into() }))
}

#[utoipa::path(post, path = "/workspaces/{id}/shell", params(("id" = String, Path, description = "Workspace ID")), responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)), tag = "workspaces")]
pub async fn open_shell(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<StatusResponse>, ApiError> {
    let (ws, repo, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let repo_name = repo.name.clone();
    let ws_name = ws.directory_name.clone();

    let shell_cmd = if ws.container_mode == ContainerMode::Container {
        ws.container_id
            .as_ref()
            .map(|cid| docker::docker_exec_cmd(cid, "/bin/bash"))
            .transpose()
            .map_err(ApiError)?
    } else {
        None
    };

    // For container workspaces we run docker exec ... /bin/bash as the
    // spawned command; for local workspaces we hand the backend an empty
    // string, which the tmux backend interprets as "default shell."
    let cmd = shell_cmd.unwrap_or_default();
    {
        let backend = state.backend.clone();
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        let wp = ws_path.clone();
        tokio::task::spawn_blocking(move || backend.spawn(&rn, &wn, &wp, &cmd))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?;
    }

    view_workspace(&state, &ws, &repo, &ws_path).await?;

    Ok(Json(StatusResponse { status: "created".into() }))
}

#[utoipa::path(post, path = "/workspaces/{id}/view", params(("id" = String, Path, description = "Workspace ID")), responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)), operation_id = "view_workspace", tag = "workspaces")]
pub async fn view(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<StatusResponse>, ApiError> {
    let (ws, repo, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let backend = state.backend.clone();
    let rn = repo.name.clone();
    let wn = ws.directory_name.clone();
    let wp = ws_path.clone();
    tokio::task::spawn_blocking(move || backend.ensure_workspace(&rn, &wn, &wp))
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?;

    view_workspace(&state, &ws, &repo, &ws_path).await?;

    Ok(Json(StatusResponse { status: "attached".into() }))
}

#[utoipa::path(delete, path = "/workspaces/{id}/panes/{index}", params(("id" = String, Path, description = "Workspace ID"), ("index" = u32, Path, description = "Pane index")), responses((status = 200, body = StatusResponse), (status = 404, body = ErrorResponse)), tag = "workspaces")]
pub async fn kill_pane_handler(
    State(state): State<Arc<AppState>>,
    Path((id, pane_index)): Path<(String, u32)>,
) -> Result<Json<StatusResponse>, ApiError> {
    let (ws, repo, _) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let backend = state.backend.clone();
    let rn = repo.name;
    let wn = ws.directory_name;
    tokio::task::spawn_blocking(move || backend.kill_slot(&rn, &wn, pane_index))
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?;

    Ok(Json(StatusResponse { status: "killed".into() }))
}

/// Observation endpoint: git diff for the workspace against the repo's
/// default branch. Returns plain text; empty if there are no changes.
#[utoipa::path(get, path = "/workspaces/{id}/diff",
    params(("id" = String, Path, description = "Workspace ID")),
    responses((status = 200, body = String), (status = 404, body = ErrorResponse)),
    tag = "workspaces"
)]
pub async fn diff(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<String, ApiError> {
    let (_ws, repo, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let base = repo.default_branch.clone();
    let diff = tokio::task::spawn_blocking(move || {
        let git = RealGit;
        git.worktree_diff(&ws_path, &base)
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
    .map_err(ApiError)?;

    Ok(diff)
}

/// Observation endpoint: contents of the workspace's result.json, if any.
/// v4 hooks (or the spawned agent itself) write this file at the workspace
/// root when work completes; observers read it here to learn the outcome.
#[utoipa::path(get, path = "/workspaces/{id}/result",
    params(("id" = String, Path, description = "Workspace ID")),
    responses(
        (status = 200, body = serde_json::Value),
        (status = 204, description = "No result has been written yet"),
        (status = 404, body = ErrorResponse)
    ),
    tag = "workspaces"
)]
pub async fn result(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    let (_ws, _repo, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let result_path = std::path::PathBuf::from(&ws_path).join("result.json");
    if !result_path.exists() {
        return Ok((StatusCode::NO_CONTENT, "").into_response());
    }
    let body = tokio::task::spawn_blocking(move || std::fs::read_to_string(&result_path))
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?;

    // Try to parse as JSON; if it doesn't parse, return as raw text.
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => Ok(Json(v).into_response()),
        Err(_) => Ok(body.into_response()),
    }
}
