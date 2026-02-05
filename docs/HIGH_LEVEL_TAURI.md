# High-Level Tauri Architecture

This doc covers only the parts of a Tauri app that are relevant to our specific feature set: managing repos/worktrees, storing config in SQLite, and navigating to or creating Claude Code terminal sessions.

## App Structure

Tauri apps have two sides:

- **Rust backend** — runs as a native process with full system access. Handles SQLite, spawning shell commands, process inspection, AppleScript execution. Exposes functions to the frontend via `#[tauri::command]`.
- **Web frontend** — renders the UI in a webview. Calls backend commands via `invoke()`. Can be React, Svelte, whatever.

The frontend never touches the filesystem or processes directly. Every system interaction goes through a Tauri command.

## SQLite

Tauri has no built-in SQLite. Use `rusqlite` (or `sqlx` if you want async) in the Rust backend. The database lives at the app's data directory, which Tauri provides via `app.path().app_data_dir()` — resolves to something like `~/Library/Application Support/com.your-app-id/`.

Expose queries as Tauri commands. The frontend calls them, the backend runs them and returns serialized results.

For our data model (repos, workspaces, sessions, settings) — see `CONDUCTOR_DATA_MODEL.md`. The schema is straightforward to replicate. Migrations can be run at app startup using raw SQL files or embedded strings.

## Git Operations

All git operations (clone, worktree add, branch create, etc.) happen in the Rust backend via shell commands. Use `std::process::Command` to run git.

Key operations:

- **Clone a repo**: `git clone <url> <path>`
- **Create a worktree**: `git worktree add <path> -b <branch> <start-point>`
- **Remove a worktree**: `git worktree remove <path>`
- **List worktrees**: `git worktree list --porcelain`

These are triggered by Tauri commands invoked from the frontend, and the results (success/failure, paths) are written back to SQLite.

## The Claude Session Button

This is the core interaction. Each workspace row in the UI has a button to open its Claude Code session. The behavior depends on whether a session is already running.

### What Happens on Click

```
User clicks "Open Claude" on workspace (repo: "frontend", worktree: "lisbon")
         │
         ▼
Frontend calls: invoke("open_claude_session", { repoName: "frontend", worktreeName: "lisbon" })
         │
         ▼
Rust backend runs the detection + navigation logic (described below)
         │
         ├── Found running session → focus its terminal
         │
         └── No running session → open new terminal with claude
```

### Backend: The Tauri Command

The `open_claude_session` command does three things in sequence:

**1. Compute the worktree path**

Combine the base workspace directory with repo name and worktree name:

```
/Users/<user>/conductor/workspaces/frontend/lisbon
```

**2. Check if claude is already running there**

Find all `claude` CLI processes and match by working directory. On macOS:

- Run `pgrep -x claude` to get PIDs
- For each PID, run `lsof -p <PID>` and extract the `cwd` line to get the process's working directory
- Compare against the worktree path

If you find a match, that's your active PID.

**Important**: Filter out Conductor-app-managed claude processes by checking the parent process chain. If a claude PID's parent chain leads back to your own Tauri app or to `Conductor.app`, skip it — you only want user-terminal claude processes. A simple heuristic: get the PPID via `ps -o ppid= -p <PID>`, then check if that parent is a shell (zsh, bash) rather than node/Electron/your app binary.

**3. Either focus the existing terminal or create a new one**

This branches based on whether a PID was found and which terminal environment is in use.

### Backend: Focusing an Existing Session

If a running claude process was found at the worktree path:

**Get its TTY**

Run `ps -o tty= -p <PID>` — returns something like `ttys028`.

**Try tmux first**

Check if tmux is running (`tmux list-sessions` succeeds). If so:

- Run `tmux list-panes -a -F "#{session_name}:#{window_index}.#{pane_index}|#{pane_tty}"`
- Find the pane whose TTY matches
- Focus it: `tmux select-window -t <session>:<window>` then `tmux select-pane -t <session>:<window>.<pane>`
- If the tmux session is inside iTerm, also activate iTerm (see below)

**Try iTerm2**

If tmux didn't match (or isn't running), use AppleScript to find and focus the iTerm tab. Execute this via `std::process::Command::new("osascript")` with `-e` flag:

```applescript
tell application "iTerm"
    repeat with w in windows
        repeat with t in tabs of w
            repeat with s in sessions of t
                if tty of s contains "ttys028" then
                    select t
                    tell w to activate
                    return
                end if
            end repeat
        end repeat
    end repeat
end tell
```

Replace `ttys028` with the actual TTY string. This brings the correct iTerm window/tab to the front.

### Backend: Creating a New Session

If no running claude process was found for the worktree:

**Detect the user's preferred terminal**

This could be a setting in the SQLite `settings` table (like Conductor's `default_open_in`), or auto-detected by checking what's running. Simplest approach: store a `terminal_preference` setting with values like `iterm`, `tmux`, or `auto`.

**iTerm2 — new tab**

Execute via `osascript`:

```applescript
tell application "iTerm"
    activate
    tell current window
        create tab with default profile
        tell current session
            write text "cd /Users/.../conductor/workspaces/frontend/lisbon && claude"
        end tell
    end tell
end tell
```

If no iTerm window exists, change `tell current window` to `set newWindow to (create window with default profile)` and `tell current session of newWindow`.

**tmux — new window**

If the user has an active tmux session:

```
tmux new-window -t <session> -n "frontend-lisbon" -c /path/to/worktree "claude"
```

If no tmux session exists, create one:

```
tmux new-session -d -s conductor -c /path/to/worktree -n "frontend-lisbon" "claude"
```

Then, if iTerm is the host terminal, activate iTerm via AppleScript so the user sees it.

**`claude` vs `claude --continue`**

When creating a new terminal session, decide whether to resume the last Claude Code conversation or start fresh. If the worktree has an existing Claude Code session (check `~/.claude/projects/-Users-...-frontend-lisbon/sessions-index.json` for entries), use `claude --continue` to resume. Otherwise, plain `claude`.

### Frontend: Reflecting Session State in the UI

The frontend needs to know whether each workspace has an active claude process so it can show the right indicator (green dot, "running" badge, etc.).

**Option A: Poll on an interval**

Call a Tauri command like `get_active_claude_sessions` every few seconds. The backend runs `pgrep` + `lsof` and returns a map of worktree paths to PIDs. The frontend matches these against workspace rows.

Polling every 3-5 seconds is fine — `pgrep` is fast, and the `lsof` calls are bounded by the number of claude processes (typically 1-5, not hundreds).

**Option B: Poll on focus**

Only refresh active session state when the Tauri window gains focus. Less overhead, slightly stale, but good enough for most use cases.

**Option C: Event-driven (harder)**

Use `kqueue` or `FSEvents` to watch for process changes. Significantly more complex for marginal benefit. Not recommended unless poll latency becomes a real problem.

### Putting It Together: Command Flow

```
┌─────────────────────────────────────────────────────┐
│  Frontend (webview)                                 │
│                                                     │
│  Workspace list:                                    │
│  ┌───────────────────────────────────────────┐      │
│  │ frontend / lisbon          [● Claude]     │      │
│  │ frontend / cape-town       [  Claude]     │      │
│  │ prismatic / bullard        [  Claude]     │      │
│  └───────────────────────────────────────────┘      │
│       │  click                                      │
│       ▼                                             │
│  invoke("open_claude_session",                      │
│         { repo: "frontend", worktree: "lisbon" })   │
└─────────────────────────────────┬───────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────┐
│  Rust backend                                       │
│                                                     │
│  1. path = ~/conductor/workspaces/frontend/lisbon   │
│                                                     │
│  2. pids = pgrep -x claude                          │
│     for pid in pids:                                │
│       cwd = lsof -p pid | grep cwd                  │
│       if cwd == path → found_pid = pid; break       │
│                                                     │
│  3a. If found_pid:                                  │
│      tty = ps -o tty= -p found_pid                  │
│      try tmux: match tty → focus pane               │
│      try iTerm: osascript → match tty → focus tab   │
│                                                     │
│  3b. If not found:                                  │
│      check settings.terminal_preference              │
│      osascript → new iTerm tab → cd && claude       │
│      OR tmux new-window → cd && claude              │
│                                                     │
│  return { status: "focused" | "created", pid }      │
└─────────────────────────────────────────────────────┘
```

## What We Don't Need to Build

For context — things the real Conductor does that are out of scope for a light version:

- Bundling our own claude/codex/gh/node binaries (use whatever the user has installed)
- The checkpointer/spotlighter system (rely on normal git)
- An embedded terminal in the app (we delegate to iTerm/tmux)
- The full session message storage and replay (Claude Code handles its own history)
- Diff viewer with inline comments
- The `index.bundled.js` backend process (Tauri's Rust backend replaces this)
