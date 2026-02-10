---
name: bunyan
description: Manage git worktrees, Claude sessions, and Docker containers via the Bunyan workspace manager. Use for side-fixes, worktree creation, session management, and container workflows.
---

# Bunyan Workspace Manager

Bunyan manages git worktrees, Claude Code sessions, tmux panes, and Docker containers. Use it to spin up isolated workspaces for side-fixes, manage multiple Claude sessions, and orchestrate container-based development.

## Prerequisites

Before any operation, verify the server is running:

```bash
curl -s http://127.0.0.1:3333/health
```

If unreachable, start it:
```bash
bunyan serve &
```

Or check `~/.bunyan/server.port` for the actual port.

## Quick Reference

| Action | Method | Endpoint |
|---|---|---|
| List repos | GET | `/repos` |
| Get repo | GET | `/repos/:id` |
| Create repo | POST | `/repos` |
| List workspaces | GET | `/workspaces?repo_id=` |
| Get workspace | GET | `/workspaces/:id` |
| Create workspace | POST | `/workspaces` |
| Archive workspace | POST | `/workspaces/:id/archive` |
| Start Claude | POST | `/workspaces/:id/claude` |
| Resume Claude | POST | `/workspaces/:id/claude/resume` |
| Open shell | POST | `/workspaces/:id/shell` |
| View workspace | POST | `/workspaces/:id/view` |
| List panes | GET | `/workspaces/:id/panes` |
| Kill pane | DELETE | `/workspaces/:id/panes/:index` |
| Active sessions | GET | `/sessions/active` |
| Docker status | GET | `/docker/status` |
| Container status | GET | `/workspaces/:id/container/status` |
| Container ports | GET | `/workspaces/:id/container/ports` |
| List settings | GET | `/settings` |
| Get setting | GET | `/settings/:key` |
| Set setting | PUT | `/settings/:key` |

## Routing

- **Creating worktrees / side-fixes**: See `reference/worktree-workflows.md`
- **Claude session management**: See `reference/session-workflows.md`
- **Repository setup**: See `reference/project-workflows.md`
- **Docker / container ops**: See `reference/container-workflows.md`
- **Full API details**: See `reference/api-reference.md`

## Common Workflow: Side-Fix

1. Find the repo: `GET /repos`
2. Create worktree: `POST /workspaces` with `repository_id`, `directory_name`, `branch`
3. Work in the worktree directory
4. Archive when done: `POST /workspaces/:id/archive`

## Guardrails

- Always check health before operations
- Use `directory_name` as a short identifier (no spaces, slashes)
- Branch names must be valid git branch names
- Archive cleans up the worktree and container (if any) — this is destructive
- Container mode requires Docker to be running (`GET /docker/status`)
- Session IDs must be alphanumeric with dashes/underscores only
