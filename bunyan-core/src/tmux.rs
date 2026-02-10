use std::process::Command;

use crate::error::{BunyanError, Result};
use crate::models::TmuxPane;

const TMUX_SOCKET: &str = "bunyan";

fn tmux_cmd() -> Command {
    let mut cmd = Command::new("tmux");
    cmd.args(["-L", TMUX_SOCKET]);
    cmd
}

/// Check if a tmux session exists for the given repo.
pub fn session_exists(repo_name: &str) -> bool {
    tmux_cmd()
        .args(["has-session", "-t", repo_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if a window exists for the given workspace within a repo session.
pub fn window_exists(repo_name: &str, workspace_name: &str) -> bool {
    let target = format!("{}:{}", repo_name, workspace_name);
    tmux_cmd()
        .args(["select-window", "-t", &target])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Ensure a tmux session exists for the repo and a window exists for the workspace.
/// Creates them if they don't exist. Returns Ok(()) on success.
pub fn ensure_workspace_window(
    repo_name: &str,
    workspace_name: &str,
    workspace_path: &str,
) -> Result<()> {
    if !session_exists(repo_name) {
        // Create session with the workspace as the first window
        let output = tmux_cmd()
            .args([
                "new-session",
                "-d",
                "-s",
                repo_name,
                "-n",
                workspace_name,
                "-c",
                workspace_path,
            ])
            .output()
            .map_err(|e| BunyanError::Process(format!("Failed to create tmux session: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BunyanError::Process(format!(
                "tmux new-session failed: {}",
                stderr
            )));
        }
        return Ok(());
    }

    if !window_exists(repo_name, workspace_name) {
        let output = tmux_cmd()
            .args([
                "new-window",
                "-t",
                repo_name,
                "-n",
                workspace_name,
                "-c",
                workspace_path,
            ])
            .output()
            .map_err(|e| BunyanError::Process(format!("Failed to create tmux window: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BunyanError::Process(format!(
                "tmux new-window failed: {}",
                stderr
            )));
        }
    }

    Ok(())
}

/// Create a new pane in the workspace window running the given command.
/// If the window doesn't exist, creates it with the command as the initial pane.
/// If the window exists, splits to create a new pane.
pub fn create_pane(
    repo_name: &str,
    workspace_name: &str,
    workspace_path: &str,
    cmd: &str,
) -> Result<()> {
    if !session_exists(repo_name) || !window_exists(repo_name, workspace_name) {
        // Create session/window with command as the initial pane
        ensure_workspace_window(repo_name, workspace_name, workspace_path)?;
        let target = format!("{}:{}", repo_name, workspace_name);
        let output = tmux_cmd()
            .args(["send-keys", "-t", &target, cmd, "Enter"])
            .output()
            .map_err(|e| BunyanError::Process(format!("Failed to send keys: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BunyanError::Process(format!(
                "tmux send-keys failed: {}",
                stderr
            )));
        }
    } else {
        // Window exists — split to create new pane
        let target = format!("{}:{}", repo_name, workspace_name);
        let output = tmux_cmd()
            .args([
                "split-window",
                "-h",
                "-t",
                &target,
                "-c",
                workspace_path,
                cmd,
            ])
            .output()
            .map_err(|e| BunyanError::Process(format!("Failed to split window: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BunyanError::Process(format!(
                "tmux split-window failed: {}",
                stderr
            )));
        }
    }

    Ok(())
}

/// Send a command to an idle pane (one running a shell, not claude).
pub fn send_to_pane(
    repo_name: &str,
    workspace_name: &str,
    pane_index: u32,
    cmd: &str,
) -> Result<()> {
    let target = format!("{}:{}.{}", repo_name, workspace_name, pane_index);
    let output = tmux_cmd()
        .args(["send-keys", "-t", &target, cmd, "Enter"])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to send keys: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BunyanError::Process(format!(
            "tmux send-keys failed: {}",
            stderr
        )));
    }

    Ok(())
}

/// List all panes in a workspace window.
pub fn list_panes(repo_name: &str, workspace_name: &str) -> Result<Vec<TmuxPane>> {
    let target = format!("{}:{}", repo_name, workspace_name);
    let output = tmux_cmd()
        .args([
            "list-panes",
            "-t",
            &target,
            "-F",
            "#{pane_index}|#{pane_current_command}|#{pane_active}|#{pane_current_path}|#{pane_pid}",
        ])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to list panes: {}", e)))?;

    if !output.status.success() {
        // Window doesn't exist — return empty list
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let panes = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(5, '|').collect();
            if parts.len() < 5 {
                return None;
            }
            Some(TmuxPane {
                pane_index: parts[0].parse().unwrap_or(0),
                command: parts[1].to_string(),
                is_active: parts[2] == "1",
                workspace_path: parts[3].to_string(),
                pane_pid: parts[4].parse().unwrap_or(0),
            })
        })
        .collect();

    Ok(panes)
}

/// List all panes across the entire bunyan tmux server.
/// Returns tuples of (session_name, window_name, TmuxPane).
pub fn list_all_panes() -> Result<Vec<(String, String, TmuxPane)>> {
    let output = tmux_cmd()
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{session_name}|#{window_name}|#{pane_index}|#{pane_current_command}|#{pane_active}|#{pane_current_path}|#{pane_pid}",
        ])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to list all panes: {}", e)))?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let panes = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(7, '|').collect();
            if parts.len() < 7 {
                return None;
            }
            Some((
                parts[0].to_string(),
                parts[1].to_string(),
                TmuxPane {
                    pane_index: parts[2].parse().unwrap_or(0),
                    command: parts[3].to_string(),
                    is_active: parts[4] == "1",
                    workspace_path: parts[5].to_string(),
                    pane_pid: parts[6].parse().unwrap_or(0),
                },
            ))
        })
        .collect();

    Ok(panes)
}

/// Find an idle pane (running a shell, not claude) in a workspace window.
pub fn find_idle_pane(repo_name: &str, workspace_name: &str) -> Result<Option<u32>> {
    let panes = list_panes(repo_name, workspace_name)?;
    let shells = ["zsh", "bash", "fish", "sh"];
    for pane in &panes {
        if shells.iter().any(|s| pane.command == *s) {
            return Ok(Some(pane.pane_index));
        }
    }
    Ok(None)
}

/// Check if any pane in the workspace window is running claude.
/// Claude CLI reports its version (e.g. "2.1.33") as pane_current_command,
/// so we detect it as any pane not running a known shell.
pub fn has_claude_running(repo_name: &str, workspace_name: &str) -> Result<bool> {
    let panes = list_panes(repo_name, workspace_name)?;
    let shells = ["zsh", "bash", "fish", "sh"];
    Ok(panes.iter().any(|p| !shells.iter().any(|s| p.command == *s)))
}

/// Get the claude session ID running in a pane, if any.
/// Checks the pane PID's own args first (for panes started with an explicit command),
/// then falls back to checking child processes (for panes started via send-keys to a shell).
pub fn get_pane_session_id(pane_pid: u32) -> Option<String> {
    let pid_str = pane_pid.to_string();

    // Check the pane process itself (tmux runs the command directly when using split-window)
    if let Some(id) = extract_session_id_from_pid(&pid_str) {
        return Some(id);
    }

    // Fall back to child processes (pane started as a shell, claude launched via send-keys)
    let output = Command::new("pgrep")
        .args(["-P", &pid_str])
        .output()
        .ok()?;

    for child_pid in std::str::from_utf8(&output.stdout).ok()?.lines() {
        if let Some(id) = extract_session_id_from_pid(child_pid) {
            return Some(id);
        }
    }

    None
}

fn extract_session_id_from_pid(pid: &str) -> Option<String> {
    let output = Command::new("ps")
        .args(["-p", pid, "-o", "args="])
        .output()
        .ok()?;
    let args = std::str::from_utf8(&output.stdout).ok()?.trim().to_string();
    if let Some(id) = args.strip_prefix("claude --resume ") {
        return Some(id.trim().to_string());
    }
    None
}

/// Find a pane in the workspace that is running a specific claude session ID.
/// Returns the pane index if found.
pub fn find_pane_with_session(
    repo_name: &str,
    workspace_name: &str,
    session_id: &str,
) -> Result<Option<u32>> {
    let panes = list_panes(repo_name, workspace_name)?;
    let shells = ["zsh", "bash", "fish", "sh"];

    for pane in &panes {
        if shells.iter().any(|s| pane.command == *s) {
            continue;
        }
        if let Some(running_session_id) = get_pane_session_id(pane.pane_pid) {
            if running_session_id == session_id {
                return Ok(Some(pane.pane_index));
            }
        }
    }

    Ok(None)
}

/// Kill a specific pane.
pub fn kill_pane(repo_name: &str, workspace_name: &str, pane_index: u32) -> Result<()> {
    let target = format!("{}:{}.{}", repo_name, workspace_name, pane_index);
    let output = tmux_cmd()
        .args(["kill-pane", "-t", &target])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to kill pane: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BunyanError::Process(format!(
            "tmux kill-pane failed: {}",
            stderr
        )));
    }

    Ok(())
}

/// Kill an entire workspace window (all panes).
pub fn kill_window(repo_name: &str, workspace_name: &str) -> Result<()> {
    let target = format!("{}:{}", repo_name, workspace_name);
    let output = tmux_cmd()
        .args(["kill-window", "-t", &target])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to kill window: {}", e)))?;

    // Ignore failures — window may not exist
    if !output.status.success() {
        // Not an error if window doesn't exist
    }

    Ok(())
}

/// Select a specific window (bring it into focus within tmux).
pub fn select_window(repo_name: &str, workspace_name: &str) -> Result<()> {
    let target = format!("{}:{}", repo_name, workspace_name);
    let _ = tmux_cmd()
        .args(["select-window", "-t", &target])
        .output();
    Ok(())
}

/// Get the tmux attach command string for use in iTerm.
pub fn attach_command(repo_name: &str) -> String {
    format!("tmux -L {} attach-session -t {}", TMUX_SOCKET, repo_name)
}

/// Get TTYs of clients attached to a specific session on the bunyan tmux server.
pub fn list_client_ttys_for_session(repo_name: &str) -> Result<Vec<String>> {
    let output = tmux_cmd()
        .args(["list-clients", "-t", repo_name, "-F", "#{client_tty}"])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to list clients: {}", e)))?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
}
