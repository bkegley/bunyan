use tauri::State;

use crate::db;
use crate::git::{GitOps, RealGit};
use crate::models::{CreateRepoInput, Repo, UpdateRepoInput};
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn list_repos(state: State<AppState>) -> Result<Vec<Repo>, String> {
    let conn = state.db.lock().unwrap();
    db::repos::list(&conn).map_err(|e| e.into())
}

#[tauri::command]
#[specta::specta]
pub fn get_repo(state: State<AppState>, id: String) -> Result<Repo, String> {
    let conn = state.db.lock().unwrap();
    db::repos::get(&conn, &id).map_err(|e| e.into())
}

#[tauri::command]
#[specta::specta]
pub async fn create_repo(
    state: State<'_, AppState>,
    input: CreateRepoInput,
) -> Result<Repo, String> {
    let url = input.remote_url.clone();
    let path = input.root_path.clone();
    tokio::task::spawn_blocking(move || {
        let git = RealGit;
        git.clone_repo(&url, &path)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let conn = state.db.lock().unwrap();
    db::repos::create(&conn, input).map_err(|e| e.into())
}

#[tauri::command]
#[specta::specta]
pub fn update_repo(state: State<AppState>, input: UpdateRepoInput) -> Result<Repo, String> {
    let conn = state.db.lock().unwrap();
    db::repos::update(&conn, input).map_err(|e| e.into())
}

#[tauri::command]
#[specta::specta]
pub fn delete_repo(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::repos::delete(&conn, &id).map_err(|e| e.into())
}
