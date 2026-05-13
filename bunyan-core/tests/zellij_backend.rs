//! Integration tests that drive the real `zellij` binary.
//!
//! Skipped automatically when `zellij` isn't on PATH (so CI without zellij
//! still passes). When zellij IS available we exercise the full
//! ensure_workspace + spawn + list_processes + kill_workspace cycle to make
//! sure the backend's command shapes still match what zellij accepts.
//!
//! Each test runs in its own session named with a unique prefix so they
//! don't collide with each other or with the user's existing zellij
//! sessions.

#![cfg(feature = "server")]

use std::process::Command;

use bunyan_core::backends::zellij::ZellijBackend;
use bunyan_core::backends::RuntimeBackend;

fn zellij_available() -> bool {
    Command::new("zellij")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn unique_session(label: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("bun-{}-{}-{}", label, std::process::id(), nanos % 1_000_000)
}

fn cleanup(session: &str) {
    let _ = Command::new("zellij")
        .args(["kill-session", session])
        .output();
    let _ = Command::new("zellij")
        .args(["delete-session", session])
        .output();
}

#[test]
fn ensure_and_kill_workspace_round_trip() {
    if !zellij_available() {
        eprintln!("skipping zellij_backend tests: zellij not on PATH");
        return;
    }
    let session = unique_session("rt");
    let backend = ZellijBackend::new();

    // Use /tmp as the workspace path; zellij just needs a real directory.
    let result = backend.ensure_workspace(&session, "wk-1", "/tmp");
    assert!(result.is_ok(), "ensure_workspace failed: {:?}", result);

    // Idempotent
    let result = backend.ensure_workspace(&session, "wk-1", "/tmp");
    assert!(result.is_ok());

    // Tear down so we don't leak sessions on the dev machine.
    let _ = backend.kill_workspace(&session, "wk-1");
    cleanup(&session);
}

#[test]
fn spawn_and_list_processes_after_short_settle() {
    if !zellij_available() {
        eprintln!("skipping zellij_backend tests: zellij not on PATH");
        return;
    }
    let session = unique_session("spawn");
    let backend = ZellijBackend::new();
    backend.ensure_workspace(&session, "wk", "/tmp").unwrap();

    // Spawn a process that sleeps so it's still alive when we list.
    backend
        .spawn(&session, "wk", "/tmp", "sleep 30")
        .expect("spawn");

    // Zellij takes a moment to surface the new pane in list-panes --json.
    std::thread::sleep(std::time::Duration::from_millis(500));

    let processes = backend
        .list_processes(&session, "wk")
        .expect("list_processes");
    assert!(
        !processes.is_empty(),
        "expected at least one pane after spawn, got {:?}",
        processes
    );

    backend.kill_workspace(&session, "wk").ok();
    cleanup(&session);
}

#[test]
fn list_all_processes_includes_our_session() {
    if !zellij_available() {
        eprintln!("skipping zellij_backend tests: zellij not on PATH");
        return;
    }
    let session = unique_session("all");
    let backend = ZellijBackend::new();
    backend.ensure_workspace(&session, "wk", "/tmp").unwrap();
    backend
        .spawn(&session, "wk", "/tmp", "sleep 30")
        .expect("spawn");
    std::thread::sleep(std::time::Duration::from_millis(500));

    let all = backend.list_all_processes().expect("list_all_processes");
    let found = all.iter().any(|(s, _, _)| s == &session);
    assert!(found, "expected to find session {} in list_all", session);

    backend.kill_workspace(&session, "wk").ok();
    cleanup(&session);
}
