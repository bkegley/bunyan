use std::path::{Path, PathBuf};
use tauri::State;

use crate::db;
use crate::models::{ClaudeSession, ClaudeSessionEntry};
use crate::process::{ProcessDetector, RealProcessDetector};
use crate::state::AppState;
use crate::terminal;

/// Resolve the filesystem path for a workspace from DB records.
fn resolve_workspace_path(
    conn: &rusqlite::Connection,
    workspace_id: &str,
) -> Result<(crate::models::Workspace, crate::models::Repo, String), String> {
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
fn read_sessions(workspace_path: &str) -> Result<Vec<ClaudeSessionEntry>, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let sanitized = workspace_path.replace('/', "-");
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

/// Open a claude session in the preferred terminal (tmux or iTerm).
fn open_in_terminal(workspace_path: &str, session_name: &str, claude_cmd: &str) -> Result<(), String> {
    let tmux_check = std::process::Command::new("tmux")
        .args(["list-sessions"])
        .output();

    match tmux_check {
        Ok(output) if output.status.success() => {
            terminal::open_tmux_session(workspace_path, session_name, claude_cmd)
        }
        _ => terminal::open_iterm_session(workspace_path, claude_cmd),
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_active_claude_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<ClaudeSession>, String> {
    let pids = tokio::task::spawn_blocking(|| {
        let detector = RealProcessDetector;
        detector.find_claude_pids()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let pids_clone = pids.clone();
    let pid_info: Vec<(u32, Option<String>, Option<String>)> =
        tokio::task::spawn_blocking(move || {
            let detector = RealProcessDetector;
            pids_clone
                .into_iter()
                .filter_map(|pid| {
                    let cwd = detector.get_pid_cwd(pid).ok()?;
                    let tty = detector.get_pid_tty(pid).ok().flatten();
                    Some((pid, Some(cwd), tty))
                })
                .collect()
        })
        .await
        .map_err(|e| e.to_string())?;

    let mut sessions = Vec::new();
    for (pid, cwd, tty) in pid_info {
        let cwd = match cwd {
            Some(c) => c,
            None => continue,
        };

        let workspace_id = {
            let conn = state.db.lock().unwrap();
            find_workspace_by_path(&conn, &cwd).ok()
        };

        sessions.push(ClaudeSession {
            pid,
            workspace_path: cwd,
            workspace_id,
            tty,
        });
    }

    Ok(sessions)
}

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

    // Check for existing claude process at this path
    let check_path = ws_path_str.clone();
    let found = tokio::task::spawn_blocking(move || {
        let detector = RealProcessDetector;
        let pids = detector.find_claude_pids().unwrap_or_default();

        for pid in pids {
            if let Ok(cwd) = detector.get_pid_cwd(pid) {
                if cwd == check_path {
                    let tty = detector.get_pid_tty(pid).ok().flatten();
                    return Some((pid, tty));
                }
            }
        }
        None
    })
    .await
    .map_err(|e| e.to_string())?;

    if let Some((_pid, tty)) = found {
        if let Some(tty) = tty {
            let tty_clone = tty.clone();
            let focused = tokio::task::spawn_blocking(move || {
                terminal::focus_tmux_pane(&tty_clone).unwrap_or(false)
            })
            .await
            .map_err(|e| e.to_string())?;

            if focused {
                terminal::activate_terminal_app();
                return Ok("focused".to_string());
            }

            let focused = tokio::task::spawn_blocking(move || {
                terminal::focus_iterm_session(&tty).unwrap_or(false)
            })
            .await
            .map_err(|e| e.to_string())?;

            if focused {
                return Ok("focused".to_string());
            }
        }
        // Process exists but couldn't focus its terminal — still activate
        terminal::activate_terminal_app();
        return Ok("running".to_string());
    }

    // Check if a previous session exists for --continue
    let resume_path = ws_path_str.clone();
    let has_previous = tokio::task::spawn_blocking(move || has_existing_session(&resume_path))
        .await
        .map_err(|e| e.to_string())?;

    let claude_cmd = if has_previous {
        "claude --continue".to_string()
    } else {
        "claude".to_string()
    };

    let session_name = format!("{}-{}", repo.name, workspace.directory_name);
    let open_path = ws_path_str.clone();

    tokio::task::spawn_blocking(move || open_in_terminal(&open_path, &session_name, &claude_cmd))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    Ok("created".to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_workspace_sessions(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Vec<ClaudeSessionEntry>, String> {
    let ws_path_str = {
        let conn = state.db.lock().unwrap();
        let (_, _, path) = resolve_workspace_path(&conn, &workspace_id)?;
        path
    };

    tokio::task::spawn_blocking(move || read_sessions(&ws_path_str))
        .await
        .map_err(|e| e.to_string())?
}

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

    // Check for existing claude process at this workspace path first.
    // If one is running, focus it instead of opening a new terminal —
    // `claude --resume` on an already-running session creates a new empty session.
    let check_path = ws_path_str.clone();
    let found = tokio::task::spawn_blocking(move || {
        let detector = RealProcessDetector;
        let pids = detector.find_claude_pids().unwrap_or_default();

        for pid in pids {
            if let Ok(cwd) = detector.get_pid_cwd(pid) {
                if cwd == check_path {
                    let tty = detector.get_pid_tty(pid).ok().flatten();
                    return Some((pid, tty));
                }
            }
        }
        None
    })
    .await
    .map_err(|e| e.to_string())?;

    if let Some((_pid, tty)) = found {
        if let Some(tty) = tty {
            let tty_clone = tty.clone();
            let focused = tokio::task::spawn_blocking(move || {
                terminal::focus_tmux_pane(&tty_clone).unwrap_or(false)
            })
            .await
            .map_err(|e| e.to_string())?;

            if focused {
                terminal::activate_terminal_app();
                return Ok("focused".to_string());
            }

            let focused = tokio::task::spawn_blocking(move || {
                terminal::focus_iterm_session(&tty).unwrap_or(false)
            })
            .await
            .map_err(|e| e.to_string())?;

            if focused {
                return Ok("focused".to_string());
            }
        }
        terminal::activate_terminal_app();
        return Ok("running".to_string());
    }

    let session_name = format!("{}-{}", repo.name, workspace.directory_name);
    let claude_cmd = format!("claude --resume {}", session_id);

    tokio::task::spawn_blocking(move || open_in_terminal(&ws_path_str, &session_name, &claude_cmd))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    Ok("resumed".to_string())
}

fn find_workspace_by_path(
    conn: &rusqlite::Connection,
    path: &str,
) -> crate::error::Result<String> {
    let workspaces = db::workspaces::list(conn, None)?;
    let repos = db::repos::list(conn)?;

    for ws in &workspaces {
        if let Some(repo) = repos.iter().find(|r| r.id == ws.repository_id) {
            let base = Path::new(&repo.root_path)
                .parent()
                .and_then(|p| p.parent());
            if let Some(base) = base {
                let ws_path = base
                    .join("workspaces")
                    .join(&repo.name)
                    .join(&ws.directory_name);
                if let Some(ws_path_str) = ws_path.to_str() {
                    if ws_path_str == path {
                        return Ok(ws.id.clone());
                    }
                }
            }
        }
    }

    Err(crate::error::BunyanError::NotFound(format!(
        "No workspace found for path: {}",
        path
    )))
}

fn has_existing_session(workspace_path: &str) -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };

    let sanitized = workspace_path.replace('/', "-");
    let sessions_path = home
        .join(".claude")
        .join("projects")
        .join(&sanitized)
        .join("sessions-index.json");

    sessions_path.exists()
}
