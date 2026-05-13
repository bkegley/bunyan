//! Inject bunyan-aware Claude Code hooks into a spawned workspace.
//!
//! When bunyan spawns Claude for a delegated task, it writes
//! `<worktree>/.claude/settings.local.json` first. Claude Code loads that
//! file automatically and runs its hooks additively with the user's global
//! settings, so this is non-invasive: nothing about the user's normal
//! Claude config changes.
//!
//! The injected hooks are simple curl invocations that POST the Claude
//! hook's stdin payload to bunyan's HTTP API. The spawned Claude is never
//! prompted with anything bunyan-aware — only its *settings* know about us.

use std::path::Path;

use serde::Serialize;
use serde_json::json;

use crate::error::{BunyanError, Result};

/// Write `<worktree>/.claude/settings.local.json` with bunyan lifecycle hooks
/// pre-wired to report back to the given workspace.
///
/// `port` is the bunyan daemon port (so the hook curl knows where to POST).
pub fn write_session_settings(worktree_path: &Path, workspace_id: &str, port: u16) -> Result<()> {
    let claude_dir = worktree_path.join(".claude");
    std::fs::create_dir_all(&claude_dir).map_err(|e| {
        BunyanError::Process(format!(
            "Failed to create {} : {}",
            claude_dir.display(),
            e
        ))
    })?;

    let settings = build_settings(workspace_id, port);
    let path = claude_dir.join("settings.local.json");
    let body = serde_json::to_string_pretty(&settings)?;
    std::fs::write(&path, body).map_err(|e| {
        BunyanError::Process(format!("Failed to write {}: {}", path.display(), e))
    })?;
    Ok(())
}

#[derive(Debug, Serialize)]
struct HookCommand {
    #[serde(rename = "type")]
    type_: &'static str,
    command: String,
}

#[derive(Debug, Serialize)]
struct HookMatcher {
    matcher: String,
    hooks: Vec<HookCommand>,
}

/// Build the settings.local.json payload Claude will merge with user settings.
pub fn build_settings(workspace_id: &str, port: u16) -> serde_json::Value {
    let curl = make_curl_command(workspace_id, port);

    // The same curl shape works for every event Claude emits; bunyan demuxes
    // server-side by reading the `hook_event_name` Claude includes in the
    // payload it pipes to stdin.
    let hook = HookCommand {
        type_: "command",
        command: curl,
    };
    let matcher = HookMatcher {
        matcher: String::new(),
        hooks: vec![hook],
    };
    json!({
        "hooks": {
            "Stop": [matcher_to_value(&matcher)],
            "SubagentStop": [matcher_to_value(&matcher)],
            "Notification": [matcher_to_value(&matcher)],
            "SessionStart": [matcher_to_value(&matcher)],
        }
    })
}

fn matcher_to_value(m: &HookMatcher) -> serde_json::Value {
    serde_json::to_value(m).unwrap()
}

fn make_curl_command(workspace_id: &str, port: u16) -> String {
    // - `-fsS` keeps curl quiet on success but surfaces server errors to
    //   Claude's hook log, where it shows up as a warning rather than a
    //   blocker.
    // - `--data-binary @-` forwards Claude's stdin payload verbatim.
    // - `|| true` ensures a transient bunyan outage never blocks Claude.
    format!(
        "curl -fsS -m 5 -X POST 'http://127.0.0.1:{port}/workspaces/{ws}/agent-events' \
-H 'content-type: application/json' --data-binary @- || true",
        port = port,
        ws = workspace_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn unique_tempdir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "bunyan-claude-settings-test-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn build_settings_includes_all_lifecycle_hooks() {
        let v = build_settings("ws-id", 3333);
        let hooks = v.get("hooks").unwrap().as_object().unwrap();
        for key in ["Stop", "SubagentStop", "Notification", "SessionStart"] {
            assert!(hooks.contains_key(key), "missing {} hook", key);
        }
    }

    #[test]
    fn curl_command_contains_workspace_id_and_port() {
        let cmd = make_curl_command("ws-12345", 4444);
        assert!(cmd.contains("/workspaces/ws-12345/agent-events"));
        assert!(cmd.contains(":4444/"));
        assert!(cmd.ends_with("|| true"));
    }

    #[test]
    fn write_session_settings_creates_file_and_dir() {
        let tmp = unique_tempdir("write");
        write_session_settings(&tmp, "ws-id", 3333).unwrap();
        let path = tmp.join(".claude/settings.local.json");
        assert!(path.exists());
        let body = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(v["hooks"]["Stop"].is_array());
        assert!(v["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("ws-id"));
    }

    #[test]
    fn write_session_settings_overwrites_previous() {
        let tmp = unique_tempdir("overwrite");
        write_session_settings(&tmp, "first", 3333).unwrap();
        write_session_settings(&tmp, "second", 3333).unwrap();
        let body = fs::read_to_string(tmp.join(".claude/settings.local.json")).unwrap();
        assert!(body.contains("second"));
        assert!(!body.contains("first"));
    }
}
