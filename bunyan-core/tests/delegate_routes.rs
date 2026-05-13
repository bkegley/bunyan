//! Integration tests for POST /delegate and the observation endpoints.
//!
//! These tests stand up a temp git repo (so git worktree add succeeds),
//! register it with the daemon, and exercise the value-prop endpoint.
//! A `FakeBackend` replaces tmux so `claude` is never actually spawned.

#![cfg(feature = "server")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rusqlite::Connection;
use tower::ServiceExt;

use bunyan_core::backends::{ProcessInfo, RuntimeBackend};
use bunyan_core::db;
use bunyan_core::error::Result;
use bunyan_core::models::CreateRepoInput;
use bunyan_core::server::build_router;
use bunyan_core::state::AppState;

#[derive(Default)]
struct FakeBackend {
    spawned: Mutex<Vec<(String, String, String, String)>>,
}

impl RuntimeBackend for FakeBackend {
    fn name(&self) -> &'static str {
        "fake"
    }
    fn ensure_workspace(&self, _: &str, _: &str, _: &str) -> Result<()> {
        Ok(())
    }
    fn kill_workspace(&self, _: &str, _: &str) -> Result<()> {
        Ok(())
    }
    fn list_processes(&self, _: &str, _: &str) -> Result<Vec<ProcessInfo>> {
        Ok(vec![])
    }
    fn list_all_processes(&self) -> Result<Vec<(String, String, ProcessInfo)>> {
        Ok(vec![])
    }
    fn spawn(&self, repo: &str, ws: &str, path: &str, cmd: &str) -> Result<()> {
        self.spawned.lock().unwrap().push((
            repo.into(),
            ws.into(),
            path.into(),
            cmd.into(),
        ));
        Ok(())
    }
    fn send_to_slot(&self, _: &str, _: &str, _: u32, _: &str) -> Result<()> {
        Ok(())
    }
    fn kill_slot(&self, _: &str, _: &str, _: u32) -> Result<()> {
        Ok(())
    }
    fn attach_command(&self, repo: &str) -> String {
        format!("fake-attach {}", repo)
    }
}

fn unique_tempdir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "bunyan-delegate-test-{}-{}-{}",
        label,
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Initialize a git repo under `path` so worktree operations work.
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
    fs::write(path.join("README.md"), "initial\n").unwrap();
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

fn make_state_with_fake_backend() -> (Arc<AppState>, Arc<FakeBackend>) {
    let conn = Connection::open_in_memory().unwrap();
    db::initialize_database(&conn).unwrap();
    let fake = Arc::new(FakeBackend::default());
    let state = Arc::new(AppState::with_backend(conn, fake.clone()));
    (state, fake)
}

async fn body_json(resp: axum::http::Response<Body>) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn post_delegate_creates_worktree_and_returns_observation_url() {
    // The delegate flow expects the repo's root_path to look like
    // ~/bunyan/repos/<name> so workspace_path() can derive
    // ~/bunyan/workspaces/<name>/<dir>. Build that shape under a temp root.
    let root = unique_tempdir("delegate_happy");
    let repos_dir = root.join("repos");
    let workspaces_dir = root.join("workspaces");
    fs::create_dir_all(&repos_dir).unwrap();
    fs::create_dir_all(&workspaces_dir).unwrap();
    let repo_path = repos_dir.join("myrepo");
    init_git_repo(&repo_path);

    let (state, fake) = make_state_with_fake_backend();
    state.set_server_origin("http://127.0.0.1:9999");
    {
        let conn = state.db.lock().unwrap();
        db::repos::create(
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
    }

    let app = build_router(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/delegate")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "repo": "myrepo",
                        "branch": "fix-flaky",
                        "prompt": "Fix the flaky test in src/foo.spec.ts",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = body_json(resp).await;
    let ws_id = body["workspace_id"].as_str().unwrap().to_string();
    assert!(!ws_id.is_empty());
    let url = body["observation_url"].as_str().unwrap();
    assert!(url.contains("/workspaces/"));
    assert!(url.starts_with("http://127.0.0.1:9999"));

    // The worktree should exist on disk.
    let expected_path = workspaces_dir.join("myrepo").join("fix-flaky");
    assert!(
        expected_path.exists(),
        "worktree path {} should exist",
        expected_path.display()
    );

    // FakeBackend.spawn should have been called with the claude command
    // wrapping the prompt.
    let spawned = fake.spawned.lock().unwrap().clone();
    assert_eq!(spawned.len(), 1);
    let (repo, ws, _path, cmd) = &spawned[0];
    assert_eq!(repo, "myrepo");
    assert_eq!(ws, "fix-flaky");
    assert!(
        cmd.starts_with("claude '") && cmd.contains("Fix the flaky test"),
        "spawn cmd should be claude with the prompt, got {:?}",
        cmd
    );

    // The DB row should have the prompt and no parent (since we didn't set `from`).
    let conn = state.db.lock().unwrap();
    let ws_row = db::workspaces::get(&conn, &ws_id).unwrap();
    assert_eq!(
        ws_row.delegation_prompt.as_deref(),
        Some("Fix the flaky test in src/foo.spec.ts")
    );
    assert!(ws_row.parent_workspace_id.is_none());
}

#[tokio::test]
async fn post_delegate_records_parent_workspace_id_when_from_set() {
    let root = unique_tempdir("delegate_from");
    let repos_dir = root.join("repos");
    fs::create_dir_all(&repos_dir).unwrap();
    fs::create_dir_all(root.join("workspaces")).unwrap();
    let repo_path = repos_dir.join("myrepo");
    init_git_repo(&repo_path);

    let (state, _fake) = make_state_with_fake_backend();
    {
        let conn = state.db.lock().unwrap();
        db::repos::create(
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
    }

    let app = build_router(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/delegate")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "repo": "myrepo",
                        "branch": "side-task",
                        "prompt": "do the thing",
                        "from": "parent-abc"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    let ws_id = body["workspace_id"].as_str().unwrap().to_string();

    let conn = state.db.lock().unwrap();
    let ws_row = db::workspaces::get(&conn, &ws_id).unwrap();
    assert_eq!(ws_row.parent_workspace_id.as_deref(), Some("parent-abc"));
}

#[tokio::test]
async fn list_workspaces_filters_by_delegated_by_and_status() {
    let root = unique_tempdir("list_filters");
    let repos_dir = root.join("repos");
    fs::create_dir_all(&repos_dir).unwrap();
    fs::create_dir_all(root.join("workspaces")).unwrap();
    let repo_path = repos_dir.join("myrepo");
    init_git_repo(&repo_path);

    let (state, _fake) = make_state_with_fake_backend();
    let repo_id = {
        let conn = state.db.lock().unwrap();
        db::repos::create(
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
        .unwrap()
        .id
    };

    // Pre-create a parent and a delegated child in the DB.
    {
        let conn = state.db.lock().unwrap();
        db::workspaces::create(
            &conn,
            bunyan_core::models::CreateWorkspaceInput {
                repository_id: repo_id.clone(),
                directory_name: "parent".into(),
                branch: "p".into(),
                container_mode: bunyan_core::models::ContainerMode::Local,
            },
        )
        .unwrap();
        db::workspaces::create_with_lineage(
            &conn,
            bunyan_core::models::CreateWorkspaceInput {
                repository_id: repo_id.clone(),
                directory_name: "kid".into(),
                branch: "k".into(),
                container_mode: bunyan_core::models::ContainerMode::Local,
            },
            Some("parent-id"),
            Some("fix thing"),
        )
        .unwrap();
    }

    let app = build_router(state.clone());

    // delegated_by filter
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/workspaces?delegated_by=parent-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let rows = body.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["directory_name"], "kid");

    // status=ready filter — both rows are ready
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/workspaces?status=ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn get_result_returns_204_when_no_result_file() {
    let root = unique_tempdir("result_empty");
    let repos_dir = root.join("repos");
    fs::create_dir_all(&repos_dir).unwrap();
    let workspaces_dir = root.join("workspaces");
    fs::create_dir_all(&workspaces_dir).unwrap();
    let repo_path = repos_dir.join("myrepo");
    init_git_repo(&repo_path);

    let (state, _) = make_state_with_fake_backend();
    let (repo_id, ws_id) = {
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
        let ws = db::workspaces::create(
            &conn,
            bunyan_core::models::CreateWorkspaceInput {
                repository_id: r.id.clone(),
                directory_name: "ws".into(),
                branch: "ws".into(),
                container_mode: bunyan_core::models::ContainerMode::Local,
            },
        )
        .unwrap();
        (r.id, ws.id)
    };
    let _ = repo_id;
    // workspace_path() will compute workspaces_dir/myrepo/ws — make sure it exists.
    fs::create_dir_all(workspaces_dir.join("myrepo").join("ws")).unwrap();

    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/workspaces/{}/result", ws_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn get_result_returns_json_when_result_file_present() {
    let root = unique_tempdir("result_present");
    let repos_dir = root.join("repos");
    fs::create_dir_all(&repos_dir).unwrap();
    let workspaces_dir = root.join("workspaces");
    fs::create_dir_all(&workspaces_dir).unwrap();
    let repo_path = repos_dir.join("myrepo");
    init_git_repo(&repo_path);
    let wt = workspaces_dir.join("myrepo").join("ws");
    fs::create_dir_all(&wt).unwrap();
    fs::write(
        wt.join("result.json"),
        r#"{"status":"done","summary":"all good"}"#,
    )
    .unwrap();

    let (state, _) = make_state_with_fake_backend();
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
            bunyan_core::models::CreateWorkspaceInput {
                repository_id: r.id,
                directory_name: "ws".into(),
                branch: "ws".into(),
                container_mode: bunyan_core::models::ContainerMode::Local,
            },
        )
        .unwrap()
        .id
    };

    let app = build_router(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/workspaces/{}/result", ws_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "done");
    assert_eq!(body["summary"], "all good");
}
