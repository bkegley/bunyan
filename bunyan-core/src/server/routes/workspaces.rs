use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::db;
use crate::docker;
use crate::git::{GitOps, RealGit};
use crate::models::{
    ClaudeSessionEntry, ContainerMode, CreateWorkspaceInput, TmuxPane, Workspace,
};
use crate::server::error::ApiError;
use crate::sessions;
use crate::state::AppState;
use crate::terminal;
use crate::tmux;
use crate::workspace;

#[derive(Deserialize)]
pub struct ListQuery {
    pub repo_id: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<Workspace>>, ApiError> {
    let conn = state.db.lock().unwrap();
    let workspaces = db::workspaces::list(&conn, query.repo_id.as_deref())?;
    Ok(Json(workspaces))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Workspace>, ApiError> {
    let conn = state.db.lock().unwrap();
    let ws = db::workspaces::get(&conn, &id)?;
    Ok(Json(ws))
}

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

    tokio::task::spawn_blocking(move || {
        let git = RealGit;
        git.worktree_add(&repo_root, &wt_path, &branch)
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
    .map_err(ApiError)?;

    let ws = {
        let conn = state.db.lock().unwrap();
        db::workspaces::create(&conn, input)?
    };

    if container_mode == ContainerMode::Container {
        let updated = workspace::setup_workspace_container(&state, &ws, &repo)
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e)))?;
        return Ok(Json(updated));
    }

    Ok(Json(ws))
}

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

    workspace::kill_workspace_window(&repo.name, &ws.directory_name);

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

    let panes = tokio::task::spawn_blocking(move || tmux::list_panes(&repo_name, &ws_name))
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?;

    Ok(Json(panes))
}

#[derive(Deserialize)]
pub struct ClaudeResumeInput {
    pub session_id: String,
}

pub async fn start_claude(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (ws, repo, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let repo_name = repo.name.clone();
    let ws_name = ws.directory_name.clone();
    let ws_path_clone = ws_path.clone();

    let has_claude = tokio::task::spawn_blocking({
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        move || tmux::has_claude_running(&rn, &wn)
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
    .map_err(ApiError)?;

    if has_claude {
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        tokio::task::spawn_blocking(move || terminal::attach_iterm(&rn, &wn))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?;
        return Ok(Json(serde_json::json!({ "status": "attached" })));
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

    let rn = repo_name.clone();
    let wn = ws_name.clone();
    let wp = ws_path_clone.clone();
    let cmd = claude_cmd.clone();
    tokio::task::spawn_blocking(move || tmux::create_pane(&rn, &wn, &wp, &cmd))
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?;

    let rn = repo_name.clone();
    let wn = ws_name.clone();
    tokio::task::spawn_blocking(move || terminal::attach_iterm(&rn, &wn))
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({ "status": "created" })))
}

pub async fn resume_claude(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<ClaudeResumeInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    workspace::validate_session_id(&input.session_id)
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e)))?;

    let (ws, repo, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let repo_name = repo.name.clone();
    let ws_name = ws.directory_name.clone();

    let existing = {
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        let sid = input.session_id.clone();
        tokio::task::spawn_blocking(move || tmux::find_pane_with_session(&rn, &wn, &sid))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?
    };

    if existing.is_some() {
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        tokio::task::spawn_blocking(move || terminal::attach_iterm(&rn, &wn))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?;
        return Ok(Json(serde_json::json!({ "status": "attached" })));
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
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        tokio::task::spawn_blocking(move || tmux::find_idle_pane(&rn, &wn))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?
    };

    if let Some(pane_index) = idle {
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        let cmd = claude_cmd.clone();
        tokio::task::spawn_blocking(move || tmux::send_to_pane(&rn, &wn, pane_index, &cmd))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?;
    } else {
        let rn = repo_name.clone();
        let wn = ws_name.clone();
        let wp = ws_path.clone();
        let cmd = claude_cmd.clone();
        tokio::task::spawn_blocking(move || tmux::create_pane(&rn, &wn, &wp, &cmd))
            .await
            .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
            .map_err(ApiError)?;
    }

    let rn = repo_name.clone();
    let wn = ws_name.clone();
    tokio::task::spawn_blocking(move || terminal::attach_iterm(&rn, &wn))
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({ "status": "resumed" })))
}

pub async fn open_shell(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
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

    let rn = repo_name.clone();
    let wn = ws_name.clone();
    let wp = ws_path.clone();
    tokio::task::spawn_blocking(move || {
        tmux::ensure_workspace_window(&rn, &wn, &wp)?;
        let target = format!("{}:{}", rn, wn);
        let mut args = vec![
            "-L", "bunyan", "split-window", "-h", "-t", &target, "-c", &wp,
        ];
        let cmd_ref;
        if let Some(ref cmd) = shell_cmd {
            cmd_ref = cmd.as_str();
            args.push(cmd_ref);
        }
        let output = std::process::Command::new("tmux")
            .args(&args)
            .output()
            .map_err(|e| {
                crate::error::BunyanError::Process(format!("Failed to split window: {}", e))
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(crate::error::BunyanError::Process(format!(
                "tmux split-window failed: {}",
                stderr
            )));
        }
        Ok(())
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
    .map_err(ApiError)?;

    let rn = repo_name.clone();
    let wn = ws_name.clone();
    tokio::task::spawn_blocking(move || terminal::attach_iterm(&rn, &wn))
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({ "status": "created" })))
}

pub async fn view(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (ws, repo, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let rn = repo.name.clone();
    let wn = ws.directory_name.clone();
    let wp = ws_path;
    tokio::task::spawn_blocking(move || tmux::ensure_workspace_window(&rn, &wn, &wp))
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?;

    let rn = repo.name.clone();
    let wn = ws.directory_name.clone();
    tokio::task::spawn_blocking(move || terminal::attach_iterm(&rn, &wn))
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({ "status": "attached" })))
}

pub async fn kill_pane_handler(
    State(state): State<Arc<AppState>>,
    Path((id, pane_index)): Path<(String, u32)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (ws, repo, _) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    let rn = repo.name;
    let wn = ws.directory_name;
    tokio::task::spawn_blocking(move || tmux::kill_pane(&rn, &wn, pane_index))
        .await
        .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({ "status": "killed" })))
}
