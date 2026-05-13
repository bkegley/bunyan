//! Helpers for firing bunyan lifecycle events through the hooks system.
//!
//! Routes call into these helpers to publish events at well-defined moments
//! in a workspace's lifecycle. The hooks module does the actual discovery
//! and execution; this layer just builds the right `HookContext`.

use std::path::PathBuf;

use crate::event_bus::{self, EventBus};
use crate::hooks::{self, DefaultHookRoots, HookContext, HookRoots, HookRunResult};
use crate::models::{Repo, Workspace};

/// All lifecycle event names bunyan emits today. New events must be added
/// here AND documented in the user-facing hooks reference.
pub mod names {
    pub const WORKSPACE_CREATED: &str = "workspace.created";
    pub const WORKSPACE_ARCHIVED: &str = "workspace.archived";
    pub const WORKSPACE_READY_TO_VIEW: &str = "workspace.ready_to_view";
    pub const CLAUDE_STARTED: &str = "claude.started";
    pub const CLAUDE_RESUMED: &str = "claude.resumed";
}

/// Build a `HookContext` populated from a `Workspace` + `Repo` pair.
pub fn context_for(event: &str, ws: &Workspace, repo: &Repo, ws_path: &str) -> HookContext {
    HookContext::new(event)
        .with_repo(&repo.name, &repo.id)
        .with_workspace(&ws.directory_name, &ws.id, ws_path)
        .with_branch(&ws.branch)
        .with_repo_root(&repo.root_path)
}

/// Discover hook roots using the repo's on-disk root path.
pub fn roots_for(repo: &Repo) -> DefaultHookRoots {
    DefaultHookRoots::new(Some(PathBuf::from(&repo.root_path)))
}

/// Fire an event for a workspace using the default hook roots.
/// Intended to be called from `tokio::task::spawn_blocking`; the hook
/// executor is synchronous.
pub fn fire_workspace_event(
    event: &str,
    ws: &Workspace,
    repo: &Repo,
    ws_path: &str,
) -> HookRunResult {
    let roots = roots_for(repo);
    let ctx = context_for(event, ws, repo, ws_path);
    hooks::fire(&roots, &ctx)
}

/// Fire an event for a workspace with extra key/value pairs added to the
/// hook context (exposed as `BUNYAN_<UPPER>` env vars and `extras.*` in JSON).
pub fn fire_workspace_event_with_extras(
    event: &str,
    ws: &Workspace,
    repo: &Repo,
    ws_path: &str,
    extras: &[(&str, &str)],
) -> HookRunResult {
    let roots = roots_for(repo);
    let mut ctx = context_for(event, ws, repo, ws_path);
    for (k, v) in extras {
        ctx = ctx.with_extra(*k, *v);
    }
    hooks::fire(&roots, &ctx)
}

/// Fire any event against arbitrary roots. Used by `bunyan hooks run`.
pub fn fire_against_roots(roots: &dyn HookRoots, ctx: &HookContext) -> HookRunResult {
    hooks::fire(roots, ctx)
}

/// Publish a context's envelope onto the event bus. SSE subscribers see it
/// in real time. Routes that want both on-disk hooks and SSE should call
/// this *and* `fire_workspace_event`.
pub fn publish_to_bus(bus: &EventBus, ctx: &HookContext) {
    bus.publish(event_bus::envelope_from_context(ctx));
}

/// Convenience: fire on-disk hooks AND publish to the bus in one go.
pub fn fire_and_publish(
    bus: &EventBus,
    event: &str,
    ws: &Workspace,
    repo: &Repo,
    ws_path: &str,
) -> HookRunResult {
    let ctx = context_for(event, ws, repo, ws_path);
    publish_to_bus(bus, &ctx);
    let roots = roots_for(repo);
    hooks::fire(&roots, &ctx)
}

/// Like `fire_and_publish` but with extras.
pub fn fire_and_publish_with_extras(
    bus: &EventBus,
    event: &str,
    ws: &Workspace,
    repo: &Repo,
    ws_path: &str,
    extras: &[(&str, &str)],
) -> HookRunResult {
    let mut ctx = context_for(event, ws, repo, ws_path);
    for (k, v) in extras {
        ctx = ctx.with_extra(*k, *v);
    }
    publish_to_bus(bus, &ctx);
    let roots = roots_for(repo);
    hooks::fire(&roots, &ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ContainerMode, WorkspaceState};

    fn make_ws() -> Workspace {
        Workspace {
            id: "ws-id".to_string(),
            repository_id: "r-id".to_string(),
            directory_name: "ws-name".to_string(),
            branch: "fix".to_string(),
            state: WorkspaceState::Ready,
            container_mode: ContainerMode::Local,
            container_id: None,
            created_at: "t".to_string(),
            updated_at: "t".to_string(),
            parent_workspace_id: None,
            delegation_prompt: None,
        }
    }

    fn make_repo() -> Repo {
        Repo {
            id: "r-id".to_string(),
            name: "frontend".to_string(),
            remote_url: "u".to_string(),
            default_branch: "main".to_string(),
            root_path: "/tmp/frontend".to_string(),
            remote: "origin".to_string(),
            display_order: 0,
            config: None,
            created_at: "t".to_string(),
            updated_at: "t".to_string(),
        }
    }

    #[test]
    fn context_for_populates_all_known_fields() {
        let ws = make_ws();
        let repo = make_repo();
        let ctx = context_for("workspace.created", &ws, &repo, "/tmp/frontend/ws-name");
        assert_eq!(ctx.event, "workspace.created");
        assert_eq!(ctx.repo_name.as_deref(), Some("frontend"));
        assert_eq!(ctx.repo_id.as_deref(), Some("r-id"));
        assert_eq!(ctx.workspace_name.as_deref(), Some("ws-name"));
        assert_eq!(ctx.workspace_id.as_deref(), Some("ws-id"));
        assert_eq!(ctx.workspace_path.as_deref(), Some("/tmp/frontend/ws-name"));
        assert_eq!(ctx.branch.as_deref(), Some("fix"));
        assert_eq!(ctx.repo_root_path.as_deref(), Some("/tmp/frontend"));
    }
}
