---
name: bunyan
description: Drive the Bunyan daemon — fire-and-forget agent delegation, workspace/worktree management, Claude session orchestration, container ops. Route to the right reference below for what you're trying to do.
---

# Bunyan

Bunyan is the substrate for **fire-and-forget agent delegation.** A parent
agent calls `POST /delegate` to hand a side-task to a fresh Claude in an
isolated worktree, gets back a URL, and forgets about it. Reviewers (humans
or other agents) come back later to inspect what got spawned.

Bunyan also predates that value-prop — it still manages worktrees, Claude
sessions, tmux/zellij panes, Docker containers, and editor launches. This
skill covers all of it.

## Where to look

**Read the section that matches what you're about to do. Don't read others.**

- **Delegating a side-task to a fresh Claude** → [`reference/delegate.md`](reference/delegate.md)
  One endpoint, one call, then forget. The parent-agent flow.

- **Reviewing what got delegated** → [`reference/review.md`](reference/review.md)
  Lists, filters, lineage, diffs, results, the SSE event stream, lifecycle
  actions. The reviewer flow.

- **Manual worktree workflows** → [`reference/worktree-workflows.md`](reference/worktree-workflows.md)
  Creating workspaces yourself instead of via delegation.

- **Existing Claude session ops** (start / resume / shell / kill pane in an
  already-created workspace) → [`reference/session-workflows.md`](reference/session-workflows.md)

- **Container-mode workspaces** (Docker) → [`reference/container-workflows.md`](reference/container-workflows.md)

- **Registering new repos** → [`reference/project-workflows.md`](reference/project-workflows.md)

- **Full endpoint catalog** → [`reference/api-reference.md`](reference/api-reference.md)
  When you need the exact path / payload / response shape.

- **Hooks — react to bunyan events from disk** → [`reference/hooks.md`](reference/hooks.md)
  Drop scripts in `~/.config/bunyan/hooks/<event>` to react to workspace
  and Claude lifecycle. The on-disk extension point.

## Before any operation

The daemon must be running:

```bash
curl -s http://127.0.0.1:3333/health
```

If unreachable: `bunyan up` to spawn it, `bunyan down` to stop it,
`bunyan serve` to run in the foreground.

## Installation (if `bunyan` is not on PATH)

```bash
curl -fsSL https://raw.githubusercontent.com/bkegley/bunyan/main/install.sh | bash
```

Drops the binary at `$HOME/.local/bin/bunyan`. Other platforms: `cargo
install --git https://github.com/bkegley/bunyan bunyan-cli`.
