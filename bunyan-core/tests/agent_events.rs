//! Integration tests for POST /workspaces/:id/agent-events.
//!
//! This endpoint is what spawned Claudes post into via their injected
//! settings.local.json. We exercise the parsing, the result.json writeback
//! on Stop, and the bunyan-event re-firing path (via a per-repo hook that
//! receives the synthetic claude.stopped event).

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
use bunyan_core::models::{ContainerMode, CreateRepoInput, CreateWorkspaceInput};
use bunyan_core::server::build_router;
use bunyan_core::state::AppState;

fn unique_tempdir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "bunyan-agent-events-test-{}-{}-{}",
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

fn seed(state: &Arc<AppState>, repo_path: &Path) -> (String, String) {
    let conn = state.db.lock().unwrap();
    let repo = db::repos::create(
        &conn,
        CreateRepoInput {
            name: "myrepo".into(),
            remote_url: "u".into(),
            root_path: repo_path.display().to_string(),
            default_branch: "main".into(),
            remote: "origin".into(),
            display_order: 0,
            config: None,
        },
    )
    .unwrap();
    let ws = db::workspaces::create(
        &conn,
        CreateWorkspaceInput {
            repository_id: repo.id.clone(),
            directory_name: "ws".into(),
            branch: "ws".into(),
            container_mode: ContainerMode::Local,
        },
    )
    .unwrap();
    (repo.id, ws.id)
}

#[tokio::test]
async fn post_agent_events_with_stop_writes_result_json() {
    // Build a temp repo+workspace shape that workspace_path() can resolve.
    let root = unique_tempdir("stop_result");
    let repos_dir = root.join("repos");
    let workspaces_dir = root.join("workspaces");
    fs::create_dir_all(&repos_dir).unwrap();
    fs::create_dir_all(&workspaces_dir).unwrap();
    let repo_path = repos_dir.join("myrepo");
    fs::create_dir_all(&repo_path).unwrap();
    let wt = workspaces_dir.join("myrepo").join("ws");
    fs::create_dir_all(&wt).unwrap();

    let state = make_state();
    let (_, ws_id) = seed(&state, &repo_path);
    let app = build_router(state);

    let payload = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "abc-123",
        "stop_hook_active": true,
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workspaces/{}/agent-events", ws_id))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    let result_path = wt.join("result.json");
    assert!(result_path.exists(), "Stop should have written result.json");
    let body: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&result_path).unwrap()).unwrap();
    assert_eq!(body["status"], "stopped");
    assert_eq!(body["hook_event_name"], "Stop");
    assert_eq!(body["raw"]["session_id"], "abc-123");
}

#[tokio::test]
async fn post_agent_events_with_notification_does_not_write_result_json() {
    let root = unique_tempdir("notify_no_result");
    let repos_dir = root.join("repos");
    let workspaces_dir = root.join("workspaces");
    fs::create_dir_all(&repos_dir).unwrap();
    fs::create_dir_all(&workspaces_dir).unwrap();
    let repo_path = repos_dir.join("myrepo");
    fs::create_dir_all(&repo_path).unwrap();
    let wt = workspaces_dir.join("myrepo").join("ws");
    fs::create_dir_all(&wt).unwrap();

    let state = make_state();
    let (_, ws_id) = seed(&state, &repo_path);
    let app = build_router(state);

    let payload = serde_json::json!({"hook_event_name": "Notification"});
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workspaces/{}/agent-events", ws_id))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    assert!(!wt.join("result.json").exists());
}

#[tokio::test]
async fn post_agent_events_fires_bunyan_event_via_per_repo_hook() {
    // Drop a per-repo hook for claude.stopped at <repo>/.bunyan/hooks/.
    // After we POST a Stop event, the hook should have fired and written
    // a marker file. This proves the agent event got re-fired into
    // bunyan's hook system, which is what gives Slack/Tauri/etc. their
    // notification surface.
    let root = unique_tempdir("refire_hook");
    let repos_dir = root.join("repos");
    let workspaces_dir = root.join("workspaces");
    fs::create_dir_all(&repos_dir).unwrap();
    fs::create_dir_all(&workspaces_dir).unwrap();
    let repo_path = repos_dir.join("myrepo");
    fs::create_dir_all(&repo_path).unwrap();
    let wt = workspaces_dir.join("myrepo").join("ws");
    fs::create_dir_all(&wt).unwrap();
    let marker = root.join("hook-fired");
    write_hook(
        &repo_path.join(".bunyan/hooks/claude.stopped"),
        &format!(
            "#!/bin/sh\necho \"$BUNYAN_CLAUDE_EVENT\" > {}\n",
            marker.display()
        ),
    );

    let state = make_state();
    let (_, ws_id) = seed(&state, &repo_path);
    let app = build_router(state);

    let payload = serde_json::json!({"hook_event_name": "Stop"});
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workspaces/{}/agent-events", ws_id))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    // The hook runs on a tokio-blocking task fired off internally; give it
    // a moment to land. (Realistically <50ms; the test sleeps to be safe.)
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(
        marker.exists(),
        "expected claude.stopped hook to have fired and written marker"
    );
    let content = fs::read_to_string(&marker).unwrap();
    assert_eq!(content.trim(), "Stop");
}

#[tokio::test]
async fn post_agent_events_accepts_malformed_body() {
    let root = unique_tempdir("malformed");
    let repos_dir = root.join("repos");
    let workspaces_dir = root.join("workspaces");
    fs::create_dir_all(&repos_dir).unwrap();
    fs::create_dir_all(&workspaces_dir).unwrap();
    let repo_path = repos_dir.join("myrepo");
    fs::create_dir_all(&repo_path).unwrap();
    let wt = workspaces_dir.join("myrepo").join("ws");
    fs::create_dir_all(&wt).unwrap();

    let state = make_state();
    let (_, ws_id) = seed(&state, &repo_path);
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workspaces/{}/agent-events", ws_id))
                .header("content-type", "application/json")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
}
