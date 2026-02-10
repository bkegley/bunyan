use std::path::PathBuf;

use std::sync::Arc;

use rusqlite::Connection;

use crate::db;
use crate::docker;
use crate::error::{BunyanError, Result};
use crate::models::{ContainerConfig, Repo, Workspace};
use crate::state::AppState;
use crate::tmux;

/// Derive the workspace filesystem path from a repo's root path.
/// ~/bunyan/repos/<name> -> ~/bunyan/workspaces/<name>/<dir_name>
pub fn workspace_path(repo_root: &str, repo_name: &str, dir_name: &str) -> Result<String> {
    let repo_path = PathBuf::from(repo_root);
    let base = repo_path
        .parent()
        .ok_or_else(|| BunyanError::Git("Invalid repo root path".to_string()))?
        .parent()
        .ok_or_else(|| BunyanError::Git("Invalid repo root path".to_string()))?;

    let path = base.join("workspaces").join(repo_name).join(dir_name);
    path.to_str()
        .ok_or_else(|| BunyanError::Git("Invalid path".to_string()))
        .map(|s| s.to_string())
}

/// Resolve workspace, repo, and filesystem path from a workspace ID.
pub fn resolve_workspace_path(
    conn: &Connection,
    workspace_id: &str,
) -> Result<(Workspace, Repo, String)> {
    let ws = db::workspaces::get(conn, workspace_id)?;
    let rp = db::repos::get(conn, &ws.repository_id)?;
    let ws_path = workspace_path(&rp.root_path, &rp.name, &ws.directory_name)?;
    Ok((ws, rp, ws_path))
}

/// Kill the entire tmux window for a workspace (used before archiving).
pub fn kill_workspace_window(repo_name: &str, workspace_name: &str) {
    let _ = tmux::kill_window(repo_name, workspace_name);
}

/// Extract container config from a repo's JSON config blob.
pub fn get_container_config(repo: &Repo) -> Option<ContainerConfig> {
    repo.config
        .as_ref()
        .and_then(|v| v.get("container"))
        .and_then(|v| serde_json::from_value::<ContainerConfig>(v.clone()).ok())
}

/// Check if dangerously_skip_permissions is enabled in the repo's container config.
pub fn should_skip_permissions(repo: &Repo) -> bool {
    get_container_config(repo)
        .map(|c| c.dangerously_skip_permissions)
        .unwrap_or(false)
}

/// Build a claude command string, optionally adding --dangerously-skip-permissions.
pub fn build_claude_cmd(base: &str, skip_permissions: bool) -> String {
    if skip_permissions {
        format!("{} --dangerously-skip-permissions", base)
    } else {
        base.to_string()
    }
}

/// Validate that a session ID is a safe UUID-like string (hex + dashes + underscores).
pub fn validate_session_id(id: &str) -> std::result::Result<(), String> {
    if id.is_empty() {
        return Err("Empty session ID".to_string());
    }
    let is_valid = id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !is_valid {
        return Err(format!("Invalid session ID: {}", id));
    }
    Ok(())
}

/// Create a workspace container (Docker container setup for container-mode workspaces).
/// Returns the updated workspace with container_id set.
/// Takes Arc<AppState> to avoid holding MutexGuard across await points.
pub async fn setup_workspace_container(
    state: &Arc<AppState>,
    workspace: &Workspace,
    repo: &Repo,
) -> std::result::Result<Workspace, String> {
    let container_config = get_container_config(repo);

    let image = container_config
        .as_ref()
        .and_then(|c| c.image.clone())
        .unwrap_or_else(|| "node:22".to_string());
    let ports = container_config
        .as_ref()
        .and_then(|c| c.ports.clone())
        .unwrap_or_default();
    let env: Vec<String> = container_config
        .as_ref()
        .and_then(|c| c.env.clone())
        .map(|m| m.into_iter().map(|(k, v)| format!("{}={}", k, v)).collect())
        .unwrap_or_default();

    let wt_path = workspace_path(&repo.root_path, &repo.name, &workspace.directory_name)
        .map_err(|e| e.to_string())?;
    let container_name = docker::sanitize_docker_name(
        &format!("bunyan-{}-{}", repo.name, workspace.directory_name),
    );

    let network_name = docker::sanitize_docker_name(&format!("bunyan-{}", repo.name));
    docker::create_network(&network_name)
        .await
        .map_err(|e| e.to_string())?;

    let container_id = docker::create_workspace_container(
        &image,
        &wt_path,
        &container_name,
        &ports,
        &env,
        Some(&network_name),
        &workspace.directory_name,
    )
    .await
    .map_err(|e| e.to_string())?;

    // Best-effort: install claude in the container
    if let Err(e) = docker::ensure_claude(&container_id).await {
        eprintln!("Warning: could not install Claude in container: {}", e);
    }

    // Lock only for DB operations, not across await
    let conn = state.db.lock().unwrap();
    db::workspaces::set_container_id(&conn, &workspace.id, &container_id)
        .map_err(|e| e.to_string())?;

    db::workspaces::get(&conn, &workspace.id).map_err(|e| e.to_string())
}
