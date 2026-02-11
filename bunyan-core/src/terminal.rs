use std::process::Command;

use crate::error::{BunyanError, Result};
use crate::tmux;

/// Attach iTerm to the bunyan tmux session for a repo.
/// First tries to focus an existing iTerm window already attached to this session.
/// Only opens a new iTerm window if no existing attachment is found.
pub fn attach_iterm(repo_name: &str, workspace_name: &str) -> Result<()> {
    // Select the workspace window before attaching/focusing
    tmux::select_window(repo_name, workspace_name)?;

    // Try to reuse an existing iTerm window already attached to this repo's session
    let client_ttys = tmux::list_client_ttys_for_session(repo_name)?;
    if !client_ttys.is_empty() {
        if focus_iterm_by_tty(&client_ttys)? {
            return Ok(());
        }
    }

    // No existing attachment — open a new iTerm window
    let attach_cmd = tmux::attach_command(repo_name);
    let session_name = format!("Bunyan: {} / {}", repo_name, workspace_name);
    let script = format!(
        r#"tell application "iTerm"
    activate
    set newWindow to (create window with default profile)
    tell current session of newWindow
        set name to "{}"
        write text "{}"
    end tell
end tell"#,
        session_name, attach_cmd
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

/// Find an iTerm session whose TTY matches one of the tmux client TTYs,
/// then focus that window. Returns true if found.
fn focus_iterm_by_tty(ttys: &[String]) -> Result<bool> {
    // Build a comma-delimited string of TTYs for matching via AppleScript `contains`
    let tty_match_str: String = ttys.iter().map(|t| format!("{},", t)).collect();

    let script = format!(
        r#"tell application "iTerm"
    set ttyMatch to "{}"
    repeat with w in windows
        repeat with t in tabs of w
            repeat with s in sessions of t
                if ttyMatch contains ((tty of s) & ",") then
                    select t
                    tell w to activate
                    return "found"
                end if
            end repeat
        end repeat
    end repeat
    return "not_found"
end tell"#,
        tty_match_str
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
