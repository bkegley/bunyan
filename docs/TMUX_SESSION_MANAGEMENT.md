## Tmux-Native Session Management

Replaces the current pgrep/lsof + iTerm/tmux hybrid detection with a Bunyan-owned tmux server that manages all Claude and shell sessions internally.

### Architecture

**Dedicated tmux server**: All Bunyan-managed sessions run under `tmux -L bunyan`. This is fully isolated from the user's personal tmux server. iTerm is used solely as the terminal emulator that attaches to this server.

**Topology**:
```
tmux server (socket: bunyan)
  └── session: <repo-name>           # one per repo (created lazily)
        ├── window: <workspace-name> # one per workspace
        │     ├── pane 0: claude     # Claude session
        │     ├── pane 1: claude     # Another Claude session (--resume <id>)
        │     └── pane 2: zsh        # Shell pane
        └── window: <workspace-name>
              └── pane 0: claude
```

**Lazy creation**: The tmux server, sessions, and windows are created on-demand when the user first interacts with a workspace. Nothing is pre-created on app launch.

**Persistence**: The tmux server survives Bunyan app restarts. When Bunyan quits, the tmux server and all running Claude sessions continue in the background. On next launch, Bunyan reconnects to the existing server and discovers running sessions.

### Session Detection

All detection uses tmux-native queries. No more pgrep/lsof/ps.

```bash
# List all panes across the bunyan server
tmux -L bunyan list-panes -a -F "#{session_name}:#{window_name}.#{pane_index}|#{pane_pid}|#{pane_current_command}|#{pane_current_path}"
```

This gives us:
- Which repo sessions exist
- Which workspace windows exist within each session
- Which panes are running Claude vs shell vs idle
- The working directory of each pane

To determine if a pane is running Claude specifically, check `pane_current_command` for `claude` or walk child processes of `pane_pid`.

### User Flows

#### Click "Claude" on a workspace

1. Check if a tmux window exists for this workspace: `tmux -L bunyan list-windows -t <repo> -F "#{window_name}"` and look for `<workspace-name>`
2. **If window exists and has Claude running**: Open iTerm, attach to the tmux session targeting that window:
   ```bash
   tmux -L bunyan select-window -t <repo>:<workspace>
   # iTerm opens with:
   tmux -L bunyan attach-session -t <repo>
   ```
3. **If window exists but no Claude running** (e.g. only idle shell panes): Start Claude in an idle pane or create a new pane, then attach.
4. **If no window exists**: Create the repo session if needed, create the workspace window, start Claude in it, then attach:
   ```bash
   # If session doesn't exist:
   tmux -L bunyan new-session -d -s <repo> -n <workspace> -c <workspace-path> "claude"
   # If session exists but window doesn't:
   tmux -L bunyan new-window -t <repo> -n <workspace> -c <workspace-path> "claude"
   # Then attach:
   # Open new iTerm window with: tmux -L bunyan attach-session -t <repo>
   ```

#### Resume a historical session

1. Check if the workspace window has an idle pane (a pane where `pane_current_command` is the shell, not `claude`).
2. **If idle pane exists**: Send the resume command to it:
   ```bash
   tmux -L bunyan send-keys -t <repo>:<workspace>.<pane> "claude --resume <session-id>" Enter
   ```
3. **If no idle pane**: Split the window to create a new pane:
   ```bash
   tmux -L bunyan split-window -t <repo>:<workspace> -c <workspace-path> "claude --resume <session-id>"
   ```
4. Attach iTerm if not already attached.

#### Open a shell pane

Same as resume, but the command is just the user's default shell (or no command — tmux defaults to shell).

```bash
# New pane with shell
tmux -L bunyan split-window -t <repo>:<workspace> -c <workspace-path>
```

#### Archive a workspace

1. Kill the tmux window for the workspace (terminates all panes including running Claude sessions):
   ```bash
   tmux -L bunyan kill-window -t <repo>:<workspace>
   ```
2. If this was the last window in the repo session, tmux automatically destroys the session.
3. Proceed with existing archive logic (update DB state, remove git worktree).

### iTerm Interaction

**Attach method**: Standard `tmux attach-session` (not `-CC` integration mode). The user gets the standard tmux experience with prefix keys for pane navigation.

**Window management**: Each attach opens a **new iTerm window**. This allows the user to have multiple workspaces visible simultaneously.

```applescript
tell application "iTerm"
    set newWindow to (create window with default profile)
    tell current session of newWindow
        write text "tmux -L bunyan attach-session -t <repo>"
    end tell
end tell
```

If the user is already attached to the target repo session in an existing iTerm window, we should focus that window instead of creating a duplicate attachment.

### Frontend UI Changes

**Workspace row**: Instead of a single "Claude" button, the workspace row shows:
- Status indicator: number of active Claude panes + shell panes
- "Claude" button: starts new Claude session or attaches to existing
- "Shell" button: opens a shell pane
- Expandable pane list

**Pane list** (expanded per workspace):
- Each pane shows: type (Claude/Shell), running command, session prompt (if Claude)
- Click a pane → attach iTerm to that workspace window
- Kill button per pane → `tmux -L bunyan kill-pane -t <target>`

**Session history** (existing feature, unchanged):
- Expandable list of past Claude sessions from `~/.claude/projects/`
- Click to resume → creates/reuses pane as described above

### Backend Changes

**`process.rs`** — Replace `ProcessDetector` trait:
- Remove `find_claude_pids()`, `get_pid_cwd()`, `get_pid_tty()`
- Add `TmuxManager` with methods:
  - `list_sessions()` → list all repo-level tmux sessions
  - `list_windows(session)` → list workspace windows in a session
  - `list_panes(session, window)` → list panes with status
  - `is_server_running()` → check if `tmux -L bunyan` server exists

**`terminal.rs`** — Simplify to tmux-only:
- Remove `open_iterm_session()`, `focus_iterm_session()`, `focus_tmux_pane()` (old approach)
- Add:
  - `ensure_workspace_window(repo, workspace, path)` → create session/window if needed
  - `create_claude_pane(repo, workspace, path, cmd)` → new pane running Claude
  - `create_shell_pane(repo, workspace, path)` → new shell pane
  - `kill_workspace_window(repo, workspace)` → destroy window and all panes
  - `kill_pane(repo, workspace, pane_index)` → destroy single pane
  - `attach_iterm(repo)` → open iTerm window attached to repo session

**`commands/claude.rs`** — Update command handlers:
- `open_claude_session()` → use `TmuxManager` + `ensure_workspace_window()` instead of process detection
- `get_active_claude_sessions()` → query tmux panes instead of pgrep
- New commands:
  - `open_shell_pane(workspace_id)`
  - `kill_pane(workspace_id, pane_index)`
  - `list_workspace_panes(workspace_id)` → returns pane details for UI

**`models.rs`** — New types:
```rust
struct TmuxPane {
    pane_index: u32,
    command: String,         // "claude" or "zsh" etc.
    is_active: bool,         // is this the currently selected pane
    workspace_path: String,
}

struct WorkspacePaneInfo {
    workspace_id: String,
    panes: Vec<TmuxPane>,
}
```

### Naming Conventions

- **Tmux socket**: `bunyan`
- **Tmux session name**: repo `name` field from DB (e.g. `frontend`, `prismatic`)
- **Tmux window name**: workspace `directory_name` from DB (e.g. `lisbon`, `nairobi`)
- **Collisions**: If a user has two repos with the same name, append a suffix. This is an edge case since repo names are derived from remote URLs and must be unique in the DB.

### Migration Path

1. The old `process.rs` (pgrep/lsof) and iTerm-specific code in `terminal.rs` can be removed entirely.
2. Existing running Claude sessions (started outside Bunyan's tmux server) will no longer be detected by Bunyan. This is acceptable — the user starts fresh with Bunyan-managed sessions.
3. The "Other claude sessions: N" footer in the UI is removed since Bunyan only tracks its own sessions now.
