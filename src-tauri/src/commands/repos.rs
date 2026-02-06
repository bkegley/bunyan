use tauri::State;

use crate::db;
use crate::git::{GitOps, RealGit};
use crate::models::{CreateRepoInput, Repo, UpdateRepoInput};
use crate::state::AppState;

#[tauri::command]
pub fn list_repos(state: State<AppState>) -> Result<Vec<Repo>, String> {
    let conn = state.db.lock().unwrap();
    db::repos::list(&conn).map_err(|e| e.into())
}

#[tauri::command]
pub fn get_repo(state: State<AppState>, id: String) -> Result<Repo, String> {
    let conn = state.db.lock().unwrap();
    db::repos::get(&conn, &id).map_err(|e| e.into())
}

#[tauri::command]
pub fn create_repo(state: State<AppState>, input: CreateRepoInput) -> Result<Repo, String> {
    let git = RealGit;
    git.clone_repo(&input.remote_url, &input.root_path)
        .map_err(|e| e.to_string())?;

    let conn = state.db.lock().unwrap();
    db::repos::create(&conn, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn update_repo(state: State<AppState>, input: UpdateRepoInput) -> Result<Repo, String> {
    let conn = state.db.lock().unwrap();
    db::repos::update(&conn, input).map_err(|e| e.into())
}

#[tauri::command]
pub fn delete_repo(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().unwrap();
    db::repos::delete(&conn, &id).map_err(|e| e.into())
}
