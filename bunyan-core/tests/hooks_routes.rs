//! End-to-end tests for the hooks HTTP surface.
//!
//! Spins up the axum router against an in-memory SQLite DB, makes real HTTP
//! requests to `/hooks` and `/hooks/run`, and exercises both the discovery
//! and execution paths.
//!
//! All hooks here live in the per-repo discovery path (`<repo>/.bunyan/hooks`)
//! to avoid touching `XDG_CONFIG_HOME` from concurrent tests.

#![cfg(feature = "server")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rusqlite::Connection;
use tower::ServiceExt;

use bunyan_core::db;
use bunyan_core::models::{CreateRepoInput, CreateWorkspaceInput};
use bunyan_core::server::build_router;
use bunyan_core::state::AppState;

fn unique_tempdir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "bunyan-int-test-{}-{}-{}",
        label,
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_hook(path: &Path, script: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, script).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

fn make_state() -> Arc<AppState> {
    let conn = Connection::open_in_memory().unwrap();
    db::initialize_database(&conn).unwrap();
    Arc::new(AppState::new(conn))
}

async fn body_json(resp: axum::http::Response<Body>) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn seed_repo(state: &Arc<AppState>, name: &str, root_path: &Path) -> String {
    let conn = state.db.lock().unwrap();
    db::repos::create(
        &conn,
        CreateRepoInput {
            name: name.into(),
            remote_url: "u".into(),
            root_path: root_path.display().to_string(),
            default_branch: "main".into(),
            remote: "origin".into(),
            display_order: 0,
            config: None,
        },
    )
    .unwrap()
    .id
}

fn seed_workspace(state: &Arc<AppState>, repo_id: &str, dir_name: &str) -> String {
    let conn = state.db.lock().unwrap();
    db::workspaces::create(
        &conn,
        CreateWorkspaceInput {
            repository_id: repo_id.to_string(),
            directory_name: dir_name.into(),
            branch: dir_name.into(),
            container_mode: bunyan_core::models::ContainerMode::Local,
        },
    )
    .unwrap()
    .id
}

#[tokio::test]
async fn get_hooks_lists_per_repo_hook() {
    let repo_root = unique_tempdir("hooks_route_repo");
    write_hook(
        &repo_root.join(".bunyan/hooks/workspace.created"),
        "#!/bin/sh\nexit 0\n",
    );

    let state = make_state();
    seed_repo(&state, "myrepo", &repo_root);
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/hooks?event=workspace.created&repo=myrepo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp).await;
    assert_eq!(body["event"], "workspace.created");
    let hooks = body["hooks"].as_array().unwrap();
    assert!(
        hooks.iter().any(|h| h.as_str().unwrap().contains(".bunyan/hooks/workspace.created")),
        "expected to find per-repo hook in {:?}",
        hooks
    );
}

#[tokio::test]
async fn post_hooks_run_with_workspace_executes_per_repo_hook() {
    let repo_root = unique_tempdir("hooks_run_per_repo");
    let marker = repo_root.join("marker.txt");
    write_hook(
        &repo_root.join(".bunyan/hooks/smoke.test"),
        &format!(
            "#!/bin/sh\necho \"$BUNYAN_EVENT=$BUNYAN_REPO/$BUNYAN_WORKSPACE\" > {}\nexit 0\n",
            marker.display()
        ),
    );

    let state = make_state();
    let repo_id = seed_repo(&state, "myrepo", &repo_root);
    let ws_id = seed_workspace(&state, &repo_id, "ws-1");
    let app = build_router(state);

    let body = serde_json::json!({
        "event": "smoke.test",
        "workspace_id": ws_id,
        "extras": {}
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/hooks/run")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp).await;
    assert_eq!(body["event"], "smoke.test");
    let outcomes = body["outcomes"].as_array().unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0]["exit_code"], 0);
    assert_eq!(outcomes[0]["succeeded"], true);

    let content = fs::read_to_string(&marker).unwrap();
    assert_eq!(content.trim(), "smoke.test=myrepo/ws-1");
}

#[tokio::test]
async fn post_hooks_run_with_extras_propagates_to_hook_env() {
    let repo_root = unique_tempdir("hooks_run_extras");
    let marker = repo_root.join("marker");
    write_hook(
        &repo_root.join(".bunyan/hooks/my.event"),
        &format!(
            "#!/bin/sh\necho \"$BUNYAN_TAG-$BUNYAN_LEVEL\" > {}\nexit 0\n",
            marker.display()
        ),
    );

    let state = make_state();
    let repo_id = seed_repo(&state, "myrepo", &repo_root);
    let ws_id = seed_workspace(&state, &repo_id, "ws-1");
    let app = build_router(state);

    let body = serde_json::json!({
        "event": "my.event",
        "workspace_id": ws_id,
        "extras": { "tag": "alpha", "level": "high" }
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/hooks/run")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let content = fs::read_to_string(&marker).expect("marker should exist");
    assert_eq!(content.trim(), "alpha-high");
}

#[tokio::test]
async fn post_hooks_run_with_failing_hook_returns_exit_code_and_succeeded_false() {
    let repo_root = unique_tempdir("hooks_run_fail");
    write_hook(
        &repo_root.join(".bunyan/hooks/my.event"),
        "#!/bin/sh\necho oops 1>&2\nexit 7\n",
    );

    let state = make_state();
    let repo_id = seed_repo(&state, "myrepo", &repo_root);
    let ws_id = seed_workspace(&state, &repo_id, "ws-1");
    let app = build_router(state);

    let body = serde_json::json!({
        "event": "my.event",
        "workspace_id": ws_id,
        "extras": {}
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/hooks/run")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp).await;
    let outcomes = body["outcomes"].as_array().unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0]["exit_code"], 7);
    assert_eq!(outcomes[0]["succeeded"], false);
    assert!(outcomes[0]["stderr"].as_str().unwrap().contains("oops"));
}
