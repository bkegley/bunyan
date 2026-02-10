use std::path::PathBuf;
use tauri::State;

use crate::commands::claude;
use crate::db;
use crate::docker;
use crate::git::{GitOps, RealGit};
use crate::models::{ContainerConfig, ContainerMode, CreateWorkspaceInput, Workspace};
use crate::state::AppState;

fn workspace_path(repo_root: &str, repo_name: &str, dir_name: &str) -> Result<String, String> {
    let repo_path = PathBuf::from(repo_root);
    let base = repo_path
        .parent()
        .ok_or("Invalid repo root path")?
        .parent()
        .ok_or("Invalid repo root path")?;

    // ~/bunyan/repos/<name> -> ~/bunyan/workspaces/<name>/<dir_name>
    let path = base.join("workspaces").join(repo_name).join(dir_name);
    path.to_str()
        .ok_or_else(|| "Invalid path".to_string())
        .map(|s| s.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn list_workspaces(
    state: State<AppState>,
    repository_id: Option<String>,
) -> Result<Vec<Workspace>, String> {
    let conn = state.db.lock().unwrap();
    db::workspaces::list(&conn, repository_id.as_deref()).map_err(|e| e.into())
}

#[tauri::command]
#[specta::specta]
pub fn get_workspace(state: State<AppState>, id: String) -> Result<Workspace, String> {
    let conn = state.db.lock().unwrap();
    db::workspaces::get(&conn, &id).map_err(|e| e.into())
}

#[tauri::command]
#[specta::specta]
pub async fn create_workspace(
    state: State<'_, AppState>,
    input: CreateWorkspaceInput,
) -> Result<Workspace, String> {
    let repo = {
        let conn = state.db.lock().unwrap();
        db::repos::get(&conn, &input.repository_id).map_err(|e| e.to_string())?
    };

    let wt_path = workspace_path(&repo.root_path, &repo.name, &input.directory_name)?;
    let repo_root = repo.root_path.clone();
    let branch = input.branch.clone();
    let container_mode = input.container_mode.clone();

    tokio::task::spawn_blocking(move || {
        let git = RealGit;
        git.worktree_add(&repo_root, &wt_path, &branch)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let workspace = {
        let conn = state.db.lock().unwrap();
        db::workspaces::create(&conn, input).map_err(|e| e.to_string())?
    };

    // If container mode, create and start a Docker container
    if container_mode == ContainerMode::Container {
        let container_config = repo
            .config
            .as_ref()
            .and_then(|v| v.get("container"))
            .and_then(|v| serde_json::from_value::<ContainerConfig>(v.clone()).ok());

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

        let wt_path = workspace_path(&repo.root_path, &repo.name, &workspace.directory_name)?;
        let container_name = docker::sanitize_docker_name(&format!("bunyan-{}-{}", repo.name, workspace.directory_name));

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

        let conn = state.db.lock().unwrap();
        db::workspaces::set_container_id(&conn, &workspace.id, &container_id)
            .map_err(|e| e.to_string())?;

        // Re-fetch to get updated container_id
        return db::workspaces::get(&conn, &workspace.id).map_err(|e| e.to_string());
    }

    Ok(workspace)
}

#[tauri::command]
#[specta::specta]
pub async fn archive_workspace(
    state: State<'_, AppState>,
    id: String,
) -> Result<Workspace, String> {
    let (workspace, repo) = {
        let conn = state.db.lock().unwrap();
        let ws = db::workspaces::get(&conn, &id).map_err(|e| e.to_string())?;
        let rp = db::repos::get(&conn, &ws.repository_id).map_err(|e| e.to_string())?;
        (ws, rp)
    };

    // Kill the tmux window for this workspace (terminates all running sessions)
    claude::kill_workspace_window(&repo.name, &workspace.directory_name);

    // If container mode, stop and remove the container
    if workspace.container_mode == ContainerMode::Container {
        if let Some(ref container_id) = workspace.container_id {
            if let Err(e) = docker::remove_container(container_id).await {
                eprintln!("Warning: failed to remove container {}: {}", container_id, e);
            }
        }

        // Clean up the network if no other container workspaces remain for this repo.
        // We check *before* archiving since the current workspace is still "ready".
        // Subtract 1 because the current workspace hasn't been archived yet.
        let remaining = {
            let conn = state.db.lock().unwrap();
            db::workspaces::count_container_workspaces(&conn, &repo.id)
                .map_err(|e| e.to_string())?
        };
        if remaining <= 1 {
            let _ = docker::remove_network(&docker::sanitize_docker_name(&format!("bunyan-{}", repo.name))).await;
        }
    }

    let wt_path = workspace_path(&repo.root_path, &repo.name, &workspace.directory_name)?;
    let repo_root = repo.root_path.clone();

    tokio::task::spawn_blocking(move || {
        let git = RealGit;
        git.worktree_remove(&repo_root, &wt_path)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let conn = state.db.lock().unwrap();
    db::workspaces::archive(&conn, &id).map_err(|e| e.into())
}
