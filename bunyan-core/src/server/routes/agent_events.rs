//! Endpoint hit by spawned Claudes via their injected settings.local.json
//! hooks. A `POST /workspaces/:id/agent-events` payload is verbatim Claude
//! Code's hook stdin JSON. We parse the `hook_event_name` and refire it as
//! a bunyan lifecycle event (so existing hook subscribers and the
//! observation surface see it).
//!
//! Hooks that report here:
//!   - Stop (claude finished its turn)
//!   - SubagentStop (a Task-spawned sub-agent finished)
//!   - Notification (claude is waiting for user input or timed out)
//!   - SessionStart (claude session was created)
//!
//! The endpoint never errors on malformed input — we always store the raw
//! body and respond 202 Accepted, because failing the hook would surface as
//! a noisy warning in the spawned Claude's session.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::events;
use crate::server::error::ApiError;
use crate::state::AppState;
use crate::workspace;

#[utoipa::path(
    post,
    path = "/workspaces/{id}/agent-events",
    params(("id" = String, Path, description = "Workspace ID")),
    request_body = serde_json::Value,
    responses(
        (status = 202, description = "Accepted; bunyan re-fires the event internally."),
        (status = 404, body = crate::models::ErrorResponse)
    ),
    tag = "workspaces"
)]
pub async fn agent_events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<StatusCode, ApiError> {
    let (ws, repo, ws_path) = {
        let conn = state.db.lock().unwrap();
        workspace::resolve_workspace_path(&conn, &id)?
    };

    // Try to parse the body; if it fails, log and accept anyway.
    let payload: serde_json::Value =
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);

    let claude_event = payload
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let bunyan_event = map_claude_event(claude_event);

    // Persist the most recent agent event as result.json for the observation
    // surface. This is a deliberate convention: the spawned Claude doesn't
    // need to write result.json — its Stop hook produces something that's
    // "good enough" for a reviewer.
    if claude_event == "Stop" || claude_event == "SubagentStop" {
        let p = std::path::PathBuf::from(&ws_path).join("result.json");
        let result_blob = serde_json::json!({
            "status": "stopped",
            "hook_event_name": claude_event,
            "received_at": chrono::Utc::now().to_rfc3339(),
            "raw": payload,
        });
        // Best-effort: failing here shouldn't fail the request.
        let _ = std::fs::write(&p, serde_json::to_string_pretty(&result_blob).unwrap_or_default());
    }

    // Fire a bunyan lifecycle event so user hooks (Slack, Tauri, etc.) get
    // notified. The event name is the dotted-lower version of the Claude
    // event for consistency with bunyan's other events.
    let ws_clone = ws.clone();
    let repo_clone = repo.clone();
    let ws_path_clone = ws_path.clone();
    let extras_payload = payload.clone();
    let bunyan_event_clone = bunyan_event.to_string();
    tokio::task::spawn_blocking(move || {
        let extras: Vec<(&str, String)> = vec![
            ("claude_event", claude_event_from_payload(&extras_payload)),
            ("payload", extras_payload.to_string()),
        ];
        let extras_refs: Vec<(&str, &str)> =
            extras.iter().map(|(k, v)| (*k, v.as_str())).collect();
        events::fire_workspace_event_with_extras(
            &bunyan_event_clone,
            &ws_clone,
            &repo_clone,
            &ws_path_clone,
            &extras_refs,
        );
    })
    .await
    .ok();

    Ok(StatusCode::ACCEPTED)
}

fn claude_event_from_payload(p: &serde_json::Value) -> String {
    p.get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Map Claude Code's hook event names to bunyan's dotted-lower convention.
fn map_claude_event(name: &str) -> &'static str {
    match name {
        "Stop" => "claude.stopped",
        "SubagentStop" => "claude.subagent_stopped",
        "Notification" => "claude.notification",
        "SessionStart" => "claude.session_started",
        "PreToolUse" => "claude.pre_tool_use",
        "PostToolUse" => "claude.post_tool_use",
        _ => "claude.event",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_claude_event_covers_known_events() {
        assert_eq!(map_claude_event("Stop"), "claude.stopped");
        assert_eq!(map_claude_event("SubagentStop"), "claude.subagent_stopped");
        assert_eq!(map_claude_event("Notification"), "claude.notification");
        assert_eq!(map_claude_event("SessionStart"), "claude.session_started");
        assert_eq!(map_claude_event("Bogus"), "claude.event");
    }

    #[test]
    fn claude_event_from_payload_returns_empty_for_missing_field() {
        let v = serde_json::json!({});
        assert_eq!(claude_event_from_payload(&v), "");
    }

    #[test]
    fn claude_event_from_payload_returns_field_value() {
        let v = serde_json::json!({"hook_event_name": "Stop"});
        assert_eq!(claude_event_from_payload(&v), "Stop");
    }
}
