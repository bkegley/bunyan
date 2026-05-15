# API Reference

Base URL: `http://127.0.0.1:3333` (or check `~/.bunyan/server.port`)

All request/response bodies are JSON. Errors return `{"error": "<message>"}`
with appropriate HTTP status.

The full OpenAPI spec is served at `GET /api-doc/openapi.json`.

## Delegation (the value-prop endpoint)

### POST /delegate
Atomic: create worktree → bootstrap (via `workspace.created` hooks) → spawn
Claude with the prompt and an injected `.claude/settings.local.json`.

Body:
```json
{
  "repo": "string (matches a Repo.name)",
  "branch": "string (new branch off default_branch)",
  "prompt": "string (the full task for the spawned Claude)",
  "from": "string? (parent workspace id, for lineage)",
  "directory_name": "string? (defaults to branch with / -> -)"
}
```

Returns `201 Created` with `{"workspace_id": "...", "observation_url": "..."}`.

See [`delegate.md`](delegate.md) for the parent-agent workflow.

## Events

### GET /events
Server-Sent Events stream of every bunyan lifecycle event. Each block is
`event: <name>\ndata: <json>\n\n`. Includes a 15-second keep-alive.

```bash
curl -N http://127.0.0.1:3333/events
```

Event names: `workspace.created`, `workspace.ready_to_view`,
`workspace.archived`, `claude.started`, `claude.resumed`, `claude.stopped`,
`claude.subagent_stopped`, `claude.notification`, `claude.session_started`.

## Health

### GET /health
Returns `{"status": "ok"}`.

## Repos

### GET /repos
List all repositories. Returns `Repo[]`.

### GET /repos/:id
Get a single repo. Returns `Repo`.

### POST /repos
Create a repository. Clones from `remote_url` to `root_path`.

Body:
```json
{
  "name": "string",
  "remote_url": "string",
  "root_path": "string",
  "default_branch": "string (default: main)",
  "remote": "string (default: origin)",
  "display_order": 0,
  "config": {}
}
```
Returns `Repo`.

### PUT /repos/:id
Update a repo. Only specified fields are changed.

Body: `{"name": "string?", "default_branch": "string?", "display_order": "number?", "config": "object?"}`. Returns `Repo`.

### DELETE /repos/:id
Delete a repo and cascade to its workspaces. Returns `null`.

## Workspaces

### GET /workspaces
List workspaces with optional filters. Returns `Workspace[]`.

Query params (any combination):
- `repo_id` — filter by repository
- `status` — `ready` or `archived`
- `delegated_by` — workspaces spawned by a specific parent workspace id
- `since` — ISO-8601 timestamp; only rows with `created_at >= since`

### GET /workspaces/:id
Get a workspace. Returns `Workspace`.

### POST /workspaces
Create a workspace (git worktree + optional container). Use this for the
*manual* flow. For agent delegation, use `POST /delegate`.

Body:
```json
{
  "repository_id": "string",
  "directory_name": "string",
  "branch": "string",
  "container_mode": "local | container (default: local)"
}
```
Returns `Workspace`.

### POST /workspaces/:id/archive
Archive a workspace. Fires `workspace.archived` hook, removes worktree,
kills panes, removes container. Returns `Workspace`.

### POST /workspaces/:id/view
Fire `workspace.ready_to_view` hook to surface the workspace in the user's
configured terminal. If no hook is configured, returns 200 and a log note —
the workspace is still up; bunyan just doesn't pop a window.

### GET /workspaces/:id/sessions
List Claude session history for this worktree (read from
`~/.claude/projects/...`). Returns `ClaudeSessionEntry[]`.

### GET /workspaces/:id/panes
List runtime backend's process slots for this workspace. Returns
`TmuxPane[]`. (Name kept for backwards-compat; works for tmux and zellij.)

### POST /workspaces/:id/claude
Start (or attach to existing) Claude session in the workspace. Returns
`{"status": "created" | "attached"}`.

### POST /workspaces/:id/claude/resume
Resume a specific Claude session.

Body: `{"session_id": "string"}`

Returns `{"status": "resumed" | "attached"}`.

### POST /workspaces/:id/shell
Open a shell pane in the workspace. Returns `{"status": "created"}`.

### DELETE /workspaces/:id/panes/:index
Kill a pane by index. Returns `{"status": "killed"}`.

### GET /workspaces/:id/diff
Git diff of the worktree vs the repo's `default_branch`. Plain text.
Empty if no changes.

### GET /workspaces/:id/result
Most recent Stop/SubagentStop payload bunyan captured from the spawned
Claude's injected hooks. Read from the workspace's `last_result` SQLite
column. Returns:
- `200` + JSON if a Stop turn has been captured
- `204 No Content` if the session hasn't stopped yet

### POST /workspaces/:id/agent-events
Ingress for spawned Claude's injected hooks. Body is Claude's verbatim
hook stdin payload. Bunyan parses `hook_event_name` and `session_id`,
persists session_id and (for Stop/SubagentStop) `last_result`, then
re-fires the event onto bunyan's bus. Returns `202 Accepted`. Always 202
— a hook crash should never block the spawned Claude.

## Sessions

### GET /sessions/active
All active Claude sessions across workspaces. Returns `WorkspacePaneInfo[]`.

## Docker

### GET /docker/status
Check Docker availability. Returns `{"available": boolean}`.

### GET /workspaces/:id/container/status
Container state. Returns `{"status": "running" | "exited" | "none"}`.

### GET /workspaces/:id/container/ports
Port mappings. Returns `PortMapping[]`.

## Editors

### GET /editors
Detect installed editors (VSCode, Cursor, Zed, Windsurf, Antigravity).
Returns `string[]` of editor IDs. Terminal/multiplexer attachment is no
longer an "editor" — that flows through the `workspace.ready_to_view`
hook (see [`hooks.md`](hooks.md)).

### POST /workspaces/:id/editor
Open the workspace directory in the specified editor.

Body: `{"editor_id": "string"}`

Returns `{"status": "opened"}`.

## Hooks (introspection / debugging)

### GET /hooks?event=<name>&repo=<name>
List which on-disk hook scripts would run for an event. Returns
`{"event": "...", "hooks": ["/path/to/script", ...]}`.

### POST /hooks/run
Fire an event by hand against the daemon. Useful for debugging hooks.

Body: `{"event": "string", "workspace_id": "string?", "extras": {"k": "v"}}`

Returns `{"event": "...", "outcomes": [{path, exit_code, duration_ms, stdout, stderr, timed_out, succeeded}, ...]}`.

## System

### GET /system/info
System metadata. Returns `{"home_dir": "string"}`.

## Settings

### GET /settings
All settings. Returns `Setting[]`.

### GET /settings/:key
Single setting. Returns `Setting`.

### PUT /settings/:key
Set a setting value.

Body: `{"value": "string"}`

Returns `Setting`.

## Types

```typescript
interface Repo {
  id: string;
  name: string;
  remote_url: string;
  default_branch: string;
  root_path: string;
  remote: string;
  display_order: number;
  config: object | null;
  created_at: string;
  updated_at: string;
}

interface Workspace {
  id: string;
  repository_id: string;
  directory_name: string;
  branch: string;
  state: "ready" | "archived";
  container_mode: "local" | "container";
  container_id: string | null;
  created_at: string;
  updated_at: string;
  // Set when this workspace was created via POST /delegate:
  parent_workspace_id: string | null;
  delegation_prompt: string | null;
  // Populated by the spawned Claude's injected hooks reporting back:
  claude_session_id: string | null;
  last_result: string | null; // JSON string; surfaced parsed via GET /workspaces/:id/result
}

interface TmuxPane {
  pane_index: number;
  command: string;
  is_active: boolean;
  workspace_path: string;
  pane_pid: number;
}

interface WorkspacePaneInfo {
  workspace_id: string;
  repo_name: string;
  workspace_name: string;
  panes: TmuxPane[];
}

interface ClaudeSessionEntry {
  session_id: string;
  first_prompt: string | null;
  message_count: number | null;
  created: string | null;
  modified: string | null;
  git_branch: string | null;
  is_sidechain: boolean | null;
}

interface PortMapping {
  container_port: string;
  host_port: string;
  host_ip: string;
}

interface Setting {
  key: string;
  value: string;
  created_at: string;
  updated_at: string;
}
```

## Error Codes

| Status | Meaning |
|---|---|
| 200 | Success |
| 201 | Created (POST /delegate, POST /repos) |
| 202 | Accepted (POST /workspaces/:id/agent-events) |
| 204 | No content (GET /workspaces/:id/result before Stop) |
| 400 | Bad request (invalid JSON, serialization error) |
| 404 | Resource not found |
| 500 | Internal error (git, docker, process, database) |
