# Session Workflows

## Overview

Bunyan manages Claude Code sessions within workspace tmux panes. You can start new sessions, resume existing ones, open shell panes, and manage pane lifecycle.

## Prerequisites

- Server running
- Workspace created and in `ready` state

## Start a Claude Session

Opens Claude in a new tmux pane for the workspace:

```bash
curl -s -X POST http://127.0.0.1:3333/workspaces/<ID>/claude
```

Returns `{"status": "created"}` for new sessions or `{"status": "attached"}` if Claude is already running.

Behavior:
- If Claude is already running in a pane, focuses that pane instead
- If previous sessions exist, uses `claude --continue` automatically
- For container workspaces, runs Claude inside the container

## Resume a Specific Session

```bash
curl -s -X POST http://127.0.0.1:3333/workspaces/<ID>/claude/resume \
  -H 'Content-Type: application/json' \
  -d '{"session_id": "<SESSION_ID>"}'
```

Returns `{"status": "resumed"}` or `{"status": "attached"}` if already running.

Session IDs must be alphanumeric with dashes/underscores only.

## Get Session History

```bash
curl -s http://127.0.0.1:3333/workspaces/<ID>/sessions
```

Returns array of session entries with `session_id`, `first_prompt`, `message_count`, `created`, `modified`, `git_branch`.

## Open a Shell Pane

Opens a new shell pane (split) in the workspace tmux window:

```bash
curl -s -X POST http://127.0.0.1:3333/workspaces/<ID>/shell
```

For container workspaces, opens a bash shell inside the container.

## List Active Sessions

Get all active Claude sessions across all workspaces:

```bash
curl -s http://127.0.0.1:3333/sessions/active
```

Returns array of `WorkspacePaneInfo` with `workspace_id`, `repo_name`, `workspace_name`, and `panes` array.

## List Panes

```bash
curl -s http://127.0.0.1:3333/workspaces/<ID>/panes
```

Returns array of panes with `pane_index`, `command`, `is_active`, `workspace_path`, `pane_pid`.

## Kill a Pane

```bash
curl -s -X DELETE http://127.0.0.1:3333/workspaces/<ID>/panes/<INDEX>
```

## Error Handling

| Error | Cause | Fix |
|---|---|---|
| 404 | Workspace not found | Check workspace ID |
| 500 "Invalid session ID" | Bad characters in session_id | Use only alphanumeric, dash, underscore |
| 500 tmux error | tmux server not running | Workspace view/claude will create it |
