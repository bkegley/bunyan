# Bunyan hooks

Bunyan publishes lifecycle events. You subscribe by dropping executable
scripts in well-known directories on disk. The model mirrors git hooks —
any shebanged file works, language-agnostic, no plugin manifest.

## Discovery order

When an event fires, bunyan runs hooks in this order (all matching hooks
run, sequentially):

1. `~/bunyan/repos/<repo>/.bunyan/hooks/<event>` — per-repo
2. `~/bunyan/repos/<repo>/.bunyan/hooks/<event>.d/*` — per-repo fan-out
3. `~/.config/bunyan/hooks/<event>` — user-global
4. `~/.config/bunyan/hooks/<event>.d/*` — user-global fan-out

A hook returning **exit code 78** short-circuits the rest for that event.

## Events bunyan emits

| Event | When |
| --- | --- |
| `workspace.created` | After worktree + DB + container are set up |
| `workspace.ready_to_view` | Whenever a workspace should be surfaced |
| `workspace.archived` | Before tear-down |
| `claude.started` | After a new Claude pane is created |
| `claude.resumed` | After resuming an existing Claude session |
| `claude.stopped` | Spawned Claude finished a turn |
| `claude.subagent_stopped` | A `Task`-spawned sub-agent finished |
| `claude.notification` | Claude is waiting / timed out |
| `claude.session_started` | Claude opened a new session |

## Context bunyan passes to a hook

**Environment variables** (canonical for shell-style hooks):

| Var | Example |
| --- | --- |
| `BUNYAN_EVENT` | `workspace.ready_to_view` |
| `BUNYAN_EVENT_TIMESTAMP` | `2026-05-14T...` |
| `BUNYAN_REPO` | `frontend` |
| `BUNYAN_REPO_ID` | uuid |
| `BUNYAN_WORKSPACE` | `fix-flaky-test` |
| `BUNYAN_WORKSPACE_ID` | uuid |
| `BUNYAN_PATH` | absolute path to the worktree |
| `BUNYAN_BRANCH` | branch name |
| `BUNYAN_SERVER_PORT` | bunyan daemon port |
| `BUNYAN_<EXTRA>` | event-specific (e.g. `BUNYAN_ATTACH_CMD`, `BUNYAN_SESSION_ID`) |

**JSON on stdin** (preferred for Python/Node/Deno hooks):

```json
{
  "event": "workspace.ready_to_view",
  "version": 1,
  "timestamp": "...",
  "server": { "port": 3333 },
  "repo": { "id": "...", "name": "frontend", "path": "..." },
  "workspace": { "id": "...", "name": "...", "path": "...", "branch": "..." },
  "extras": { "attach_cmd": "tmux -L bunyan attach -t frontend" }
}
```

## Installing an example

The bunyan repo ships templates at `examples/hooks/`:

```sh
# Restore the legacy iTerm window-open behavior:
mkdir -p ~/.config/bunyan/hooks
cp examples/hooks/workspace.ready_to_view.iterm.sh \
   ~/.config/bunyan/hooks/workspace.ready_to_view
chmod +x ~/.config/bunyan/hooks/workspace.ready_to_view
```

Other templates in `examples/hooks/`: `workspace.ready_to_view.zellij.sh`,
`workspace.created.bootstrap.sh`, `workspace.archived.backup-notes.sh`,
`claude.started.slack.py`.

## HTTP, not just disk

Every event also publishes onto an SSE stream:

```bash
curl -N http://127.0.0.1:3333/events
```

Same envelopes, different transport. Use disk hooks for "this should
run on the local box every time"; use SSE for "I want a remote dashboard
to subscribe."

## Debugging

```bash
# See what hooks would run for an event
bunyan hooks list workspace.ready_to_view --repo myrepo

# Fire an event by hand
bunyan hooks run workspace.ready_to_view --workspace <id>

# Hook output is captured and surfaced in the bunyan daemon's log
```

## What hooks should NOT do

- **Don't write to bunyan's database.** Hooks read context, do side
  effects. State changes go through the HTTP API.
- **Don't block forever.** Per-event timeouts apply (default 10s, 5min for
  `workspace.created`). Long-running side effects should detach with `&`
  or `setsid`.
- **Don't assume cwd.** Hooks run in an unspecified working directory.
  Use `$BUNYAN_PATH` or `cd` explicitly.
