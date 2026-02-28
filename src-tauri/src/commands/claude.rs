use std::path::{Path, PathBuf};
use tauri::State;

use bunyan_core::db;
use bunyan_core::docker;
use bunyan_core::editor;
use bunyan_core::models::{ClaudeSessionEntry, ContainerConfig, ContainerMode, TmuxPane, WorkspacePaneInfo};
use bunyan_core::state::AppState;
use bunyan_core::terminal;
use bunyan_core::tmux;

/// Validate that a session ID is a safe UUID-like string (hex + dashes).
fn validate_session_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Empty session ID".to_string());
    }
    let is_valid = id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !is_valid {
        return Err(format!("Invalid session ID: {}", id));
    }
    Ok(())
}

/// Check if dangerously_skip_permissions is enabled in the repo's container config.
fn should_skip_permissions(repo: &bunyan_core::models::Repo) -> bool {
    repo.config
        .as_ref()
        .and_then(|v| v.get("container"))
        .and_then(|v| serde_json::from_value::<ContainerConfig>(v.clone()).ok())
        .map(|c| c.dangerously_skip_permissions)
        .unwrap_or(false)
}

/// Build a claude command string, optionally adding --dangerously-skip-permissions.
fn build_claude_cmd(base: &str, skip_permissions: bool) -> String {
    if skip_permissions {
        format!("{} --dangerously-skip-permissions", base)
    } else {
        base.to_string()
    }
}

/// Resolve the filesystem path for a workspace from DB records.
/// Returns (workspace, repo, workspace_path_string).
fn resolve_workspace_path(
    conn: &rusqlite::Connection,
    workspace_id: &str,
) -> Result<(bunyan_core::models::Workspace, bunyan_core::models::Repo, String), String> {
    let ws = db::workspaces::get(conn, workspace_id).map_err(|e| e.to_string())?;
    let rp = db::repos::get(conn, &ws.repository_id).map_err(|e| e.to_string())?;
    let base = PathBuf::from(&rp.root_path)
        .parent()
        .and_then(|p| p.parent())
        .ok_or("Invalid repo root path")?
        .to_path_buf();
    let ws_path = base
        .join("workspaces")
        .join(&rp.name)
        .join(&ws.directory_name);
    let ws_path_str = ws_path
        .to_str()
        .ok_or("Invalid workspace path")?
        .to_string();
    Ok((ws, rp, ws_path_str))
}

/// Read sessions for a workspace. Tries sessions-index.json first, falls back
/// to scanning JSONL files directly.
fn read_sessions(
    workspace_path: &str,
    container_mode: &ContainerMode,
    directory_name: &str,
) -> Result<Vec<ClaudeSessionEntry>, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let sanitized = if *container_mode == ContainerMode::Container {
        format!("/workspace/{}", directory_name).replace('/', "-")
    } else {
        workspace_path.replace('/', "-")
    };
    let project_dir = home
        .join(".claude")
        .join("projects")
        .join(&sanitized);

    if !project_dir.exists() {
        return Ok(vec![]);
    }

    // Try sessions-index.json first
    let index_path = project_dir.join("sessions-index.json");
    if index_path.exists() {
        if let Ok(sessions) = read_sessions_from_index(&index_path) {
            return Ok(sessions);
        }
    }

    // Fall back to scanning JSONL files
    read_sessions_from_jsonl(&project_dir)
}

fn read_sessions_from_index(index_path: &Path) -> Result<Vec<ClaudeSessionEntry>, String> {
    let content = std::fs::read_to_string(index_path)
        .map_err(|e| format!("Failed to read sessions-index.json: {}", e))?;

    #[derive(serde::Deserialize)]
    struct SessionsIndex {
        entries: Vec<ClaudeSessionEntry>,
    }

    let index: SessionsIndex = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse sessions-index.json: {}", e))?;

    let mut sessions: Vec<ClaudeSessionEntry> = index
        .entries
        .into_iter()
        .filter(|e| !e.is_sidechain.unwrap_or(false))
        .collect();
    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));

    Ok(sessions)
}

/// Scan .jsonl files in a project directory and extract session metadata
/// from the first user message in each file.
fn read_sessions_from_jsonl(project_dir: &Path) -> Result<Vec<ClaudeSessionEntry>, String> {
    let entries = std::fs::read_dir(project_dir)
        .map_err(|e| format!("Failed to read project directory: {}", e))?;

    let mut sessions = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }

        // Session ID is the filename without extension
        let session_id = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        // Read file metadata for modified time
        let modified = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| {
                let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                let dt = chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)?;
                Some(dt.to_rfc3339())
            });

        // Read first few lines to find the first user message
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;

        let mut first_prompt = None;
        let mut created = None;
        let mut git_branch = None;
        let mut is_sidechain = None;
        let mut message_count: i32 = 0;

        for line in reader.lines().take(50) {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            let val: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let msg_type = val.get("type").and_then(|t| t.as_str());

            if msg_type == Some("user") || msg_type == Some("assistant") {
                message_count += 1;
            }

            // Extract metadata from the first user message
            if msg_type == Some("user") && first_prompt.is_none() {
                first_prompt = val
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string());
                created = val
                    .get("timestamp")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());
                git_branch = val
                    .get("gitBranch")
                    .and_then(|b| b.as_str())
                    .map(|s| s.to_string());
                is_sidechain = val
                    .get("isSidechain")
                    .and_then(|b| b.as_bool());
            }
        }

        if is_sidechain == Some(true) {
            continue;
        }

        sessions.push(ClaudeSessionEntry {
            session_id,
            first_prompt,
            message_count: Some(message_count),
            created,
            modified,
            git_branch,
            is_sidechain,
        });
    }

    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(sessions)
}

fn has_existing_session(
    workspace_path: &str,
    container_mode: &ContainerMode,
    directory_name: &str,
) -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };

    let sanitized = if *container_mode == ContainerMode::Container {
        format!("/workspace/{}", directory_name).replace('/', "-")
    } else {
        workspace_path.replace('/', "-")
    };
    let sessions_path = home
        .join(".claude")
        .join("projects")
        .join(&sanitized)
        .join("sessions-index.json");

    sessions_path.exists()
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Get pane info for all workspaces that have active tmux windows.
/// Used by the frontend for polling active sessions.
#[tauri::command]
#[specta::specta]
pub async fn get_active_claude_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<WorkspacePaneInfo>, String> {
    // Get all panes from the bunyan tmux server
    let all_panes = tokio::task::spawn_blocking(|| tmux::list_all_panes())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    if all_panes.is_empty() {
        return Ok(vec![]);
    }

    // Group panes by (session_name, window_name)
    let mut grouped: std::collections::HashMap<(String, String), Vec<TmuxPane>> =
        std::collections::HashMap::new();
    for (session_name, window_name, pane) in all_panes {
        grouped
            .entry((session_name, window_name))
            .or_default()
            .push(pane);
    }

    // Match against workspaces in DB
    let (workspaces, repos) = {
        let conn = state.db.lock().unwrap();
        let ws = db::workspaces::list(&conn, None).map_err(|e| e.to_string())?;
        let rp = db::repos::list(&conn).map_err(|e| e.to_string())?;
        (ws, rp)
    };

    let mut results = Vec::new();
    for ((session_name, window_name), panes) in grouped {
        // Find matching workspace: session_name = repo.name, window_name = workspace.directory_name
        let workspace = workspaces.iter().find(|ws| {
            ws.directory_name == window_name
                && repos
                    .iter()
                    .any(|r| r.id == ws.repository_id && r.name == session_name)
        });

        if let Some(ws) = workspace {
            results.push(WorkspacePaneInfo {
                workspace_id: ws.id.clone(),
                repo_name: session_name,
                workspace_name: window_name,
                panes,
            });
        }
    }

    Ok(results)
}

/// Open a Claude session in a workspace.
/// - If Claude is already running → attach to the existing window
/// - If no Claude running → create a new pane with claude, then attach
#[tauri::command]
#[specta::specta]
pub async fn open_claude_session(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<String, String> {
    let (workspace, repo, ws_path_str) = {
        let conn = state.db.lock().unwrap();
        resolve_workspace_path(&conn, &workspace_id)?
    };

    let repo_name = repo.name.clone();
    let ws_name = workspace.directory_name.clone();
    let ws_path = ws_path_str.clone();

    // Check if Claude is already running in this workspace
    let ws_name_check = ws_name.clone();
    let repo_name_check = repo_name.clone();
    let has_claude = tokio::task::spawn_blocking(move || {
        tmux::has_claude_running(&repo_name_check, &ws_name_check)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    if has_claude {
        // Claude is running — just attach iTerm to the session
        let repo_name_attach = repo_name.clone();
        let ws_name_attach = ws_name.clone();
        tokio::task::spawn_blocking(move || {
            terminal::attach_iterm(&repo_name_attach, &ws_name_attach)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        return Ok("attached".to_string());
    }

    // No Claude running — determine command
    let resume_path = ws_path.clone();
    let resume_container_mode = workspace.container_mode.clone();
    let resume_dir_name = workspace.directory_name.clone();
    let has_previous = tokio::task::spawn_blocking(move || {
        has_existing_session(&resume_path, &resume_container_mode, &resume_dir_name)
    })
    .await
    .map_err(|e| e.to_string())?;

    let skip_perms = workspace.container_mode == ContainerMode::Container
        && should_skip_permissions(&repo);

    let base_cmd = if has_previous {
        build_claude_cmd("claude --continue", skip_perms)
    } else {
        build_claude_cmd("claude", skip_perms)
    };

    let claude_cmd = if workspace.container_mode == ContainerMode::Container {
        match &workspace.container_id {
            Some(cid) => docker::docker_exec_cmd(cid, &base_cmd).map_err(|e| e.to_string())?,
            None => base_cmd,
        }
    } else {
        base_cmd
    };

    // Create pane with Claude
    let repo_name_create = repo_name.clone();
    let ws_name_create = ws_name.clone();
    let ws_path_create = ws_path.clone();
    tokio::task::spawn_blocking(move || {
        tmux::create_pane(&repo_name_create, &ws_name_create, &ws_path_create, &claude_cmd)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    // Attach iTerm
    let repo_name_attach = repo_name.clone();
    let ws_name_attach = ws_name.clone();
    tokio::task::spawn_blocking(move || {
        terminal::attach_iterm(&repo_name_attach, &ws_name_attach)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    Ok("created".to_string())
}

/// Resume a specific Claude session by session_id.
/// Reuses an idle pane if available, otherwise creates a new one.
#[tauri::command]
#[specta::specta]
pub async fn resume_claude_session(
    state: State<'_, AppState>,
    workspace_id: String,
    session_id: String,
) -> Result<String, String> {
    let (workspace, repo, ws_path_str) = {
        let conn = state.db.lock().unwrap();
        resolve_workspace_path(&conn, &workspace_id)?
    };

    let repo_name = repo.name.clone();
    let ws_name = workspace.directory_name.clone();
    let ws_path = ws_path_str.clone();

    // Check if this session is already running in a pane
    let repo_name_find = repo_name.clone();
    let ws_name_find = ws_name.clone();
    let sid = session_id.clone();
    let existing_pane = tokio::task::spawn_blocking(move || {
        tmux::find_pane_with_session(&repo_name_find, &ws_name_find, &sid)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    if existing_pane.is_some() {
        // Session already running — just attach
        let repo_name_attach = repo_name.clone();
        let ws_name_attach = ws_name.clone();
        tokio::task::spawn_blocking(move || {
            terminal::attach_iterm(&repo_name_attach, &ws_name_attach)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        return Ok("attached".to_string());
    }

    // Validate session_id before using it in a shell command
    validate_session_id(&session_id)?;

    // Session not running — resume it
    let skip_perms = workspace.container_mode == ContainerMode::Container
        && should_skip_permissions(&repo);
    let base_cmd = build_claude_cmd(&format!("claude --resume {}", session_id), skip_perms);
    let claude_cmd = if workspace.container_mode == ContainerMode::Container {
        match &workspace.container_id {
            Some(cid) => docker::docker_exec_cmd(cid, &base_cmd).map_err(|e| e.to_string())?,
            None => base_cmd,
        }
    } else {
        base_cmd
    };

    // Try to find an idle pane
    let repo_name_idle = repo_name.clone();
    let ws_name_idle = ws_name.clone();
    let idle_pane = tokio::task::spawn_blocking(move || {
        tmux::find_idle_pane(&repo_name_idle, &ws_name_idle)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    if let Some(pane_index) = idle_pane {
        // Reuse idle pane
        let repo_name_send = repo_name.clone();
        let ws_name_send = ws_name.clone();
        let cmd = claude_cmd.clone();
        tokio::task::spawn_blocking(move || {
            tmux::send_to_pane(&repo_name_send, &ws_name_send, pane_index, &cmd)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    } else {
        // No idle pane — create a new one
        let repo_name_create = repo_name.clone();
        let ws_name_create = ws_name.clone();
        let ws_path_create = ws_path.clone();
        let cmd = claude_cmd.clone();
        tokio::task::spawn_blocking(move || {
            tmux::create_pane(&repo_name_create, &ws_name_create, &ws_path_create, &cmd)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    }

    // Attach iTerm
    let repo_name_attach = repo_name.clone();
    let ws_name_attach = ws_name.clone();
    tokio::task::spawn_blocking(move || {
        terminal::attach_iterm(&repo_name_attach, &ws_name_attach)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    Ok("resumed".to_string())
}

/// Get session history for a workspace (from ~/.claude/projects/).
#[tauri::command]
#[specta::specta]
pub async fn get_workspace_sessions(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Vec<ClaudeSessionEntry>, String> {
    let (workspace, _, ws_path_str) = {
        let conn = state.db.lock().unwrap();
        resolve_workspace_path(&conn, &workspace_id)?
    };

    let container_mode = workspace.container_mode.clone();
    let dir_name = workspace.directory_name.clone();
    tokio::task::spawn_blocking(move || read_sessions(&ws_path_str, &container_mode, &dir_name))
        .await
        .map_err(|e| e.to_string())?
}

/// List panes for a specific workspace.
#[tauri::command]
#[specta::specta]
pub async fn list_workspace_panes(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Vec<TmuxPane>, String> {
    let (workspace, repo, _) = {
        let conn = state.db.lock().unwrap();
        resolve_workspace_path(&conn, &workspace_id)?
    };

    let repo_name = repo.name;
    let ws_name = workspace.directory_name;

    tokio::task::spawn_blocking(move || tmux::list_panes(&repo_name, &ws_name))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Open a shell pane in the workspace window.
#[tauri::command]
#[specta::specta]
pub async fn open_shell_pane(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<String, String> {
    let (workspace, repo, ws_path_str) = {
        let conn = state.db.lock().unwrap();
        resolve_workspace_path(&conn, &workspace_id)?
    };

    let repo_name = repo.name.clone();
    let ws_name = workspace.directory_name.clone();
    let ws_path = ws_path_str.clone();

    // Determine shell command based on container mode
    let shell_cmd = if workspace.container_mode == ContainerMode::Container {
        match workspace.container_id.as_ref() {
            Some(cid) => Some(docker::docker_exec_cmd(cid, "/bin/bash").map_err(|e| e.to_string())?),
            None => None,
        }
    } else {
        None
    };

    // Ensure workspace window exists, then split a new shell pane
    let repo_name_create = repo_name.clone();
    let ws_name_create = ws_name.clone();
    let ws_path_create = ws_path.clone();
    tokio::task::spawn_blocking(move || {
        tmux::ensure_workspace_window(&repo_name_create, &ws_name_create, &ws_path_create)?;
        let target = format!("{}:{}", repo_name_create, ws_name_create);
        let mut args = vec!["-L", "bunyan", "split-window", "-h", "-t", &target, "-c", &ws_path_create];
        let cmd_ref;
        if let Some(ref cmd) = shell_cmd {
            cmd_ref = cmd.as_str();
            args.push(cmd_ref);
        }
        let output = std::process::Command::new("tmux")
            .args(&args)
            .output()
            .map_err(|e| bunyan_core::error::BunyanError::Process(format!("Failed to split window: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(bunyan_core::error::BunyanError::Process(format!(
                "tmux split-window failed: {}",
                stderr
            )));
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    // Attach iTerm
    let repo_name_attach = repo_name.clone();
    let ws_name_attach = ws_name.clone();
    tokio::task::spawn_blocking(move || {
        terminal::attach_iterm(&repo_name_attach, &ws_name_attach)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    Ok("created".to_string())
}

/// View a workspace in iTerm — ensures the tmux window exists and attaches
/// without creating any new panes.
#[tauri::command]
#[specta::specta]
pub async fn view_workspace(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<String, String> {
    let (workspace, repo, ws_path_str) = {
        let conn = state.db.lock().unwrap();
        resolve_workspace_path(&conn, &workspace_id)?
    };

    let repo_name = repo.name.clone();
    let ws_name = workspace.directory_name.clone();
    let ws_path = ws_path_str.clone();

    tokio::task::spawn_blocking(move || {
        tmux::ensure_workspace_window(&repo_name, &ws_name, &ws_path)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let repo_name_attach = repo.name.clone();
    let ws_name_attach = workspace.directory_name.clone();
    tokio::task::spawn_blocking(move || {
        terminal::attach_iterm(&repo_name_attach, &ws_name_attach)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    Ok("attached".to_string())
}

/// Kill a specific pane in a workspace window.
#[tauri::command]
#[specta::specta]
pub async fn kill_pane(
    state: State<'_, AppState>,
    workspace_id: String,
    pane_index: u32,
) -> Result<String, String> {
    let (workspace, repo, _) = {
        let conn = state.db.lock().unwrap();
        resolve_workspace_path(&conn, &workspace_id)?
    };

    let repo_name = repo.name;
    let ws_name = workspace.directory_name;

    tokio::task::spawn_blocking(move || tmux::kill_pane(&repo_name, &ws_name, pane_index))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    Ok("killed".to_string())
}

/// Kill the entire tmux window for a workspace (used before archiving).
pub fn kill_workspace_window(repo_name: &str, workspace_name: &str) {
    let _ = tmux::kill_window(repo_name, workspace_name);
}

/// Detect which editors/IDEs are installed on the system.
#[tauri::command]
#[specta::specta]
pub async fn detect_editors() -> Result<Vec<String>, String> {
    let editors = tokio::task::spawn_blocking(|| editor::detect_installed_editors())
        .await
        .map_err(|e| e.to_string())?;

    Ok(editors.iter().map(|e| e.id().to_string()).collect())
}

/// Open a workspace folder in a specific editor/IDE.
#[tauri::command]
#[specta::specta]
pub async fn open_in_editor(
    state: State<'_, AppState>,
    workspace_id: String,
    editor_id: String,
) -> Result<String, String> {
    let ed = editor::Editor::from_id(&editor_id)
        .ok_or_else(|| format!("Unknown editor: {}", editor_id))?;

    let (workspace, repo, ws_path_str) = {
        let conn = state.db.lock().unwrap();
        resolve_workspace_path(&conn, &workspace_id)?
    };

    // For iTerm, use the existing tmux+iTerm flow
    if ed == editor::Editor::Iterm {
        let repo_name = repo.name.clone();
        let ws_name = workspace.directory_name.clone();
        let ws_path = ws_path_str.clone();

        tokio::task::spawn_blocking(move || {
            tmux::ensure_workspace_window(&repo_name, &ws_name, &ws_path)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        let repo_name_attach = repo.name.clone();
        let ws_name_attach = workspace.directory_name.clone();
        tokio::task::spawn_blocking(move || {
            terminal::attach_iterm(&repo_name_attach, &ws_name_attach)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        return Ok("attached".to_string());
    }

    // For other editors, open the workspace folder
    let path = ws_path_str.clone();
    tokio::task::spawn_blocking(move || editor::open_in_editor(&ed, &path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    Ok("opened".to_string())
}
