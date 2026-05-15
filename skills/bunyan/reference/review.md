# Reviewing delegated work

This page is for the *reviewer* — a human catching up on delegated work,
or an agent specifically tasked with reviewing/auditing what bunyan
spawned. It does not delegate new work; for that, see
[`delegate.md`](delegate.md).

## When to use this

- "What's stuck?" / "What did I delegate?" / "What finished while I was gone?"
- Reading the result of a delegated side-task.
- Inspecting the diff a delegated agent produced.
- Resuming a delegated Claude session for follow-up.
- Archiving stale or completed workspaces.

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
when it was created via `POST /delegate`, so you can trace lineage. The
row also carries `claude_session_id` once the spawned agent reports back
— that's what `claude --resume` takes.

## Reading results and diffs

```bash
# Most recent Stop/SubagentStop payload bunyan captured from the agent
curl -s http://127.0.0.1:3333/workspaces/<id>/result

# Git diff of the worktree vs the repo's default branch
curl -s http://127.0.0.1:3333/workspaces/<id>/diff
```

`GET /workspaces/:id/result` returns:
- `200` + JSON when bunyan has captured a Stop turn.
- `204 No Content` if the session hasn't stopped yet.

Bunyan auto-populates this via the injected `.claude/settings.local.json`
hooks. Reads come from a SQLite column (`last_result`), not from a file
in the worktree — git stays clean.

## Resuming a delegated session

```bash
# Find the session id (it's on the workspace row, populated automatically)
curl -s http://127.0.0.1:3333/workspaces/<id> | jq -r .claude_session_id

# Resume via bunyan (re-attaches the existing pane, or spawns a new pane
# running `claude --resume <id>`)
curl -s -X POST http://127.0.0.1:3333/workspaces/<id>/claude/resume \
  -H 'content-type: application/json' \
  -d '{"session_id":"<that-id>"}'
```

Or, if you want to resume from your own terminal outside bunyan:
`claude --resume <id>` directly.

## Process and session state

```bash
# What processes are running for this workspace
curl -s http://127.0.0.1:3333/workspaces/<id>/panes

# Past Claude sessions for this workspace (read from ~/.claude/projects)
curl -s http://127.0.0.1:3333/workspaces/<id>/sessions

# All active Claude sessions across all workspaces
curl -s http://127.0.0.1:3333/sessions/active
```

## Lifecycle actions

```bash
# Surface a workspace in the user's configured terminal (fires
# workspace.ready_to_view hook)
curl -s -X POST http://127.0.0.1:3333/workspaces/<id>/view

# Open the worktree in an editor (vscode/cursor/zed/windsurf/antigravity)
curl -s -X POST http://127.0.0.1:3333/workspaces/<id>/editor \
  -H 'content-type: application/json' \
  -d '{"editor_id":"zed"}'

# Archive (tear down the worktree + processes; destructive)
curl -s -X POST http://127.0.0.1:3333/workspaces/<id>/archive
```

## Live event stream

```bash
# Tail every bunyan lifecycle event in real time (workspace.*, claude.*)
curl -N http://127.0.0.1:3333/events
```

Server-Sent Events: each block is `event: <name>\ndata: <json>\n\n`.
Useful for tailing what's happening while you wait, or piping into jq for
filtering. The on-disk hook executor (see [`hooks.md`](hooks.md)) is the
other consumer of the same stream.

## Hook inspection (when something's not working)

```bash
# What hooks would run for an event in a given repo?
bunyan hooks list workspace.ready_to_view --repo myrepo

# Fire an event by hand against a workspace (great for debugging)
bunyan hooks run workspace.ready_to_view --workspace <id>
```

## A typical review session

1. `GET /workspaces?status=ready` — see what's live
2. Pick one; `GET /workspaces/:id` for details + lineage + session id
3. `GET /workspaces/:id/result` for the structured outcome
4. `GET /workspaces/:id/diff` if you want the code review
5. Either resume Claude there for follow-up (`claude/resume`), or archive

## Guardrails

- Archive is destructive — it removes the worktree on disk.
- Killing a pane mid-session interrupts whatever the agent was doing.
- `/result` returns the Stop hook's view. If a richer human-readable
  summary exists, look in the worktree's git diff for files the agent
  wrote.
