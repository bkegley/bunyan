//! Verify the RuntimeBackend trait actually decouples routes from tmux.
//!
//! We build a fake in-memory backend, register it on AppState, and exercise
//! the routes that previously called tmux directly. The point is to prove
//! the routes no longer hardcode tmux: a backend that never shells out at
//! all can still serve the API.

#![cfg(feature = "server")]

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rusqlite::Connection;
use tower::ServiceExt;

use bunyan_core::backends::{ProcessInfo, RuntimeBackend};
use bunyan_core::db;
use bunyan_core::error::Result;
use bunyan_core::models::{ContainerMode, CreateRepoInput, CreateWorkspaceInput};
use bunyan_core::server::build_router;
use bunyan_core::state::AppState;

#[derive(Default)]
struct FakeBackend {
    calls: Mutex<Vec<String>>,
    /// Optional canned process list returned from list_processes.
    canned_processes: Mutex<Vec<ProcessInfo>>,
}

impl FakeBackend {
    fn record(&self, call: &str) {
        self.calls.lock().unwrap().push(call.to_string());
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    fn set_processes(&self, p: Vec<ProcessInfo>) {
        *self.canned_processes.lock().unwrap() = p;
    }
}

impl RuntimeBackend for FakeBackend {
    fn name(&self) -> &'static str {
        "fake"
    }
    fn ensure_workspace(
        &self,
        repo: &str,
        ws: &str,
        path: &str,
    ) -> Result<()> {
        self.record(&format!("ensure_workspace:{repo}/{ws}@{path}"));
        Ok(())
    }
    fn kill_workspace(&self, repo: &str, ws: &str) -> Result<()> {
        self.record(&format!("kill_workspace:{repo}/{ws}"));
        Ok(())
    }
    fn list_processes(&self, _: &str, _: &str) -> Result<Vec<ProcessInfo>> {
        Ok(self.canned_processes.lock().unwrap().clone())
    }
    fn list_all_processes(&self) -> Result<Vec<(String, String, ProcessInfo)>> {
        Ok(vec![])
    }
    fn spawn(&self, repo: &str, ws: &str, path: &str, cmd: &str) -> Result<()> {
        self.record(&format!("spawn:{repo}/{ws}@{path}!{cmd}"));
        Ok(())
    }
    fn send_to_slot(&self, repo: &str, ws: &str, idx: u32, cmd: &str) -> Result<()> {
        self.record(&format!("send_to_slot:{repo}/{ws}:{idx}!{cmd}"));
        Ok(())
    }
    fn kill_slot(&self, repo: &str, ws: &str, idx: u32) -> Result<()> {
        self.record(&format!("kill_slot:{repo}/{ws}:{idx}"));
        Ok(())
    }
    fn attach_command(&self, repo: &str) -> String {
        format!("fake-attach {}", repo)
    }
}

fn make_state(backend: Arc<dyn RuntimeBackend>) -> Arc<AppState> {
    let conn = Connection::open_in_memory().unwrap();
    db::initialize_database(&conn).unwrap();
    Arc::new(AppState::with_backend(conn, backend))
}

fn seed(state: &Arc<AppState>) -> (String, String) {
    let conn = state.db.lock().unwrap();
    let repo = db::repos::create(
        &conn,
        CreateRepoInput {
            name: "myrepo".into(),
            remote_url: "u".into(),
            root_path: "/tmp/myrepo".into(),
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
            directory_name: "fix".into(),
            branch: "fix".into(),
            container_mode: ContainerMode::Local,
        },
    )
    .unwrap();
    (repo.id, ws.id)
}

async fn body_json(resp: axum::http::Response<Body>) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn get_panes_returns_canned_processes_from_fake_backend() {
    let fake = Arc::new(FakeBackend::default());
    fake.set_processes(vec![ProcessInfo {
        handle: "fake:1".into(),
        command: "claude".into(),
        is_active: true,
        cwd: "/tmp/myrepo".into(),
        pid: 12345,
        slot_index: 0,
    }]);
    let state = make_state(fake.clone());
    let (_repo_id, ws_id) = seed(&state);
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/workspaces/{}/panes", ws_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body[0]["command"], "claude");
    assert_eq!(body[0]["pane_index"], 0);
    assert_eq!(body[0]["pane_pid"], 12345);
}

#[tokio::test]
async fn view_route_invokes_ensure_workspace_on_backend() {
    let fake = Arc::new(FakeBackend::default());
    let state = make_state(fake.clone());
    let (_, ws_id) = seed(&state);
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workspaces/{}/view", ws_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let calls = fake.calls();
    assert!(
        calls.iter().any(|c| c.starts_with("ensure_workspace:myrepo/fix")),
        "expected ensure_workspace, got {:?}",
        calls
    );
}

#[tokio::test]
async fn archive_route_invokes_kill_workspace_on_backend() {
    let fake = Arc::new(FakeBackend::default());
    let state = make_state(fake.clone());
    let (_, ws_id) = seed(&state);
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workspaces/{}/archive", ws_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Will likely error because the worktree path doesn't exist; that's fine —
    // what we care about is that kill_workspace was called before the git op.
    let _ = resp;

    let calls = fake.calls();
    assert!(
        calls.iter().any(|c| c == "kill_workspace:myrepo/fix"),
        "expected kill_workspace, got {:?}",
        calls
    );
}

#[tokio::test]
async fn kill_pane_route_invokes_kill_slot_on_backend() {
    let fake = Arc::new(FakeBackend::default());
    let state = make_state(fake.clone());
    let (_, ws_id) = seed(&state);
    let app = build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/workspaces/{}/panes/3", ws_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let calls = fake.calls();
    assert!(
        calls.iter().any(|c| c == "kill_slot:myrepo/fix:3"),
        "expected kill_slot, got {:?}",
        calls
    );
}
