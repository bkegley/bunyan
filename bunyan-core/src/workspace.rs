use std::path::PathBuf;

use std::sync::Arc;

use rusqlite::Connection;

use crate::db;
use crate::docker;
use crate::error::{BunyanError, Result};
use crate::models::{ContainerConfig, Repo, Workspace};
use crate::state::AppState;
use crate::tmux;

/// Derive the workspace filesystem path from a repo's root path.
/// ~/bunyan/repos/<name> -> ~/bunyan/workspaces/<name>/<dir_name>
pub fn workspace_path(repo_root: &str, repo_name: &str, dir_name: &str) -> Result<String> {
    let repo_path = PathBuf::from(repo_root);
    let base = repo_path
        .parent()
        .ok_or_else(|| BunyanError::Git("Invalid repo root path".to_string()))?
        .parent()
        .ok_or_else(|| BunyanError::Git("Invalid repo root path".to_string()))?;

    let path = base.join("workspaces").join(repo_name).join(dir_name);
    path.to_str()
        .ok_or_else(|| BunyanError::Git("Invalid path".to_string()))
        .map(|s| s.to_string())
}

/// Resolve workspace, repo, and filesystem path from a workspace ID.
pub fn resolve_workspace_path(
    conn: &Connection,
    workspace_id: &str,
) -> Result<(Workspace, Repo, String)> {
    let ws = db::workspaces::get(conn, workspace_id)?;
    let rp = db::repos::get(conn, &ws.repository_id)?;
    let ws_path = workspace_path(&rp.root_path, &rp.name, &ws.directory_name)?;
    Ok((ws, rp, ws_path))
}

/// Kill the entire tmux window for a workspace (used before archiving).
pub fn kill_workspace_window(repo_name: &str, workspace_name: &str) {
    let _ = tmux::kill_window(repo_name, workspace_name);
}

/// Extract container config from a repo's JSON config blob.
pub fn get_container_config(repo: &Repo) -> Option<ContainerConfig> {
    repo.config
        .as_ref()
        .and_then(|v| v.get("container"))
        .and_then(|v| serde_json::from_value::<ContainerConfig>(v.clone()).ok())
}

/// Check if dangerously_skip_permissions is enabled in the repo's container config.
pub fn should_skip_permissions(repo: &Repo) -> bool {
    get_container_config(repo)
        .map(|c| c.dangerously_skip_permissions)
        .unwrap_or(false)
}

/// Build a claude command string, optionally adding --dangerously-skip-permissions.
pub fn build_claude_cmd(base: &str, skip_permissions: bool) -> String {
    if skip_permissions {
        format!("{} --dangerously-skip-permissions", base)
    } else {
        base.to_string()
    }
}

/// Validate that a session ID is a safe UUID-like string (hex + dashes + underscores).
pub fn validate_session_id(id: &str) -> std::result::Result<(), String> {
    if id.is_empty() {
        return Err("Empty session ID".to_string());
    }
    let is_valid = id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !is_valid {
        return Err(format!("Invalid session ID: {}", id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Repo;

    #[test]
    fn workspace_path_derives_from_repo_root() {
        let result = workspace_path("/home/user/bunyan/repos/myrepo", "myrepo", "fix-bug").unwrap();
        assert_eq!(result, "/home/user/bunyan/workspaces/myrepo/fix-bug");
    }

    #[test]
    fn workspace_path_different_repo_names() {
        let result = workspace_path("/data/bunyan/repos/backend", "backend", "feature-x").unwrap();
        assert_eq!(result, "/data/bunyan/workspaces/backend/feature-x");
    }

    #[test]
    fn workspace_path_single_component_errors() {
        // A path like "/repos" has only one parent ("/"), so second .parent() = None
        let result = workspace_path("/repos", "myrepo", "fix");
        assert!(result.is_err());
    }

    #[test]
    fn workspace_path_root_errors() {
        let result = workspace_path("/", "myrepo", "fix");
        assert!(result.is_err());
    }

    #[test]
    fn validate_session_id_accepts_uuid() {
        assert!(validate_session_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn validate_session_id_accepts_alphanumeric() {
        assert!(validate_session_id("abc123").is_ok());
    }

    #[test]
    fn validate_session_id_accepts_underscores() {
        assert!(validate_session_id("my_session_id").is_ok());
    }

    #[test]
    fn validate_session_id_rejects_empty() {
        assert!(validate_session_id("").is_err());
    }

    #[test]
    fn validate_session_id_rejects_shell_metacharacters() {
        assert!(validate_session_id("id;rm -rf /").is_err());
        assert!(validate_session_id("id$(whoami)").is_err());
        assert!(validate_session_id("id`cmd`").is_err());
        assert!(validate_session_id("id|cat").is_err());
        assert!(validate_session_id("id&bg").is_err());
    }

    #[test]
    fn validate_session_id_rejects_spaces() {
        assert!(validate_session_id("id with spaces").is_err());
    }

    #[test]
    fn validate_session_id_rejects_slashes() {
        assert!(validate_session_id("../../etc/passwd").is_err());
    }

    #[test]
    fn build_claude_cmd_without_skip() {
        assert_eq!(build_claude_cmd("claude", false), "claude");
    }

    #[test]
    fn build_claude_cmd_with_skip() {
        assert_eq!(
            build_claude_cmd("claude", true),
            "claude --dangerously-skip-permissions"
        );
    }

    #[test]
    fn build_claude_cmd_continue_with_skip() {
        assert_eq!(
            build_claude_cmd("claude --continue", true),
            "claude --continue --dangerously-skip-permissions"
        );
    }

    #[test]
    fn build_claude_cmd_resume_without_skip() {
        let cmd = build_claude_cmd("claude --resume abc-123", false);
        assert_eq!(cmd, "claude --resume abc-123");
    }

    fn make_repo(config: Option<serde_json::Value>) -> Repo {
        Repo {
            id: "id".to_string(),
            name: "test".to_string(),
            remote_url: "url".to_string(),
            default_branch: "main".to_string(),
            root_path: "/tmp/repos/test".to_string(),
            remote: "origin".to_string(),
            display_order: 0,
            config,
            created_at: "".to_string(),
            updated_at: "".to_string(),
        }
    }

    #[test]
    fn get_container_config_none_when_no_config() {
        let repo = make_repo(None);
        assert!(get_container_config(&repo).is_none());
    }

    #[test]
    fn get_container_config_none_when_no_container_key() {
        let repo = make_repo(Some(serde_json::json!({"other": "value"})));
        assert!(get_container_config(&repo).is_none());
    }

    #[test]
    fn get_container_config_parses_valid() {
        let repo = make_repo(Some(serde_json::json!({
            "container": {
                "enabled": true,
                "image": "python:3.12",
                "dangerously_skip_permissions": true
            }
        })));
        let cfg = get_container_config(&repo).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.image.unwrap(), "python:3.12");
        assert!(cfg.dangerously_skip_permissions);
    }

    #[test]
    fn get_container_config_ignores_invalid_shape() {
        let repo = make_repo(Some(serde_json::json!({
            "container": "not an object"
        })));
        assert!(get_container_config(&repo).is_none());
    }

    #[test]
    fn should_skip_permissions_false_when_no_config() {
        let repo = make_repo(None);
        assert!(!should_skip_permissions(&repo));
    }

    #[test]
    fn should_skip_permissions_false_by_default() {
        let repo = make_repo(Some(serde_json::json!({
            "container": {"enabled": true}
        })));
        assert!(!should_skip_permissions(&repo));
    }

    #[test]
    fn should_skip_permissions_true_when_set() {
        let repo = make_repo(Some(serde_json::json!({
            "container": {
                "enabled": true,
                "dangerously_skip_permissions": true
            }
        })));
        assert!(should_skip_permissions(&repo));
    }
}

/// Create a workspace container (Docker container setup for container-mode workspaces).
/// Returns the updated workspace with container_id set.
/// Takes Arc<AppState> to avoid holding MutexGuard across await points.
pub async fn setup_workspace_container(
    state: &Arc<AppState>,
    workspace: &Workspace,
    repo: &Repo,
) -> std::result::Result<Workspace, String> {
    let container_config = get_container_config(repo);

    let image = container_config
        .as_ref()
        .and_then(|c| c.image.clone())
        .unwrap_or_else(|| "node:22".to_string());
    let ports = container_config
        .as_ref()
        .and_then(|c| c.ports.clone())
        .unwrap_or_default();
    let env: Vec<String> = container_config
        .as_ref()
        .and_then(|c| c.env.clone())
        .map(|m| m.into_iter().map(|(k, v)| format!("{}={}", k, v)).collect())
        .unwrap_or_default();

    let wt_path = workspace_path(&repo.root_path, &repo.name, &workspace.directory_name)
        .map_err(|e| e.to_string())?;
    let container_name = docker::sanitize_docker_name(
        &format!("bunyan-{}-{}", repo.name, workspace.directory_name),
    );

    let network_name = docker::sanitize_docker_name(&format!("bunyan-{}", repo.name));
    docker::create_network(&network_name)
        .await
        .map_err(|e| e.to_string())?;

    let container_id = docker::create_workspace_container(
        &image,
        &wt_path,
        &container_name,
        &ports,
        &env,
        Some(&network_name),
        &workspace.directory_name,
    )
    .await
    .map_err(|e| e.to_string())?;

    // Best-effort: install claude in the container
    if let Err(e) = docker::ensure_claude(&container_id).await {
        eprintln!("Warning: could not install Claude in container: {}", e);
    }

    // Lock only for DB operations, not across await
    let conn = state.db.lock().unwrap();
    db::workspaces::set_container_id(&conn, &workspace.id, &container_id)
        .map_err(|e| e.to_string())?;

    db::workspaces::get(&conn, &workspace.id).map_err(|e| e.to_string())
}
