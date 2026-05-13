//! Integration tests for GET /events (Server-Sent Events).
//!
//! We boot a real TCP server, hit /events with an HTTP request, then drive
//! routes (POST /workspaces, etc.) that publish to the bus. The SSE
//! response body should contain the matching `event:` and `data:` lines.

#![cfg(feature = "server")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use bunyan_core::db;
use bunyan_core::models::{ContainerMode, CreateRepoInput, CreateWorkspaceInput};
use bunyan_core::server::build_router;
use bunyan_core::state::AppState;
use rusqlite::Connection;

fn unique_tempdir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "bunyan-events-stream-test-{}-{}-{}",
        label,
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn init_git_repo(path: &Path) {
    fs::create_dir_all(path).unwrap();
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@bunyan.local"])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Bunyan Test"])
        .current_dir(path)
        .output()
        .unwrap();
    fs::write(path.join("README.md"), "init\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(path)
        .output()
        .unwrap();
}

#[tokio::test]
async fn events_stream_emits_workspace_archived_event() {
    let root = unique_tempdir("sse_archive");
    let repos_dir = root.join("repos");
    let workspaces_dir = root.join("workspaces");
    fs::create_dir_all(&repos_dir).unwrap();
    fs::create_dir_all(&workspaces_dir).unwrap();
    let repo_path = repos_dir.join("myrepo");
    init_git_repo(&repo_path);

    let conn = Connection::open_in_memory().unwrap();
    db::initialize_database(&conn).unwrap();
    let state = Arc::new(AppState::new(conn));

    // Pre-seed a repo + a workspace that points at a real worktree on disk
    // (so the archive route's git operations don't fail). We add a real
    // worktree pointing at workspaces_dir/myrepo/ws.
    let wt = workspaces_dir.join("myrepo").join("ws");
    Command::new("git")
        .args([
            "worktree",
            "add",
            wt.to_str().unwrap(),
            "-b",
            "ws-branch",
        ])
        .current_dir(&repo_path)
        .output()
        .unwrap();

    let ws_id = {
        let conn = state.db.lock().unwrap();
        let r = db::repos::create(
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
        db::workspaces::create(
            &conn,
            CreateWorkspaceInput {
                repository_id: r.id,
                directory_name: "ws".into(),
                branch: "ws-branch".into(),
                container_mode: ContainerMode::Local,
            },
        )
        .unwrap()
        .id
    };

    // Bind to a random port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = build_router(state.clone());

    let server_task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Subscribe to /events with a regular HTTP GET. Read incrementally.
    let client = reqwest::Client::new();
    let mut response = client
        .get(format!("http://{}/events", addr))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Drive an archive in the background so the SSE stream sees the event.
    let archive_addr = addr;
    let archive_ws = ws_id.clone();
    let driver = tokio::spawn(async move {
        // Give the SSE subscriber a beat to be registered on the bus.
        tokio::time::sleep(Duration::from_millis(150)).await;
        reqwest::Client::new()
            .post(format!(
                "http://{}/workspaces/{}/archive",
                archive_addr, archive_ws
            ))
            .send()
            .await
            .unwrap();
    });

    // Read chunks until we see workspace.archived or timeout at 3s.
    let mut buffer = String::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let found = loop {
        if std::time::Instant::now() >= deadline {
            break false;
        }
        match tokio::time::timeout(Duration::from_millis(200), response.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                if buffer.contains("event: workspace.archived") {
                    break true;
                }
            }
            Ok(Ok(None)) => break false,
            Ok(Err(_)) | Err(_) => continue,
        }
    };

    driver.await.unwrap();
    server_task.abort();
    let _ = server_task.await;

    assert!(
        found,
        "expected SSE to deliver workspace.archived; saw:\n{}",
        buffer
    );
    assert!(
        buffer.contains("\"event\":\"workspace.archived\""),
        "expected JSON event field in data: line"
    );
}
