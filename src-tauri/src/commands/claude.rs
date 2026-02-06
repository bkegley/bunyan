use std::path::{Path, PathBuf};
use tauri::State;

use crate::db;
use crate::models::ClaudeSession;
use crate::process::{ProcessDetector, RealProcessDetector};
use crate::state::AppState;
use crate::terminal;

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
    let ws_path_str = ws_path
        .to_str()
        .ok_or("Invalid workspace path")?
        .to_string();

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
        return Ok("running".to_string());
    }

    // Check if a previous session exists for --continue
    let resume_path = ws_path_str.clone();
    let resume = tokio::task::spawn_blocking(move || has_existing_session(&resume_path))
        .await
        .map_err(|e| e.to_string())?;

    // Detect preferred terminal and open session
    let session_name = format!("{}-{}", repo.name, workspace.directory_name);
    let open_path = ws_path_str.clone();

    tokio::task::spawn_blocking(move || {
        let tmux_check = std::process::Command::new("tmux")
            .args(["list-sessions"])
            .output();

        match tmux_check {
            Ok(output) if output.status.success() => {
                terminal::open_tmux_session(&open_path, &session_name, resume)
            }
            _ => terminal::open_iterm_session(&open_path, resume),
        }
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

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
