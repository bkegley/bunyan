use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::db;
use crate::git::{GitOps, RealGit};
use crate::models::{CreateRepoInput, Repo, UpdateRepoInput};
use crate::server::error::ApiError;
use crate::state::AppState;

pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Repo>>, ApiError> {
    let conn = state.db.lock().unwrap();
    let repos = db::repos::list(&conn)?;
    Ok(Json(repos))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Repo>, ApiError> {
    let conn = state.db.lock().unwrap();
    let repo = db::repos::get(&conn, &id)?;
    Ok(Json(repo))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateRepoInput>,
) -> Result<Json<Repo>, ApiError> {
    let url = input.remote_url.clone();
    let path = input.root_path.clone();
    tokio::task::spawn_blocking(move || {
        let git = RealGit;
        git.clone_repo(&url, &path)
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?
    .map_err(ApiError)?;

    let conn = state.db.lock().unwrap();
    let repo = db::repos::create(&conn, input)?;
    Ok(Json(repo))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(mut input): Json<UpdateRepoInput>,
) -> Result<Json<Repo>, ApiError> {
    input.id = id;
    let conn = state.db.lock().unwrap();
    let repo = db::repos::update(&conn, input)?;
    Ok(Json(repo))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<()>, ApiError> {
    let conn = state.db.lock().unwrap();
    db::repos::delete(&conn, &id)?;
    Ok(Json(()))
}
