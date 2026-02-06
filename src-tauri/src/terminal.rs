use std::process::Command;

use crate::error::{BunyanError, Result};

pub fn focus_tmux_pane(tty: &str) -> Result<bool> {
    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{session_name}:#{window_index}.#{pane_index}|#{pane_tty}",
        ])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to run tmux list-panes: {}", e)))?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(2, '|').collect();
        if parts.len() == 2 && parts[1].contains(tty) {
            let target = parts[0];
            // Parse session:window.pane
            if let Some(dot_pos) = target.rfind('.') {
                let session_window = &target[..dot_pos];
                let _ = Command::new("tmux")
                    .args(["select-window", "-t", session_window])
                    .output();
                let _ = Command::new("tmux")
                    .args(["select-pane", "-t", target])
                    .output();
                return Ok(true);
            }
        }
    }

    Ok(false)
}

pub fn focus_iterm_session(tty: &str) -> Result<bool> {
    let script = format!(
        r#"tell application "iTerm"
    repeat with w in windows
        repeat with t in tabs of w
            repeat with s in sessions of t
                if tty of s contains "{}" then
                    select t
                    tell w to activate
                    return "found"
                end if
            end repeat
        end repeat
    end repeat
    return "not_found"
end tell"#,
        tty
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to run osascript: {}", e)))?;

    if !output.status.success() {
        return Ok(false);
    }

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(result == "found")
}

pub fn open_iterm_session(workspace_path: &str, resume: bool) -> Result<()> {
    let claude_cmd = if resume {
        "claude --continue"
    } else {
        "claude"
    };

    let script = format!(
        r#"tell application "iTerm"
    activate
    if (count of windows) = 0 then
        set newWindow to (create window with default profile)
        tell current session of newWindow
            write text "cd '{}' && {}"
        end tell
    else
        tell current window
            create tab with default profile
            tell current session
                write text "cd '{}' && {}"
            end tell
        end tell
    end if
end tell"#,
        workspace_path, claude_cmd, workspace_path, claude_cmd
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| BunyanError::Process(format!("Failed to run osascript: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BunyanError::Process(format!(
            "osascript failed: {}",
            stderr
        )));
    }

    Ok(())
}

pub fn open_tmux_session(workspace_path: &str, name: &str, resume: bool) -> Result<()> {
    let claude_cmd = if resume {
        "claude --continue"
    } else {
        "claude"
    };

    // Check if tmux has any sessions
    let check = Command::new("tmux").args(["list-sessions"]).output();

    match check {
        Ok(output) if output.status.success() => {
            // tmux is running — create new window in first session
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(session_name) = stdout.lines().next().and_then(|l| l.split(':').next()) {
                Command::new("tmux")
                    .args([
                        "new-window",
                        "-t",
                        session_name,
                        "-n",
                        name,
                        "-c",
                        workspace_path,
                        claude_cmd,
                    ])
                    .output()
                    .map_err(|e| {
                        BunyanError::Process(format!("Failed to run tmux new-window: {}", e))
                    })?;
            }
        }
        _ => {
            // No tmux session — create one
            Command::new("tmux")
                .args([
                    "new-session",
                    "-d",
                    "-s",
                    "bunyan",
                    "-c",
                    workspace_path,
                    "-n",
                    name,
                    claude_cmd,
                ])
                .output()
                .map_err(|e| {
                    BunyanError::Process(format!("Failed to run tmux new-session: {}", e))
                })?;
        }
    }

    Ok(())
}
