//! Tmux backend.
//!
//! The behavior here is the original `crate::tmux` module behind the
//! `RuntimeBackend` trait. Routes used to call free functions like
//! `tmux::ensure_workspace_window`; now they call `backend.ensure_workspace`
//! on the backend the app booted with.

use std::process::Command;

use super::{ProcessHandle, ProcessInfo, RuntimeBackend};
use crate::error::{BunyanError, Result};

const TMUX_SOCKET: &str = "bunyan";
const TITLE_FORMAT: &str = "Bunyan: #S / #W";

#[derive(Default)]
pub struct TmuxBackend;

impl TmuxBackend {
    pub fn new() -> Self {
        Self
    }
}

fn tmux_cmd() -> Command {
    let mut cmd = Command::new("tmux");
    cmd.args(["-L", TMUX_SOCKET]);
    cmd
}

fn configure_titles(repo_name: &str) {
    let _ = tmux_cmd()
        .args(["set-option", "-t", repo_name, "set-titles", "on"])
        .output();
    let _ = tmux_cmd()
        .args(["set-option", "-t", repo_name, "set-titles-string", TITLE_FORMAT])
        .output();
}

fn session_exists(repo_name: &str) -> bool {
    tmux_cmd()
        .args(["has-session", "-t", repo_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn window_exists(repo_name: &str, workspace_name: &str) -> bool {
    let target = format!("{}:{}", repo_name, workspace_name);
    tmux_cmd()
        .args(["select-window", "-t", &target])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn make_handle(repo: &str, workspace: &str, slot_index: u32) -> ProcessHandle {
    format!("tmux:{}/{}/{}", TMUX_SOCKET, repo, workspace) + &format!(":{}", slot_index)
}

fn parse_pane_line(line: &str) -> Option<ProcessInfo> {
    // Format: pane_index|pane_current_command|pane_active|pane_current_path|pane_pid
    let parts: Vec<&str> = line.splitn(5, '|').collect();
    if parts.len() < 5 {
        return None;
    }
    let slot_index: u32 = parts[0].parse().unwrap_or(0);
    Some(ProcessInfo {
        // Handle is built once we know the workspace.
        handle: String::new(),
        command: parts[1].to_string(),
        is_active: parts[2] == "1",
        cwd: parts[3].to_string(),
        pid: parts[4].parse().unwrap_or(0),
        slot_index,
    })
}

fn parse_all_panes_line(line: &str) -> Option<(String, String, ProcessInfo)> {
    // Format: session_name|window_name|pane_index|pane_current_command|pane_active|pane_current_path|pane_pid
    let parts: Vec<&str> = line.splitn(7, '|').collect();
    if parts.len() < 7 {
        return None;
    }
    let info = ProcessInfo {
        handle: String::new(),
        command: parts[3].to_string(),
        is_active: parts[4] == "1",
        cwd: parts[5].to_string(),
        pid: parts[6].parse().unwrap_or(0),
        slot_index: parts[2].parse().unwrap_or(0),
    };
    Some((parts[0].to_string(), parts[1].to_string(), info))
}

impl RuntimeBackend for TmuxBackend {
    fn name(&self) -> &'static str {
        "tmux"
    }

    fn ensure_workspace(
        &self,
        repo_name: &str,
        workspace_name: &str,
        workspace_path: &str,
    ) -> Result<()> {
        if !session_exists(repo_name) {
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
                .map_err(|e| {
                    BunyanError::Process(format!("Failed to create tmux session: {}", e))
                })?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(BunyanError::Process(format!(
                    "tmux new-session failed: {}",
                    stderr
                )));
            }
            configure_titles(repo_name);
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
                .map_err(|e| {
                    BunyanError::Process(format!("Failed to create tmux window: {}", e))
                })?;
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

    fn kill_workspace(&self, repo_name: &str, workspace_name: &str) -> Result<()> {
        let target = format!("{}:{}", repo_name, workspace_name);
        let _ = tmux_cmd().args(["kill-window", "-t", &target]).output();
        Ok(())
    }

    fn list_processes(
        &self,
        repo_name: &str,
        workspace_name: &str,
    ) -> Result<Vec<ProcessInfo>> {
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
            return Ok(vec![]);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let processes = stdout
            .lines()
            .filter_map(|line| parse_pane_line(line))
            .map(|mut p| {
                p.handle = make_handle(repo_name, workspace_name, p.slot_index);
                p
            })
            .collect();
        Ok(processes)
    }

    fn list_all_processes(&self) -> Result<Vec<(String, String, ProcessInfo)>> {
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
        let processes = stdout
            .lines()
            .filter_map(parse_all_panes_line)
            .map(|(sess, win, mut p)| {
                p.handle = make_handle(&sess, &win, p.slot_index);
                (sess, win, p)
            })
            .collect();
        Ok(processes)
    }

    fn spawn(
        &self,
        repo_name: &str,
        workspace_name: &str,
        workspace_path: &str,
        cmd: &str,
    ) -> Result<()> {
        if !session_exists(repo_name) || !window_exists(repo_name, workspace_name) {
            self.ensure_workspace(repo_name, workspace_name, workspace_path)?;
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

    fn send_to_slot(
        &self,
        repo_name: &str,
        workspace_name: &str,
        slot_index: u32,
        cmd: &str,
    ) -> Result<()> {
        let target = format!("{}:{}.{}", repo_name, workspace_name, slot_index);
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

    fn kill_slot(&self, repo_name: &str, workspace_name: &str, slot_index: u32) -> Result<()> {
        let target = format!("{}:{}.{}", repo_name, workspace_name, slot_index);
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

    fn attach_command(&self, repo_name: &str) -> String {
        format!("tmux -L {} attach-session -t {}", TMUX_SOCKET, repo_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_command_uses_bunyan_socket() {
        let backend = TmuxBackend::new();
        assert_eq!(
            backend.attach_command("frontend"),
            "tmux -L bunyan attach-session -t frontend"
        );
    }

    #[test]
    fn name_is_tmux() {
        assert_eq!(TmuxBackend::new().name(), "tmux");
    }

    #[test]
    fn parse_pane_line_extracts_all_fields() {
        let info = parse_pane_line("0|claude|1|/tmp/x|1234").unwrap();
        assert_eq!(info.slot_index, 0);
        assert_eq!(info.command, "claude");
        assert!(info.is_active);
        assert_eq!(info.cwd, "/tmp/x");
        assert_eq!(info.pid, 1234);
    }

    #[test]
    fn parse_pane_line_returns_none_on_malformed_input() {
        assert!(parse_pane_line("nope").is_none());
        assert!(parse_pane_line("0|zsh|").is_none());
    }

    #[test]
    fn parse_all_panes_line_extracts_session_and_window() {
        let (sess, win, info) =
            parse_all_panes_line("front|fix-bug|1|zsh|0|/p|9999").unwrap();
        assert_eq!(sess, "front");
        assert_eq!(win, "fix-bug");
        assert_eq!(info.slot_index, 1);
        assert_eq!(info.command, "zsh");
        assert!(!info.is_active);
        assert_eq!(info.cwd, "/p");
        assert_eq!(info.pid, 9999);
    }

    #[test]
    fn make_handle_includes_backend_socket_and_indexes() {
        let h = make_handle("frontend", "fix-bug", 2);
        assert_eq!(h, "tmux:bunyan/frontend/fix-bug:2");
    }
}
