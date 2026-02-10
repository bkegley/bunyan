# API Reference

Base URL: `http://127.0.0.1:3333` (or check `~/.bunyan/server.port`)

All request/response bodies are JSON. Errors return `{"error": "<message>"}` with appropriate HTTP status.

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

Body:
```json
{
  "name": "string?",
  "default_branch": "string?",
  "display_order": "number?",
  "config": "object?"
}
```
Returns `Repo`.

### DELETE /repos/:id
Delete a repo and cascade to all its workspaces. Returns `null`.

## Workspaces

### GET /workspaces
List workspaces. Optional query param `repo_id` to filter. Returns `Workspace[]`.

### GET /workspaces/:id
Get a workspace. Returns `Workspace`.

### POST /workspaces
Create a workspace (git worktree + optional container).

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
Archive a workspace. Removes worktree, kills panes, removes container. Returns `Workspace`.

### POST /workspaces/:id/view
Focus workspace in iTerm. Returns `{"status": "attached"}`.

### GET /workspaces/:id/sessions
Get Claude session history. Returns `ClaudeSessionEntry[]`.

### GET /workspaces/:id/panes
List tmux panes. Returns `TmuxPane[]`.

### POST /workspaces/:id/claude
Start or attach to Claude session. Returns `{"status": "created" | "attached"}`.

### POST /workspaces/:id/claude/resume
Resume a specific session.

Body: `{"session_id": "string"}`

Returns `{"status": "resumed" | "attached"}`.

### POST /workspaces/:id/shell
Open a shell pane. Returns `{"status": "created"}`.

### DELETE /workspaces/:id/panes/:index
Kill a pane by index. Returns `{"status": "killed"}`.

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
| 400 | Bad request (invalid JSON, serialization error) |
| 404 | Resource not found |
| 500 | Internal error (git, docker, process, database) |
