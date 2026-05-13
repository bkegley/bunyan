---
name: bunyan-review
description: Inspect and reason about bunyan workspaces — list them, filter by status or lineage, read results and diffs, archive stale ones. Use when reviewing delegated work, debugging a side-task, or auditing what got spawned.
---

# Bunyan: observation and review

This skill is for the *reviewer* — a human catching up on delegated work,
or an agent specifically tasked with reviewing/auditing what bunyan
spawned. It does not delegate new work; for that, use `bunyan-delegate`.

## When to use this

- "What's stuck?" / "What did I delegate?" / "What finished while I was gone?"
- Reading the result of a delegated side-task.
- Inspecting the diff a delegated agent produced.
- Archiving stale or completed workspaces.

## Prerequisites

```bash
curl -s http://127.0.0.1:3333/health
```

Start the daemon with `bunyan up` if it isn't reachable.

## Listing and filtering

```bash
# Everything
curl -s http://127.0.0.1:3333/workspaces

# By repo
curl -s 'http://127.0.0.1:3333/workspaces?repo_id=<id>'

# By status
curl -s 'http://127.0.0.1:3333/workspaces?status=ready'
curl -s 'http://127.0.0.1:3333/workspaces?status=archived'

# Workspaces a specific parent delegated
curl -s 'http://127.0.0.1:3333/workspaces?delegated_by=<workspace-id>'

# New since some timestamp
curl -s 'http://127.0.0.1:3333/workspaces?since=2026-05-01T00:00:00Z'

# Combine filters freely
curl -s 'http://127.0.0.1:3333/workspaces?status=ready&delegated_by=<id>&since=...'
```

Each workspace row carries `parent_workspace_id` and `delegation_prompt`
when it was created via `POST /delegate`, so you can trace lineage.

## Reading results and diffs

```bash
# Structured outcome the spawned agent (or its Stop hook) wrote
curl -s http://127.0.0.1:3333/workspaces/<id>/result

# Git diff of the worktree vs the repo's default branch
curl -s http://127.0.0.1:3333/workspaces/<id>/diff
```

`GET /workspaces/:id/result` returns:
- `200` + JSON when `result.json` exists at the worktree root.
- `204 No Content` if the spawned agent hasn't written results yet.

## Process and session state

```bash
# What processes are running for this workspace
curl -s http://127.0.0.1:3333/workspaces/<id>/panes

# Past Claude sessions for this workspace
curl -s http://127.0.0.1:3333/workspaces/<id>/sessions

# All active Claude sessions across all workspaces
curl -s http://127.0.0.1:3333/sessions/active
```

## Lifecycle actions

```bash
# Surface a workspace in the user's configured terminal (fires
# workspace.ready_to_view hook)
curl -s -X POST http://127.0.0.1:3333/workspaces/<id>/view

# Resume an existing Claude session in this workspace
curl -s -X POST http://127.0.0.1:3333/workspaces/<id>/claude/resume \
  -H 'content-type: application/json' \
  -d '{"session_id":"<id>"}'

# Archive (tear down the worktree + processes; destructive)
curl -s -X POST http://127.0.0.1:3333/workspaces/<id>/archive
```

## Hook inspection (when something's not working)

```bash
# What hooks would run for an event in a given repo?
bunyan hooks list workspace.ready_to_view --repo myrepo

# Fire an event by hand against a workspace (great for debugging)
bunyan hooks run workspace.ready_to_view --workspace <id>
```

## A typical review session

1. `GET /workspaces?status=ready` — see what's live
2. Pick one; `GET /workspaces/:id` for details + lineage
3. `GET /workspaces/:id/result` for the structured outcome
4. `GET /workspaces/:id/diff` if you want the code review
5. Either resume Claude there for follow-up, or archive

## Guardrails

- Archive is destructive — it removes the worktree on disk.
- Killing a pane mid-session interrupts whatever the agent was doing.
- `result.json` is convention, not contract. If a spawned agent didn't
  write one, `GET .../result` returns 204; check the diff and transcript
  instead.
