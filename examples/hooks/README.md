# Example bunyan hooks

Bunyan publishes lifecycle events; you subscribe by dropping executable scripts
in well-known directories. The model mirrors git hooks — any shebanged file works.

## Discovery order

When an event fires, bunyan runs hooks in this order (all matching hooks run,
sequentially):

1. `~/bunyan/repos/<repo>/.bunyan/hooks/<event>` — per-repo, lives with the repo
2. `~/bunyan/repos/<repo>/.bunyan/hooks/<event>.d/*` — per-repo fan-out
3. `~/.config/bunyan/hooks/<event>` — user-global
4. `~/.config/bunyan/hooks/<event>.d/*` — user-global fan-out

A hook returning **exit code 78** short-circuits the rest for that event.

## Events bunyan emits today

| Event | When it fires | Notable extras |
| --- | --- | --- |
| `workspace.created` | After worktree + DB + container are set up | — |
| `workspace.ready_to_view` | Whenever a workspace should be surfaced to the user | `attach_cmd` (the runtime's attach command), `backend` |
| `workspace.archived` | Before tear-down | — |
| `claude.started` | After a new Claude pane is created | — |
| `claude.resumed` | After resuming an existing Claude session | `session_id` |
| `claude.stopped` | Spawned Claude finished a turn (Stop hook) | `claude_event`, `payload` (raw Claude hook JSON) |
| `claude.subagent_stopped` | Sub-agent (Task) finished | `claude_event`, `payload` |
| `claude.notification` | Claude is waiting / timed out | `claude_event`, `payload` |
| `claude.session_started` | Claude opened a new session | `claude_event`, `payload` |

> The `claude.*` events at the bottom of the table are re-fired by bunyan
> when a delegated Claude reports back via its injected
> `.claude/settings.local.json` hooks. They mirror Claude Code's hook
> events but are namespaced into bunyan's convention.

## Context bunyan passes to a hook

**As environment variables** (canonical):

| Var | Example |
| --- | --- |
| `BUNYAN_EVENT` | `workspace.ready_to_view` |
| `BUNYAN_EVENT_TIMESTAMP` | `2026-05-13T14:23:01Z` |
| `BUNYAN_REPO` | `frontend` |
| `BUNYAN_REPO_ID` | uuid |
| `BUNYAN_WORKSPACE` | `fix-flaky-test` |
| `BUNYAN_WORKSPACE_ID` | uuid |
| `BUNYAN_PATH` | absolute path to the worktree |
| `BUNYAN_BRANCH` | branch name (when known) |
| `BUNYAN_SERVER_PORT` | bunyan daemon port |
| `BUNYAN_<EXTRA>` | event-specific extras |

**As JSON on stdin** (richer; preferred for Python/Node/Deno hooks):

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

```sh
mkdir -p ~/.config/bunyan/hooks
cp examples/hooks/workspace.ready_to_view.iterm.sh \
   ~/.config/bunyan/hooks/workspace.ready_to_view
chmod +x ~/.config/bunyan/hooks/workspace.ready_to_view
```

(Remove the `.iterm.sh` / `.zellij.sh` suffix when copying — bunyan looks for
files named exactly after the event.)

## Debugging

```sh
# See what hooks would run for an event
bunyan hooks list workspace.ready_to_view

# Dry-run an event against a specific workspace
bunyan hooks run workspace.ready_to_view --workspace <ws-id>
```
