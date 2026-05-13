//! Workspace view dispatch.
//!
//! Bunyan no longer ships an opinion about which terminal/multiplexer the user
//! wants. The `workspace.ready_to_view` event is the only mechanism; users
//! drop a hook in `~/.config/bunyan/hooks/workspace.ready_to_view` (see
//! `examples/hooks/`) to control what happens when a workspace becomes
//! viewable.
//!
//! If no hook is configured, bunyan logs a helpful message and returns Ok.
//! The workspace is still set up, processes are still running — just nothing
//! pops open a terminal.

use std::path::PathBuf;

use crate::error::Result;
use crate::events::names;
use crate::hooks::{self, DefaultHookRoots, HookContext, HookRoots, HookRunResult};
use crate::tmux;

/// What the hook layer decided we should do next.
#[derive(Debug, PartialEq)]
pub enum ViewDispatch {
    /// At least one hook succeeded — the workspace view is already opened.
    HandledByHook,
    /// No hook ran for this event. The workspace is ready but nothing
    /// surfaced it visually.
    NoHookConfigured,
    /// One or more hooks ran but all failed.
    AllHooksFailed,
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
    let mut ctx = HookContext::new(names::WORKSPACE_READY_TO_VIEW);
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
fn dispatch_from_result(result: &HookRunResult) -> ViewDispatch {
    if result.any_succeeded() {
        ViewDispatch::HandledByHook
    } else if result.any_ran() {
        ViewDispatch::AllHooksFailed
    } else {
        ViewDispatch::NoHookConfigured
    }
}

/// Fire `workspace.ready_to_view` hooks against the given roots and return
/// the dispatch decision. Exposed for tests; callers use `open_workspace_view`.
#[allow(clippy::too_many_arguments)]
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

/// Fire the user's `workspace.ready_to_view` hook. If no hook is configured,
/// log a helpful note and return Ok — the workspace is up regardless.
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
        ViewDispatch::HandledByHook => {}
        ViewDispatch::NoHookConfigured => {
            let p = workspace_path.unwrap_or("(path unknown)");
            eprintln!(
                "[bunyan] no workspace.ready_to_view hook configured; workspace is ready at {} \
(see examples/hooks/ in the bunyan repo for templates)",
                p
            );
        }
        ViewDispatch::AllHooksFailed => {
            eprintln!(
                "[bunyan] workspace.ready_to_view hook(s) all failed — see logs above"
            );
        }
    }
    Ok(())
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
    fn ready_to_view_no_hook_configured_when_none_present() {
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
        assert_eq!(dispatch, ViewDispatch::NoHookConfigured);
    }

    #[test]
    fn ready_to_view_reports_all_hooks_failed() {
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
        assert_eq!(dispatch, ViewDispatch::AllHooksFailed);
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

    #[test]
    fn dispatch_from_no_outcomes_is_no_hook_configured() {
        let result = HookRunResult::default();
        assert_eq!(dispatch_from_result(&result), ViewDispatch::NoHookConfigured);
    }
}
