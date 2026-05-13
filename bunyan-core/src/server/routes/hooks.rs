use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::events;
use crate::hooks::{self, DefaultHookRoots, HookContext};
use crate::models::ErrorResponse;
use crate::server::error::ApiError;
use crate::state::AppState;
use crate::workspace;

#[derive(Deserialize)]
pub struct ListQuery {
    pub event: String,
    /// Optional repo *name* to include per-repo hooks in the discovery.
    pub repo: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct HookListResponse {
    pub event: String,
    pub hooks: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/hooks",
    params(
        ("event" = String, Query, description = "Event name (e.g. workspace.ready_to_view)"),
        ("repo" = Option<String>, Query, description = "Optional repo name to include per-repo hooks")
    ),
    responses((status = 200, body = HookListResponse), (status = 500, body = ErrorResponse)),
    tag = "hooks"
)]
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<HookListResponse>, ApiError> {
    // Resolve the repo's root_path so DefaultHookRoots can find per-repo hooks.
    let repo_root: Option<String> = match query.repo.as_deref() {
        Some(name) => {
            let conn = state.db.lock().unwrap();
            db::repos::list(&conn)?
                .into_iter()
                .find(|r| r.name == name)
                .map(|r| r.root_path)
        }
        None => None,
    };

    let event = query.event.clone();
    let repo_name = query.repo.clone();
    let hooks_found = tokio::task::spawn_blocking(move || {
        let roots = DefaultHookRoots::new(repo_root.map(std::path::PathBuf::from));
        hooks::discover_hooks(&roots, &event, repo_name.as_deref())
            .into_iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?;

    Ok(Json(HookListResponse {
        event: query.event,
        hooks: hooks_found,
    }))
}

#[derive(Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RunInput {
    pub event: String,
    pub workspace_id: Option<String>,
    /// Optional event-specific extras, exposed as `BUNYAN_<UPPER_KEY>` env vars.
    #[serde(default)]
    pub extras: std::collections::HashMap<String, String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct HookOutcomeJson {
    pub path: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub succeeded: bool,
}

#[derive(Serialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RunResponse {
    pub event: String,
    pub outcomes: Vec<HookOutcomeJson>,
}

#[utoipa::path(
    post,
    path = "/hooks/run",
    request_body = RunInput,
    responses((status = 200, body = RunResponse), (status = 404, body = ErrorResponse), (status = 500, body = ErrorResponse)),
    tag = "hooks"
)]
pub async fn run(
    State(state): State<Arc<AppState>>,
    Json(input): Json<RunInput>,
) -> Result<Json<RunResponse>, ApiError> {
    // Resolve workspace context if given; otherwise fire an empty event.
    let ctx_data: Option<(crate::models::Workspace, crate::models::Repo, String)> =
        match input.workspace_id.as_deref() {
            Some(id) => {
                let conn = state.db.lock().unwrap();
                Some(workspace::resolve_workspace_path(&conn, id)?)
            }
            None => None,
        };

    let event = input.event.clone();
    let extras = input.extras.clone();
    let result = tokio::task::spawn_blocking(move || {
        let (ctx, roots): (HookContext, DefaultHookRoots) = match ctx_data {
            Some((ws, repo, ws_path)) => {
                let mut c = events::context_for(&event, &ws, &repo, &ws_path);
                for (k, v) in extras {
                    c = c.with_extra(k, v);
                }
                let r = events::roots_for(&repo);
                (c, r)
            }
            None => {
                let mut c = HookContext::new(&event);
                for (k, v) in extras {
                    c = c.with_extra(k, v);
                }
                (c, DefaultHookRoots::new(None))
            }
        };
        hooks::fire(&roots, &ctx)
    })
    .await
    .map_err(|e| ApiError(crate::error::BunyanError::Process(e.to_string())))?;

    let outcomes = result
        .outcomes
        .into_iter()
        .map(|o| HookOutcomeJson {
            path: o.path.display().to_string(),
            exit_code: o.exit_code,
            duration_ms: o.duration.as_millis(),
            stdout: o.stdout,
            stderr: o.stderr,
            timed_out: o.timed_out,
            succeeded: o.exit_code == Some(0) || o.exit_code == Some(78),
        })
        .collect();

    Ok(Json(RunResponse {
        event: input.event,
        outcomes,
    }))
}
