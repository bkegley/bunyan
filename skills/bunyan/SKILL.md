---
name: bunyan
description: Drive the Bunyan daemon — an HTTP API for managing git worktrees, running Claude sessions, delegating side-tasks to fresh agents, and reacting to lifecycle events. Use for any workspace, session, container, or delegation operation. Route to the right reference page below based on the task.
---

# Bunyan

Bunyan is a local HTTP daemon (default `http://127.0.0.1:3333`) that
orchestrates the moving parts of a multi-workspace development setup:

- **Git worktrees** — each "workspace" is a worktree on its own branch,
  living under `~/bunyan/workspaces/<repo>/<dir>`. Bunyan creates, lists,
  and tears them down.
- **Claude sessions** — bunyan supervises long-lived Claude processes
  inside the worktree via a pluggable runtime backend (tmux or zellij).
  Sessions persist across terminal restarts.
- **Fire-and-forget delegation** — `POST /delegate` hands a side-task to a
  fresh Claude in a new worktree atomically (worktree + bootstrap +
  spawn + hook injection). The caller gets back a URL and moves on.
- **Lifecycle events + hooks** — every state change publishes a
  bunyan event. Scripts on disk (`~/.config/bunyan/hooks/<event>`) and
  HTTP subscribers (`GET /events` SSE) both receive them.
- **Observation surface** — read `result`, `diff`, `panes`, `sessions`
  per-workspace; filter the workspace list by status, lineage, or
  age; tail live events.
- **Container mode** — workspaces can run inside per-workspace Docker
  containers with port forwarding.
- **Editor launch** — open any workspace in VS Code / Cursor / Zed /
  Windsurf / Antigravity with one POST.

## Where to look

Read the reference page that matches the task. **Don't read others** —
each page is self-contained.

| If you're about to… | Read |
| --- | --- |
| Hand a side-task to a fresh Claude (fire-and-forget) | [`reference/delegate.md`](reference/delegate.md) |
| Inspect / review delegated work (list, filter, diff, result, resume) | [`reference/review.md`](reference/review.md) |
| Manually create a workspace you'll drive yourself | [`reference/worktree-workflows.md`](reference/worktree-workflows.md) |
| Manage Claude sessions in an existing workspace (start, resume, shell, kill pane) | [`reference/session-workflows.md`](reference/session-workflows.md) |
| Set up container-mode workspaces (Docker) | [`reference/container-workflows.md`](reference/container-workflows.md) |
| Register a new repository | [`reference/project-workflows.md`](reference/project-workflows.md) |
| Subscribe to lifecycle events from a script or run code on workspace creation, claude stop, etc. | [`reference/hooks.md`](reference/hooks.md) |
| Look up an exact endpoint, payload, or response shape | [`reference/api-reference.md`](reference/api-reference.md) |

## Prerequisites

The daemon must be reachable before any operation:

```bash
curl -s http://127.0.0.1:3333/health
```

If unreachable:

```bash
bunyan up      # spawn the daemon in the background
bunyan down    # stop it
bunyan serve   # run in the foreground
```

The CLI auto-discovers the port from `~/.bunyan/server.port`; pass
`--port <N>` to override.

## Installation

If `bunyan` is not on the user's PATH (`command -v bunyan` fails):

```bash
curl -fsSL https://raw.githubusercontent.com/bkegley/bunyan/main/install.sh | bash
```

Drops the binary at `$HOME/.local/bin/bunyan`. Other platforms:
`cargo install --git https://github.com/bkegley/bunyan bunyan-cli`.

## Universal guardrails

- Always health-check before operations.
- `directory_name` is a short identifier — no spaces or slashes.
- Branch names must be valid git branch names.
- Archive (`POST /workspaces/:id/archive`) removes the worktree on disk
  and tears down its processes. Destructive.
- Container mode requires Docker to be running
  (`GET /docker/status`).
