## Identifying Active Claude Code Sessions for Worktrees

### The Problem

Given a Conductor workspace (e.g. `frontend/lisbon`), can we determine if a Claude Code CLI session is actively running in a terminal for that worktree? And can we either navigate to that terminal or create a new one?

### What We Know

#### Claude Code's Local Storage

Claude Code stores session data in `~/.claude/projects/`. Each project directory is named by sanitizing the absolute path (replacing `/` with `-`):

```
~/.claude/projects/-Users-bkegley-conductor-workspaces-frontend-lisbon/
├── sessions-index.json
├── <session-uuid>.jsonl
└── memory/
```

The `sessions-index.json` contains metadata about all sessions for that project:

```json
{
  "version": 1,
  "entries": [
    {
      "sessionId": "bfe6491e-...",
      "fullPath": "/Users/bkegley/.claude/projects/-Users-bkegley-conductor-workspaces-frontend-lisbon/bfe6491e-....jsonl",
      "fileMtime": 1770299070157,
      "firstPrompt": "It looks like it's working when I submit...",
      "messageCount": 23,
      "created": "2026-02-05T13:28:35.918Z",
      "modified": "2026-02-05T13:44:30.124Z",
      "gitBranch": "bkegley/ewb-build-with-ai",
      "projectPath": "/Users/bkegley/conductor/workspaces/frontend/lisbon",
      "isSidechain": false
    }
  ],
  "originalPath": "/Users/bkegley/conductor/workspaces/frontend/lisbon"
}
```

However, this only tells us about past sessions — it does not include a PID, socket, or any indicator of whether a session is currently running. Claude Code has no lock files, PID files, or socket files in `~/.claude/`.

### Strategy: Process-Based Detection

The most reliable approach is detecting running `claude` processes and mapping them to worktree directories.

#### Step 1: Find All Running Claude Processes

```
pgrep -x claude
```

Returns PIDs of all running `claude` CLI processes (not the desktop app).

#### Step 2: Map Each PID to Its Working Directory

On macOS, `lsof` can get the current working directory of a process:

```
lsof -p <PID> | grep cwd
```

This returns the worktree path, for example:

```
PID 73850 -> /Users/bkegley/conductor/workspaces/frontend/lisbon
PID 69165 -> /Users/bkegley/repos/prismatic/frontend/main
PID 10571 -> /Users/bkegley/repos/prismatic/workclaw
```

#### Step 3: Match to Conductor Workspace

Given a workspace with `directory_name: "lisbon"` in repo `frontend`, its path is:

```
~/conductor/workspaces/frontend/lisbon
```

If any `claude` process has its CWD set to that path, the session is active.

#### Step 4: Find Which Terminal Contains That Process

##### For tmux

```
tmux list-panes -a -F "#{session_name}:#{window_index}.#{pane_index}|#{pane_pid}|#{pane_current_path}|#{pane_current_command}"
```

This lists every pane across all tmux sessions with its PID, CWD, and current command. You can match the `pane_pid` to the `claude` PID (or match `pane_current_path` to the worktree path). To find the claude process, you may need to walk the process tree since `pane_pid` is the shell PID and `claude` is a child:

```
# Get child processes of a tmux pane's shell
ps -o pid,comm --ppid <pane_pid>
```

Or match by directory:

```
tmux list-panes -a -F "#{session_name}:#{window_index}.#{pane_index}|#{pane_current_path}" | grep "conductor/workspaces/frontend/lisbon"
```

To focus that pane:

```
tmux select-window -t <session>:<window>
tmux select-pane -t <session>:<window>.<pane>
```

Or attach to the session if detached:

```
tmux attach-session -t <session>
```

##### For iTerm2

iTerm2 exposes session information via AppleScript:

```applescript
tell application "iTerm"
    repeat with w in windows
        repeat with t in tabs of w
            repeat with s in sessions of t
                set tty_name to tty of s
                -- Match tty_name against the TTY from ps output
            end repeat
        end repeat
    end repeat
end tell
```

The flow is:
1. From the `claude` PID, get its TTY: `ps -o tty -p <PID>` → e.g. `ttys028`
2. Iterate iTerm sessions, match by TTY device
3. Focus the matching tab/window:

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

### Strategy: Creating a New Terminal Session

If no `claude` process is running for a workspace, create one:

##### iTerm2 — New Tab

```applescript
tell application "iTerm"
    tell current window
        create tab with default profile
        tell current session
            write text "cd /Users/bkegley/conductor/workspaces/frontend/lisbon && claude"
        end tell
    end tell
end tell
```

##### iTerm2 — New Window

```applescript
tell application "iTerm"
    set newWindow to (create window with default profile)
    tell current session of newWindow
        write text "cd /Users/bkegley/conductor/workspaces/frontend/lisbon && claude"
    end tell
end tell
```

##### tmux — New Window in Existing Session

```
tmux new-window -t <session> -c /Users/bkegley/conductor/workspaces/frontend/lisbon "claude"
```

##### tmux — New Session

```
tmux new-session -d -s frontend-lisbon -c /Users/bkegley/conductor/workspaces/frontend/lisbon "claude"
tmux attach-session -t frontend-lisbon
```

### Putting It All Together: Detection Algorithm

```
Input: workspace (repo_name, directory_name)
       e.g. ("frontend", "lisbon")

1. Compute worktree path:
   path = ~/conductor/workspaces/{repo_name}/{directory_name}

2. Find matching claude process:
   for each PID in `pgrep -x claude`:
     cwd = `lsof -p PID | grep cwd` → extract path
     if cwd == path:
       found_pid = PID
       break

3. If found_pid exists (session is running):
   tty = `ps -o tty -p found_pid` → e.g. "ttys028"

   a. Check tmux first:
      pane_info = `tmux list-panes -a -F "#{session_name}:#{window_index}.#{pane_index}|#{pane_tty}"`
      match tty against pane_tty
      if match: focus that tmux pane

   b. Check iTerm:
      use AppleScript to iterate iTerm sessions
      match by tty
      if match: activate that iTerm window/tab

4. If not found (no running session):
   detect user's terminal (tmux active? iTerm frontmost?)
   create new terminal session at the worktree path
   optionally auto-launch `claude` or `claude --continue`
```

### Caveats and Limitations

- **`lsof` can be slow** on systems with many open files. Batching PIDs helps: `lsof -p PID1,PID2,PID3`.
- **Process CWD is the CWD at launch time** for the `claude` binary. If claude internally changes directories (it doesn't typically), the `lsof` CWD would still match the worktree root.
- **Conductor's own sessions** (in `conductor.db`) use a separate session management system from Claude Code CLI's `~/.claude/projects/` sessions. They are not the same session IDs. Conductor runs Claude through its bundled binary at `~/Library/Application Support/com.conductor.app/bin/claude`.
- **Multiple claude processes** could exist in the same worktree (e.g. if the user manually opened a second one). The detection should handle this gracefully, perhaps by showing all matches.
- **iTerm2 Python API** is more robust than AppleScript for complex automation but requires the iTerm2 Python runtime. The AppleScript approach works without any setup.
- **macOS permissions**: `lsof` and process inspection may require full disk access or developer tools permissions in newer macOS versions.
- **The Conductor desktop app** also runs a `claude` binary (bundled in its `bin/` directory). You need to distinguish between Conductor-managed claude processes and user-launched CLI claude processes. Conductor's claude processes will have a PPID that traces back to the Conductor app (PID of `/Applications/Conductor.app`). User-launched ones will have a shell (zsh/bash) or tmux/iTerm as their parent.

### Distinguishing Conductor-Managed vs User-Launched Claude

To determine if a `claude` process was launched by Conductor vs by the user in a terminal:

```
ps -o pid,ppid,comm -p <claude_pid>
```

Then trace the parent:

```
ps -o pid,ppid,comm -p <ppid>
```

- If the parent chain leads to `Conductor` (or `Electron` or a node process in the Conductor app bundle), it's Conductor-managed.
- If the parent chain leads to `zsh` → `tmux` or `zsh` → `iTerm2`, it's user-launched.

For our use case (navigating to the user's terminal), we care about user-launched claude processes specifically.
