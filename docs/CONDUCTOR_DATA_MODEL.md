# Conductor Data Model

## Storage Locations

All persistent state lives in two places:

1. **SQLite Database**: `~/Library/Application Support/com.conductor.app/conductor.db`
2. **Filesystem**: `~/conductor/` (repos, worktrees, archived contexts)

Supporting files in the app data directory:

- `bin/` — bundled binaries (claude, codex, gh, node, watchexec, checkpointer.sh, etc.)
- `.window-state.json` — UI window geometry (x, y, width, height, maximized, fullscreen)

---

## Database Schema

### `repos`

The central configuration entity. Each row represents a git repository managed by Conductor.

| Column | Type | Default | Description |
|--------|------|---------|-------------|
| `id` | TEXT (UUID) | PK | Unique identifier |
| `name` | TEXT | | Human-readable repo name (e.g. "frontend") |
| `remote_url` | TEXT | | Git remote URL (e.g. `git@gitlab.com:org/repo.git`) |
| `default_branch` | TEXT | `'main'` | Default branch name |
| `root_path` | TEXT | | Absolute path to the bare clone on disk |
| `remote` | TEXT | | Git remote name (e.g. `origin`) |
| `display_order` | INTEGER | `0` | UI sort order |
| `storage_version` | INTEGER | `1` | Internal versioning |
| `setup_script` | TEXT | | **Legacy** — unused, superseded by `conductor_config` |
| `run_script` | TEXT | | **Legacy** — unused, superseded by `conductor_config` |
| `run_script_mode` | TEXT | `'concurrent'` | **Legacy** — superseded by `conductor_config` |
| `archive_script` | TEXT | | **Legacy** — unused, superseded by `conductor_config` |
| `conductor_config` | TEXT (JSON) | | **Active config** — scripts and settings as JSON |
| `custom_prompt_code_review` | TEXT | | Custom prompt for AI code review |
| `custom_prompt_create_pr` | TEXT | | Custom prompt for AI PR creation |
| `custom_prompt_rename_branch` | TEXT | | Custom prompt for AI branch renaming |
| `custom_prompt_general` | TEXT | | Custom general-purpose AI prompt |
| `created_at` | TEXT | `datetime('now')` | Row creation timestamp |
| `updated_at` | TEXT | `datetime('now')` | Auto-updated via trigger |

#### `conductor_config` JSON Structure

This is the authoritative source for repo-level scripts and configuration. The legacy `setup_script`, `run_script`, and `archive_script` columns exist but are empty in practice.

```json
{
  "scripts": {
    "setup": "mise trust && make setup",
    "run": "cd frontend && npm run start"
  },
  "runScriptMode": "concurrent"
}
```

| Key | Description |
|-----|-------------|
| `scripts.setup` | Shell command run when initializing a new worktree. Installs deps, configures tooling, etc. |
| `scripts.run` | Shell command to start the dev server / main process for the worktree |
| `scripts.archive` | Shell command run when archiving a worktree (not observed in current data but structurally supported) |
| `runScriptMode` | How the run script executes: `"concurrent"` (run alongside AI session) |

---

### `workspaces`

Each row is a git worktree created from a repo. The worktree directory name is a random city name.

| Column | Type | Default | Description |
|--------|------|---------|-------------|
| `id` | TEXT (UUID) | PK | Unique identifier |
| `repository_id` | TEXT (UUID) | | FK to `repos.id` |
| `directory_name` | TEXT | | City name used as the worktree folder name (e.g. "lisbon") |
| `DEPRECATED_city_name` | TEXT | | Old column, superseded by `directory_name` |
| `branch` | TEXT | | Git branch name (e.g. `bkegley/lisbon`) |
| `state` | TEXT | `'active'` | Lifecycle state: `ready` or `archived` |
| `DEPRECATED_archived` | INTEGER | `0` | Old boolean, superseded by `state` |
| `active_session_id` | TEXT (UUID) | | FK to `sessions.id` — the currently active AI session |
| `initialization_parent_branch` | TEXT | | Branch the worktree was created from (e.g. `main`) |
| `intended_target_branch` | TEXT | | Branch this worktree's changes should merge into |
| `placeholder_branch_name` | TEXT | | Temporary branch name before final naming |
| `unread` | INTEGER | `0` | Whether workspace has unread AI messages |
| `big_terminal_mode` | INTEGER | `0` | Per-workspace toggle for expanded terminal view |
| `setup_log_path` | TEXT | | Path to setup script execution log (in `/var/folders/...`) |
| `initialization_log_path` | TEXT | | Path to worktree initialization log |
| `initialization_files_copied` | INTEGER | | Whether files were copied during init |
| `pinned_at` | TEXT | | Timestamp if workspace is pinned in UI |
| `linked_workspace_ids` | TEXT | | JSON array of linked workspace IDs (cross-repo linking) |
| `notes` | TEXT | | User notes for this workspace |
| `manual_status` | TEXT | | User-set status override |
| `derived_status` | TEXT | `'in-progress'` | Computed status (e.g. `in-progress`) |
| `archive_commit` | TEXT | | Git commit SHA stored when archiving |
| `created_at` | TEXT | `datetime('now')` | Row creation timestamp |
| `updated_at` | TEXT | `datetime('now')` | Auto-updated via trigger |

---

### `sessions`

AI chat sessions. Each workspace has one active session (referenced by `workspaces.active_session_id`), but the schema supports multiple sessions per workspace via the `workspace_id` column.

| Column | Type | Default | Description |
|--------|------|---------|-------------|
| `id` | TEXT (UUID) | PK | Unique identifier |
| `workspace_id` | TEXT (UUID) | | FK to `workspaces.id` |
| `status` | TEXT | `'idle'` | Session state: `idle`, `error` |
| `model` | TEXT | | AI model (e.g. `opus`) |
| `permission_mode` | TEXT | `'default'` | Tool permission level: `default` |
| `thinking_enabled` | INTEGER | `1` | Whether extended thinking is on |
| `codex_thinking_level` | TEXT | | Codex thinking level (e.g. `high`) |
| `agent_type` | TEXT | | Which agent backend: `claude` |
| `title` | TEXT | `'Untitled'` | Session title (user-visible) |
| `context_used_percent` | FLOAT | | How much of the context window is used |
| `context_token_count` | INTEGER | `0` | Raw token count |
| `claude_session_id` | TEXT | | External session ID for Claude API |
| `unread_count` | INTEGER | `0` | Unread message count |
| `freshly_compacted` | INTEGER | `0` | Whether session was recently compacted |
| `is_compacting` | INTEGER | `0` | Whether compaction is in progress |
| `is_hidden` | INTEGER | `0` | Whether session is hidden in UI |
| `last_user_message_at` | TEXT | | Timestamp of last user message |
| `resume_session_at` | TEXT | | Scheduled resume timestamp |
| `DEPRECATED_thinking_level` | TEXT | `'NONE'` | Old thinking config |
| `created_at` | TEXT | `datetime('now')` | Row creation timestamp |
| `updated_at` | TEXT | `datetime('now')` | Auto-updated via trigger |

---

### `session_messages`

Individual messages within a session. Content is stored as either plain text (user messages) or JSON (assistant messages containing tool calls, thinking, etc.).

| Column | Type | Default | Description |
|--------|------|---------|-------------|
| `id` | TEXT (UUID) | PK | Unique identifier |
| `session_id` | TEXT (UUID) | | FK to `sessions.id` |
| `role` | TEXT | | `user` or `assistant` |
| `content` | TEXT | | Message content — plain text or full JSON |
| `full_message` | TEXT | | Extended message payload (used by slash commands) |
| `sent_at` | TEXT | | ISO 8601 timestamp when sent |
| `cancelled_at` | TEXT | | Timestamp if message was cancelled |
| `model` | TEXT | | Model used for this specific message |
| `sdk_message_id` | TEXT | | API-level message ID |
| `last_assistant_message_id` | TEXT | | Reference to previous assistant message |
| `turn_id` | TEXT | | Groups messages in the same turn (user msg + all assistant responses + tool results) |
| `created_at` | TEXT | `datetime('now')` | Row creation timestamp |

**Indexes**: `session_id + sent_at`, `session_id + cancelled_at`, `turn_id`

#### Assistant Message Content Structure

Assistant messages store the full Claude API response as JSON:

```json
{
  "type": "assistant",
  "message": {
    "model": "claude-opus-4-5-20251101",
    "id": "msg_...",
    "role": "assistant",
    "content": [
      { "type": "thinking", "thinking": "..." },
      { "type": "text", "text": "..." },
      { "type": "tool_use", "id": "toolu_...", "name": "Bash", "input": { "command": "..." } }
    ]
  },
  "session_id": "...",
  "parent_tool_use_id": null
}
```

Tool results come as separate messages with role `assistant` but type `user` internally:

```json
{
  "type": "user",
  "message": {
    "role": "user",
    "content": [
      { "tool_use_id": "toolu_...", "type": "tool_result", "content": "...", "is_error": false }
    ]
  },
  "parent_tool_use_id": null,
  "session_id": "..."
}
```

The first assistant message in a session is a system init:

```json
{
  "type": "system",
  "subtype": "init",
  "cwd": "/Users/bkegley/conductor/workspaces/frontend/spokane",
  "session_id": "...",
  "tools": ["Task", "Bash", "Glob", "Grep", ...]
}
```

---

### `attachments`

Files (images, documents) attached to sessions.

| Column | Type | Default | Description |
|--------|------|---------|-------------|
| `id` | TEXT (UUID) | PK | Unique identifier |
| `type` | TEXT | | MIME category (e.g. `image`) |
| `original_name` | TEXT | | Original filename (e.g. `image.png`) |
| `path` | TEXT | | Absolute path on disk (stored in worktree's `.context/attachments/`) |
| `is_loading` | INTEGER | `0` | Whether file is still being processed |
| `session_id` | TEXT (UUID) | | FK to `sessions.id` |
| `session_message_id` | TEXT (UUID) | | FK to `session_messages.id` |
| `is_draft` | INTEGER | `1` | `1` = not yet sent, `0` = sent with a message |
| `created_at` | TEXT | `datetime('now')` | Row creation timestamp |

**Indexes**: `session_id`, `session_message_id`, `is_draft`

---

### `diff_comments`

PR review comments linked to a workspace's diff.

| Column | Type | Default | Description |
|--------|------|---------|-------------|
| `id` | TEXT (UUID) | PK | Unique identifier |
| `workspace_id` | TEXT (UUID) | | FK to `workspaces.id` |
| `file_path` | TEXT | | File the comment is on |
| `line_number` | INTEGER | | Line number in the diff |
| `body` | TEXT | | Comment text |
| `state` | TEXT | | Comment state (e.g. resolved, pending) |
| `location` | TEXT | | Position context |
| `author` | TEXT | | Comment author |
| `remote_url` | TEXT | | Remote URL for the PR |
| `thread_id` | TEXT | | Groups threaded replies |
| `reply_to_comment_id` | TEXT | | Parent comment in a thread |
| `update_memory` | INTEGER | | Whether to persist this comment to AI memory |
| `created_at` | INTEGER | | Unix timestamp |
| `updated_at` | INTEGER | | Unix timestamp |

**Index**: `workspace_id`

---

### `settings`

Global key-value application settings.

| Column | Type | Default | Description |
|--------|------|---------|-------------|
| `key` | TEXT | PK | Setting name |
| `value` | TEXT | | Setting value (string) |
| `created_at` | TEXT | `datetime('now')` | Row creation timestamp |
| `updated_at` | TEXT | `datetime('now')` | Auto-updated via trigger |

#### Known Settings

| Key | Example Value | Description |
|-----|---------------|-------------|
| `default_model` | `opus` | Default AI model for new sessions |
| `branch_prefix_type` | `github_username` | How branch names are prefixed |
| `default_codex_thinking_level` | `high` | Codex thinking level default |
| `review_codex_thinking_level` | `high` | Thinking level for code reviews |
| `conductor_api_token` | `6uQH...` | API auth token |
| `first_time_onboarding_step` | `999` | Onboarding progress |
| `default_open_in` | `zed` | Preferred editor |
| `show_cost_in_topbar` | `true` | Show session cost |
| `always_show_context_wheel` | `true` | Show context usage indicator |
| `zoom_level_v2` | `1` | UI zoom |
| `last_seen_announcement` | `2.33.0` | Dismisses changelog banners |
| `last_clone_directory` | `/Users/bkegley/conductor/repos` | Default clone path |
| `onboarding_step` | `0` | Current onboarding state |
| `conductor_config_repo_root_migration_complete` | `true` | One-time migration flag |

---

### `_sqlx_migrations`

Internal migration tracking table. Contains ~70 migrations documenting the full schema evolution. Not application data.

---

## Filesystem Layout

```
~/conductor/
├── repos/                              # Bare git clones
│   ├── frontend/                       # Cloned repo
│   │   ├── .git/
│   │   ├── .conductor/                 # Reserved directory (currently empty)
│   │   └── <source files>
│   ├── prismatic/
│   └── ...
│
├── workspaces/                         # Git worktrees, grouped by repo name
│   ├── frontend/
│   │   ├── lisbon/                     # A worktree (city name)
│   │   │   ├── .git                    # Worktree git link file
│   │   │   ├── .context/              # Conductor workspace-level context
│   │   │   │   ├── notes.md           # User notes
│   │   │   │   ├── todos.md           # User todos
│   │   │   │   ├── attachments/       # Images/files attached to sessions
│   │   │   │   └── <other docs>/      # Additional context files (reports, etc.)
│   │   │   └── <source files>
│   │   ├── cape-town/
│   │   └── ...
│   └── prismatic/
│       ├── bullard/
│       └── ...
│
└── archived-contexts/                  # Preserved context from archived worktrees
    ├── frontend/
    │   ├── dakar/
    │   │   ├── notes.md
    │   │   └── todos.md
    │   └── ...
    └── ...
```

### Key Filesystem Observations

- **`.conductor/`** exists in repo roots but is currently empty. Likely reserved for future file-based config (e.g. a `conductor.json` that gets committed to the repo).
- **`.context/`** exists in each active worktree. Contains `notes.md`, `todos.md`, and optionally `attachments/` and additional context documents.
- **`archived-contexts/`** preserves the `.context/` contents (notes, todos) after a worktree is archived and its directory is deleted.
- Attachment paths follow versioned naming: `image.png`, `image-v1.png`, `image-v2.png`, etc.
- Setup and initialization logs are written to temporary directories (`/var/folders/.../conductor-*-.log`), not persisted in the workspace.

---

## User Interaction Workflows

### Adding a New Repo

When a user adds a new repository to Conductor:

1. User provides a git remote URL (e.g. `git@github.com:org/repo.git`)
2. Conductor clones the repo to `~/conductor/repos/<repo-name>/`
3. A new row is inserted into `repos`:

```json
{
  "id": "generated-uuid",
  "name": "repo-name",
  "remote_url": "git@github.com:org/repo.git",
  "default_branch": "main",
  "root_path": "/Users/bkegley/conductor/repos/repo-name",
  "remote": "origin",
  "display_order": 13,
  "conductor_config": null,
  "setup_script": null,
  "run_script": null,
  "archive_script": null,
  "run_script_mode": "concurrent",
  "custom_prompt_code_review": null,
  "custom_prompt_create_pr": null,
  "custom_prompt_rename_branch": null,
  "custom_prompt_general": null
}
```

4. An empty `.conductor/` directory is created inside the cloned repo
5. The `last_clone_directory` setting is updated to `~/conductor/repos`

### Configuring a Repo

When a user configures setup scripts, run scripts, or coding preferences for a repo:

1. User opens repo settings in the UI
2. Changes are written to the `conductor_config` JSON column on the `repos` row:

```json
{
  "scripts": {
    "setup": "mise trust && make setup",
    "run": "cd frontend && npm run start"
  },
  "runScriptMode": "concurrent"
}
```

3. Custom AI prompts are stored in their own columns:

```json
{
  "custom_prompt_code_review": "Focus on performance and security...",
  "custom_prompt_create_pr": "Use conventional commits format...",
  "custom_prompt_rename_branch": null,
  "custom_prompt_general": "This codebase uses React and TypeScript..."
}
```

The legacy `setup_script`, `run_script`, and `archive_script` columns are not used — everything goes through `conductor_config`.

### Creating a New Worktree (Workspace)

When a user creates a new workspace from a repo:

1. Conductor generates a random city name (e.g. "lisbon")
2. A git worktree is created: `git worktree add ~/conductor/workspaces/<repo-name>/<city-name> -b <branch-prefix>/<city-name>`
3. The branch is created from the repo's `default_branch` (usually `main`)
4. A `.context/` directory is created inside the worktree with empty `notes.md` and `todos.md`
5. A new row is inserted into `workspaces`:

```json
{
  "id": "generated-uuid",
  "repository_id": "repo-uuid",
  "directory_name": "lisbon",
  "branch": "bkegley/lisbon",
  "state": "ready",
  "initialization_parent_branch": "main",
  "intended_target_branch": "main",
  "derived_status": "in-progress",
  "manual_status": null,
  "big_terminal_mode": 0,
  "pinned_at": null,
  "linked_workspace_ids": null,
  "notes": null
}
```

6. If the repo has a `scripts.setup` in `conductor_config`, it is executed in the worktree directory. The output is logged to a temp file and the path is stored in `setup_log_path` and/or `initialization_log_path`.
7. A new AI session is created in `sessions`:

```json
{
  "id": "session-uuid",
  "workspace_id": "workspace-uuid",
  "status": "idle",
  "model": "opus",
  "permission_mode": "default",
  "thinking_enabled": 1,
  "agent_type": "claude",
  "title": "Untitled",
  "is_hidden": 0,
  "codex_thinking_level": "high"
}
```

8. The workspace's `active_session_id` is set to this session's ID.

### Sending a Message to a Workspace

When a user sends a message in a workspace's AI session:

1. The message is inserted into `session_messages`:

```json
{
  "id": "message-uuid",
  "session_id": "session-uuid",
  "role": "user",
  "content": "Fix the bug in the login form",
  "sent_at": "2026-02-05T13:28:35.918Z",
  "model": "opus",
  "turn_id": "message-uuid"
}
```

2. Conductor sends the message to the Claude CLI running in the worktree directory
3. Assistant responses stream back as one or more `session_messages` with `role: "assistant"`, all sharing the same `turn_id`
4. Tool calls and results are stored as separate message rows within the same turn
5. The session's `status` updates (`idle` → active → `idle` or `error`)
6. `context_used_percent` is updated as the conversation grows

### Attaching Files

1. User drags an image or file into the session
2. The file is saved to `<worktree>/.context/attachments/<filename>.png`
3. If multiple versions exist, files are suffixed: `image.png`, `image-v1.png`, `image-v2.png`
4. An `attachments` row is inserted with `is_draft: 1`
5. When the user sends the message, `is_draft` is set to `0` and `session_message_id` is populated

### Archiving a Worktree

When a user archives a workspace:

1. The `workspaces.state` is updated from `ready` to `archived`
2. If the archive script exists in `conductor_config`, it executes
3. The current commit SHA may be stored in `archive_commit`
4. The `.context/` directory (notes.md, todos.md) is copied to `~/conductor/archived-contexts/<repo-name>/<city-name>/`
5. The git worktree directory is removed from `~/conductor/workspaces/<repo-name>/<city-name>/`
6. The `workspaces` row persists in the database (state = `archived`) so the history, sessions, and messages remain accessible

### Running a Workspace's Dev Server

When a user triggers the "run" action for a workspace:

1. Conductor reads `repos.conductor_config` → `scripts.run`
2. The run command is executed in the worktree directory (e.g. `~/conductor/workspaces/frontend/lisbon/`)
3. `runScriptMode: "concurrent"` means it runs alongside the AI session, not blocking it
4. Output appears in the workspace's terminal panel

### Pinning a Workspace

Setting `workspaces.pinned_at` to an ISO 8601 timestamp causes the workspace to sort to the top of the UI. Setting it to `null` unpins it.

### Linking Workspaces

Cross-repo workspace linking (e.g. a frontend worktree linked to a backend worktree) is stored in `linked_workspace_ids` as a JSON array of workspace UUIDs.

### Changing AI Model or Settings Per-Session

Each session has its own `model`, `permission_mode`, `thinking_enabled`, and `codex_thinking_level`. These override the global defaults from the `settings` table for that specific session.

### PR Review Comments

When Conductor fetches PR review comments (from GitHub/GitLab), they are stored in `diff_comments` linked to the workspace. The `update_memory` flag indicates whether a comment should be fed back into the AI's memory for learning.

---
