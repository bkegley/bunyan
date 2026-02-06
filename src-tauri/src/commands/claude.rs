use std::path::{Path, PathBuf};
use tauri::State;

use crate::db;
use crate::models::ClaudeSession;
use crate::process::{ProcessDetector, RealProcessDetector};
use crate::state::AppState;
use crate::terminal;

#[tauri::command]
pub fn get_active_claude_sessions(state: State<AppState>) -> Result<Vec<ClaudeSession>, String> {
    let detector = RealProcessDetector;
    let pids = detector
        .find_claude_pids()
        .map_err(|e| e.to_string())?;

    let mut sessions = Vec::new();

    for pid in pids {
        let cwd = match detector.get_pid_cwd(pid) {
            Ok(path) => path,
            Err(_) => continue,
        };

        let tty = detector.get_pid_tty(pid).ok().flatten();

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
pub fn open_claude_session(state: State<AppState>, workspace_id: String) -> Result<String, String> {
    let (workspace, repo) = {
        let conn = state.db.lock().unwrap();
        let ws = db::workspaces::get(&conn, &workspace_id).map_err(|e| e.to_string())?;
        let rp = db::repos::get(&conn, &ws.repository_id).map_err(|e| e.to_string())?;
        (ws, rp)
    };

    let base = PathBuf::from(&repo.root_path)
        .parent()
        .and_then(|p| p.parent())
        .ok_or("Invalid repo root path")?
        .to_path_buf();
    let ws_path = base
        .join("workspaces")
        .join(&repo.name)
        .join(&workspace.directory_name);
    let ws_path_str = ws_path.to_str().ok_or("Invalid workspace path")?;

    // Check for existing claude process at this path
    let detector = RealProcessDetector;
    let pids = detector.find_claude_pids().map_err(|e| e.to_string())?;

    for pid in pids {
        if let Ok(cwd) = detector.get_pid_cwd(pid) {
            if cwd == ws_path_str {
                // Already running — try to focus it
                if let Ok(Some(tty)) = detector.get_pid_tty(pid) {
                    if terminal::focus_tmux_pane(&tty).unwrap_or(false) {
                        return Ok("focused".to_string());
                    }
                    if terminal::focus_iterm_session(&tty).unwrap_or(false) {
                        return Ok("focused".to_string());
                    }
                }
                return Ok("running".to_string());
            }
        }
    }

    // Check if a previous session exists for --continue
    let resume = has_existing_session(ws_path_str);

    // Try to detect preferred terminal and open session
    // Prefer tmux if it's running, otherwise iTerm
    let tmux_check = std::process::Command::new("tmux")
        .args(["list-sessions"])
        .output();

    let session_name = format!("{}-{}", repo.name, workspace.directory_name);

    match tmux_check {
        Ok(output) if output.status.success() => {
            terminal::open_tmux_session(ws_path_str, &session_name, resume)
                .map_err(|e| e.to_string())?;
        }
        _ => {
            terminal::open_iterm_session(ws_path_str, resume).map_err(|e| e.to_string())?;
        }
    }

    Ok("created".to_string())
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
