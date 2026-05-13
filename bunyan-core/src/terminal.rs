use std::path::PathBuf;
use std::process::Command;

use crate::error::{BunyanError, Result};
use crate::hooks::{self, DefaultHookRoots, HookContext, HookRoots, HookRunResult};
use crate::tmux;

/// What the hook layer decided we should do next.
#[derive(Debug, PartialEq)]
pub enum ViewDispatch {
    /// At least one hook succeeded — the workspace view is already opened.
    HandledByHook,
    /// No hook ran, or every hook that ran failed. Fall back to the legacy
    /// iTerm flow. `had_failures` is true if any hook ran but errored.
    FallBackToIterm { had_failures: bool },
}

fn build_ready_to_view_context(
    repo_name: &str,
    workspace_name: &str,
    workspace_path: Option<&str>,
    workspace_id: Option<&str>,
    repo_id: Option<&str>,
    branch: Option<&str>,
    repo_root_path: Option<&str>,
) -> HookContext {
    let mut ctx = HookContext::new("workspace.ready_to_view");
    ctx.repo_name = Some(repo_name.to_string());
    if let Some(id) = repo_id {
        ctx.repo_id = Some(id.to_string());
    }
    ctx.workspace_name = Some(workspace_name.to_string());
    if let Some(id) = workspace_id {
        ctx.workspace_id = Some(id.to_string());
    }
    if let Some(p) = workspace_path {
        ctx.workspace_path = Some(p.to_string());
    }
    if let Some(b) = branch {
        ctx.branch = Some(b.to_string());
    }
    if let Some(p) = repo_root_path {
        ctx.repo_root_path = Some(p.to_string());
    }
    // Expose the tmux attach command as an event extra so hooks can be
    // backend-agnostic. Today the runtime is always tmux; the v2 refactor
    // will replace this with whatever the active backend reports.
    ctx.extras
        .insert("attach_cmd".into(), tmux::attach_command(repo_name));
    ctx
}

/// Decide what to do after firing the `workspace.ready_to_view` hook chain.
/// Pulled out for testability — the production path wraps this with
/// `DefaultHookRoots` and the iTerm fallback.
fn dispatch_from_result(result: &HookRunResult) -> ViewDispatch {
    if result.any_succeeded() {
        ViewDispatch::HandledByHook
    } else {
        ViewDispatch::FallBackToIterm {
            had_failures: result.any_ran(),
        }
    }
}

/// Fire `workspace.ready_to_view` hooks against the given roots and return
/// the dispatch decision. Exposed for tests; callers use `open_workspace_view`.
pub fn fire_ready_to_view(
    roots: &dyn HookRoots,
    repo_name: &str,
    workspace_name: &str,
    workspace_path: Option<&str>,
    workspace_id: Option<&str>,
    repo_id: Option<&str>,
    branch: Option<&str>,
    repo_root_path: Option<&str>,
) -> ViewDispatch {
    let ctx = build_ready_to_view_context(
        repo_name,
        workspace_name,
        workspace_path,
        workspace_id,
        repo_id,
        branch,
        repo_root_path,
    );
    let result = hooks::fire(roots, &ctx);
    dispatch_from_result(&result)
}

/// Try the user's `workspace.ready_to_view` hook first; fall back to the
/// hardcoded iTerm flow only if no hook ran successfully.
///
/// `repo_root_path` is the absolute path of the repo's clone (used to discover
/// per-repo hooks). Pass `None` if not available.
pub fn open_workspace_view(
    repo_name: &str,
    workspace_name: &str,
    workspace_path: Option<&str>,
    workspace_id: Option<&str>,
    repo_id: Option<&str>,
    branch: Option<&str>,
    repo_root_path: Option<&str>,
) -> Result<()> {
    let roots = DefaultHookRoots::new(repo_root_path.map(PathBuf::from));
    let dispatch = fire_ready_to_view(
        &roots,
        repo_name,
        workspace_name,
        workspace_path,
        workspace_id,
        repo_id,
        branch,
        repo_root_path,
    );
    match dispatch {
        ViewDispatch::HandledByHook => Ok(()),
        ViewDispatch::FallBackToIterm { had_failures } => {
            if had_failures {
                eprintln!(
                    "[bunyan] workspace.ready_to_view hook(s) failed; falling back to iTerm"
                );
            }
            attach_iterm(repo_name, workspace_name)
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    struct StaticRoots {
        user: Option<PathBuf>,
    }
    impl HookRoots for StaticRoots {
        fn user_root(&self) -> Option<PathBuf> {
            self.user.clone()
        }
        fn repo_root(&self, _: &str) -> Option<PathBuf> {
            None
        }
    }

    fn unique_tempdir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "bunyan-terminal-test-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_hook(path: &Path, script: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, script).unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn ready_to_view_handled_by_hook_when_hook_succeeds() {
        let tmp = unique_tempdir("rv_success");
        write_hook(&tmp.join("workspace.ready_to_view"), "#!/bin/sh\nexit 0\n");
        let roots = StaticRoots { user: Some(tmp) };
        let dispatch = fire_ready_to_view(
            &roots,
            "frontend",
            "ws-1",
            Some("/tmp/x"),
            Some("ws-id"),
            Some("repo-id"),
            Some("main"),
            None,
        );
        assert_eq!(dispatch, ViewDispatch::HandledByHook);
    }

    #[test]
    fn ready_to_view_falls_back_when_no_hook_present() {
        let tmp = unique_tempdir("rv_none");
        let roots = StaticRoots { user: Some(tmp) };
        let dispatch = fire_ready_to_view(
            &roots,
            "frontend",
            "ws-1",
            Some("/tmp/x"),
            Some("ws-id"),
            Some("repo-id"),
            Some("main"),
            None,
        );
        assert_eq!(
            dispatch,
            ViewDispatch::FallBackToIterm { had_failures: false }
        );
    }

    #[test]
    fn ready_to_view_falls_back_when_hook_fails() {
        let tmp = unique_tempdir("rv_fail");
        write_hook(&tmp.join("workspace.ready_to_view"), "#!/bin/sh\nexit 5\n");
        let roots = StaticRoots { user: Some(tmp) };
        let dispatch = fire_ready_to_view(
            &roots,
            "frontend",
            "ws-1",
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            dispatch,
            ViewDispatch::FallBackToIterm { had_failures: true }
        );
    }

    #[test]
    fn ready_to_view_short_circuit_counts_as_success() {
        let tmp = unique_tempdir("rv_short");
        write_hook(&tmp.join("workspace.ready_to_view"), "#!/bin/sh\nexit 78\n");
        let roots = StaticRoots { user: Some(tmp) };
        let dispatch = fire_ready_to_view(
            &roots,
            "frontend",
            "ws-1",
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(dispatch, ViewDispatch::HandledByHook);
    }

    #[test]
    fn ready_to_view_hook_receives_attach_cmd_extra() {
        let tmp = unique_tempdir("rv_attach");
        let marker = tmp.join("marker");
        let script = format!(
            "#!/bin/sh\necho \"$BUNYAN_ATTACH_CMD\" > {}\n",
            marker.display()
        );
        write_hook(&tmp.join("workspace.ready_to_view"), &script);
        let roots = StaticRoots {
            user: Some(tmp.clone()),
        };
        let dispatch = fire_ready_to_view(
            &roots,
            "myrepo",
            "ws-1",
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(dispatch, ViewDispatch::HandledByHook);
        let content = fs::read_to_string(&marker).unwrap();
        // tmux::attach_command builds: "tmux -L bunyan attach -t <repo>"
        assert!(
            content.contains("tmux") && content.contains("myrepo"),
            "expected attach_cmd to contain tmux + repo name, got {:?}",
            content
        );
    }
}
