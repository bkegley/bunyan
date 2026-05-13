//! POST /delegate logic.
//!
//! Bunyan's value-prop endpoint: a parent agent calls this once, gets a
//! minimal handle back, and forgets the spawned task ever happened.
//! Internally we atomically:
//!   1. create the worktree
//!   2. record the workspace row with lineage
//!   3. fire workspace.created (so per-repo bootstrap hooks can run)
//!   4. spawn `claude "<prompt>"` in the new workspace
//!
//! Errors at any step return a clean error; partial state is not cleaned
//! up automatically (the caller can archive the workspace to wind it back).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::claude_settings;
use crate::db;
use crate::error::{BunyanError, Result};
use crate::events::{self, names};
use crate::git::{GitOps, RealGit};
use crate::models::{ContainerMode, CreateWorkspaceInput, Workspace};
use crate::state::AppState;
use crate::workspace;

/// POST /delegate request body.
#[derive(Debug, Deserialize, Clone)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DelegateInput {
    /// Repo name (matches the `name` field on a Repo row).
    pub repo: String,
    /// Branch to create the worktree on.
    pub branch: String,
    /// The literal prompt for the spawned Claude.
    pub prompt: String,
    /// Optional workspace ID of the parent (for lineage).
    pub from: Option<String>,
    /// Optional human-friendly directory name; defaults to `branch`.
    pub directory_name: Option<String>,
}

/// POST /delegate response — deliberately minimal so the parent agent doesn't
/// accumulate context about the spawned task.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DelegateResponse {
    pub workspace_id: String,
    pub observation_url: String,
}

/// Perform the full delegation flow. Returns the new workspace + the
/// observation URL the parent should log.
pub async fn delegate(
    state: &Arc<AppState>,
    input: DelegateInput,
    server_origin: &str,
) -> Result<DelegateResponse> {
    // 1. Resolve the repo by name.
    let repo = {
        let conn = state.db.lock().unwrap();
        let repos = db::repos::list(&conn)?;
        repos
            .into_iter()
            .find(|r| r.name == input.repo)
            .ok_or_else(|| BunyanError::NotFound(format!("Repo not found: {}", input.repo)))?
    };

    let dir_name = input
        .directory_name
        .clone()
        .unwrap_or_else(|| input.branch.replace('/', "-"));

    // 2. Create the worktree.
    let wt_path =
        workspace::workspace_path(&repo.root_path, &repo.name, &dir_name)?;
    let repo_root = repo.root_path.clone();
    let branch = input.branch.clone();
    let wt_for_git = wt_path.clone();
    tokio::task::spawn_blocking(move || {
        let git = RealGit;
        git.worktree_add(&repo_root, &wt_for_git, &branch)
    })
    .await
    .map_err(|e| BunyanError::Process(e.to_string()))??;

    // 3. Insert the workspace row with lineage info.
    let ws: Workspace = {
        let conn = state.db.lock().unwrap();
        db::workspaces::create_with_lineage(
            &conn,
            CreateWorkspaceInput {
                repository_id: repo.id.clone(),
                directory_name: dir_name.clone(),
                branch: input.branch.clone(),
                container_mode: ContainerMode::Local,
            },
            input.from.as_deref(),
            Some(&input.prompt),
        )?
    };

    // 4. Inject bunyan-aware Claude hooks into the new worktree. The spawned
    //    Claude will report back to bunyan at Stop/SubagentStop/Notification/
    //    SessionStart without its prompt knowing anything about bunyan.
    {
        let wt = std::path::PathBuf::from(&wt_path);
        let ws_id = ws.id.clone();
        let port = parse_port(&server_origin.to_string()).unwrap_or(3333);
        tokio::task::spawn_blocking(move || {
            claude_settings::write_session_settings(&wt, &ws_id, port)
        })
        .await
        .map_err(|e| BunyanError::Process(e.to_string()))??;
    }

    // 5. Fire workspace.created so per-repo bootstrap hooks can install deps,
    //    seed .env, etc. We block on this — a delegated agent needs the env
    //    bootstrapped before it starts.
    {
        let ws_clone = ws.clone();
        let repo_clone = repo.clone();
        let wt_clone = wt_path.clone();
        let bus = state.event_bus.clone();
        tokio::task::spawn_blocking(move || {
            events::fire_and_publish(
                &bus,
                names::WORKSPACE_CREATED,
                &ws_clone,
                &repo_clone,
                &wt_clone,
            );
        })
        .await
        .map_err(|e| BunyanError::Process(e.to_string()))?;
    }

    // 6. Spawn Claude with the prompt in the new workspace.
    {
        let backend = state.backend.clone();
        let rn = repo.name.clone();
        let wn = ws.directory_name.clone();
        let wp = wt_path.clone();
        let cmd = build_claude_command(&input.prompt);
        tokio::task::spawn_blocking(move || backend.spawn(&rn, &wn, &wp, &cmd))
            .await
            .map_err(|e| BunyanError::Process(e.to_string()))??;
    }

    // 7. Fire claude.started.
    {
        let ws_clone = ws.clone();
        let repo_clone = repo.clone();
        let wt_clone = wt_path.clone();
        let bus = state.event_bus.clone();
        tokio::task::spawn_blocking(move || {
            events::fire_and_publish(
                &bus,
                names::CLAUDE_STARTED,
                &ws_clone,
                &repo_clone,
                &wt_clone,
            );
        })
        .await
        .ok();
    }

    let observation_url = format!(
        "{}/workspaces/{}",
        server_origin.trim_end_matches('/'),
        ws.id
    );
    Ok(DelegateResponse {
        workspace_id: ws.id,
        observation_url,
    })
}

/// Extract the port out of an origin URL like "http://127.0.0.1:3333".
/// Returns None if the URL doesn't carry a port (e.g. bare "http://host").
fn parse_port(origin: &str) -> Option<u16> {
    let after_scheme = origin.split("://").nth(1)?;
    // strip trailing path or query
    let host_port = after_scheme.split(['/', '?']).next()?;
    let port_str = host_port.rsplit(':').next()?;
    port_str.parse().ok()
}

/// Build the `claude` shell invocation for a delegated prompt.
///
/// Uses `claude -p` (print mode) so the prompt is the first user turn and
/// the session runs without an interactive UI on the receiving side.
/// Surrounding the prompt with single quotes is enough for shells; any
/// single quotes inside the prompt get escaped via the standard bash
/// 'closing-then-escaping' trick.
pub fn build_claude_command(prompt: &str) -> String {
    let escaped = prompt.replace('\'', r#"'\''"#);
    format!("claude '{}'", escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_claude_command_quotes_the_prompt() {
        assert_eq!(
            build_claude_command("fix the bug"),
            "claude 'fix the bug'"
        );
    }

    #[test]
    fn build_claude_command_escapes_single_quotes() {
        // The bash idiom for embedding a ' inside a single-quoted string
        // is '\''. We want round-trip safety: shell parses this back as a
        // literal apostrophe inside the prompt.
        let cmd = build_claude_command("fix Bob's bug");
        assert_eq!(cmd, r#"claude 'fix Bob'\''s bug'"#);
    }

    #[test]
    fn build_claude_command_handles_multiline() {
        let cmd = build_claude_command("line1\nline2");
        assert_eq!(cmd, "claude 'line1\nline2'");
    }

    #[test]
    fn parse_port_handles_common_origins() {
        assert_eq!(parse_port("http://127.0.0.1:3333"), Some(3333));
        assert_eq!(parse_port("http://localhost:9999/"), Some(9999));
        assert_eq!(parse_port("https://example.com:8443/path"), Some(8443));
        assert_eq!(parse_port("http://no-port-here"), None);
    }
}
