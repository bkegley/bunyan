use std::path::PathBuf;
use tauri::State;

use crate::commands::claude;
use crate::db;
use crate::git::{GitOps, RealGit};
use crate::models::{CreateWorkspaceInput, Workspace};
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

    tokio::task::spawn_blocking(move || {
        let git = RealGit;
        git.worktree_add(&repo_root, &wt_path, &branch)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let conn = state.db.lock().unwrap();
    db::workspaces::create(&conn, input).map_err(|e| e.into())
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
