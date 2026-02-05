# Additional Binaries

Conductor bundles several binaries and scripts in `~/Library/Application Support/com.conductor.app/bin/`. Beyond the obvious ones (claude, codex, gh, node), there are five that power Conductor's checkpoint and file-sync system.

## watchexec

An open-source filesystem watcher. Monitors a directory for file changes (create, write, rename, delete) and runs a command in response. Conductor bundles it so it doesn't depend on anything installed on the user's machine.

Used by `spotlighter.sh` to detect when files change in a worktree.

## checkpointer.sh

A bash script that implements git-based snapshots using private refs (`refs/conductor-checkpoints/<id>`). Three subcommands:

**`save`** — Captures the full state of a worktree (HEAD position, staged index, and entire working tree including untracked files) into a single git commit object stored under a private ref. Does not move HEAD or touch any files on disk. Generates a checkpoint ID like `cp-20260205T131500Z` or accepts a custom one via `--id`.

**`restore`** — Reverts a worktree to a saved checkpoint. Resets HEAD, restores the working tree, cleans untracked files not in the snapshot, and restores the index to its saved state. This is how Conductor's "undo AI changes" works.

**`diff`** — Compares two checkpoints (or a checkpoint vs the current state). Produces a standard git diff of the full working-tree snapshots.

Checkpoints are stored as git objects inside the repo itself — no external storage needed. They're invisible to normal git operations because they live under `refs/conductor-checkpoints/`, not `refs/heads/` or `refs/tags/`.

## git-busy-check.sh

A guard script that checks whether a git repo has an in-progress operation that would prevent safe committing. Checks for:

- Active rebase (`rebase-merge/` or `rebase-apply/` directories)
- Active merge (`MERGE_HEAD`)
- Active cherry-pick (`CHERRY_PICK_HEAD`)
- Active revert (`REVERT_HEAD`)

Returns `"clean"` or `"busy:<operation>"`. Called by `checkpointer.sh save` before attempting a checkpoint — if the repo is busy, the checkpoint is skipped (exit code 101) rather than failing.

## spotlighter.sh

The orchestrator that ties watchexec and checkpointer together. It's the live sync mechanism that keeps the bare repo clone in sync with whatever is happening in a worktree.

The flow:

1. Launched in a worktree directory with env vars pointing to the checkpointer, watchexec, and the bare repo path (`CONDUCTOR_ROOT_PATH`)
2. Starts `watchexec` watching the worktree (ignores `.context/**` and `*.tmp.*`)
3. On any file change: runs `checkpointer save` in the worktree, then `checkpointer restore` in the bare repo
4. This means the bare repo always mirrors the latest state of whichever worktree is being "spotlighted"

The practical effect: when you're editing code in a worktree (or claude is editing it for you), the source repo automatically stays in sync. This is likely what powers Conductor's diff viewer — it can show what changed because it has both the checkpoint and the current state.

Logs to `/tmp/conductor-spotlight-<pid>.log`.

## index.bundled.js

A minified/bundled Node.js application (run via the bundled `node` binary). This is Conductor's backend process — the server-side logic that the desktop app (Tauri/Electron) spawns to manage sessions, execute scripts, coordinate the Claude CLI, and handle the bridge between the UI and git/terminal operations.
