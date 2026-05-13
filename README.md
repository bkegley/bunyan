# Bunyan

The substrate for fire-and-forget agent delegation. Your current Claude says
"spawn a side-task to handle X" via `POST /delegate` and immediately forgets
it ever happened; a fresh Claude runs the side-task to completion in its own
worktree. Bunyan keeps track of the spawned work so a human (or a different
agent) can come back and review it later.

Internally bunyan owns git worktrees, tmux/zellij/etc. process supervision,
optional Docker containers, and a filesystem-based hook system that lets you
react to lifecycle events. The desktop GUI and CLI are both clients of the
same HTTP server.

## Quick Start

The recommended way to use Bunyan is via the CLI. The desktop GUI is optional and documented further down.

```sh
# Install the CLI (macOS Apple Silicon prebuilt binary)
curl -fsSL https://raw.githubusercontent.com/bkegley/bunyan/main/install.sh | bash

# Start the background daemon
bunyan up

# Register a repo and create a worktree
bunyan repo create --name myrepo --remote-url git@github.com:you/myrepo.git --root-path ~/bunyan/repos/myrepo
bunyan workspace create --repo <repo-id> --name feature-x --branch feature-x
```

The installer drops the binary into `$HOME/.local/bin` (or `$BUNYAN_INSTALL_DIR` / `$XDG_BIN_DIR` if set). Other platforms can build from source with `cargo install --git https://github.com/bkegley/bunyan bunyan-cli`.

`bunyan --help` lists every subcommand. Stop the daemon with `bunyan down`.

## Coding Agent Skill

This repo ships a skill at `skills/bunyan/` that teaches coding agents (Claude Code, Cursor, etc.) how to drive the Bunyan HTTP API. Install it with [skills.sh](https://skills.sh):

```sh
# Project-scoped
npx skills add bkegley/bunyan

# Global
npx skills add bkegley/bunyan -g
```

skills.sh auto-detects installed agents. Pass `-a <agent>` to target a specific one.

## Features

- **Worktree-based workflows** — Each task gets its own Git worktree with an isolated branch, dependencies, and state. No more stashing or context-switching.
- **Background sessions** — Claude Code and shell sessions run in tmux behind the scenes. They persist across app restarts and survive closing your terminal.
- **Container isolation** — Optionally run workspaces inside Docker containers with automatic volume mounts, port forwarding, and per-repo network isolation.
- **Desktop GUI** — Two-panel interface with a tree sidebar for repos and worktrees and a detail panel showing active panes, port mappings, and session history.
- **CLI** — Full-featured `bunyan` command for headless and scripted usage. Talks to the same backend as the GUI.
- **Event hooks** — Drop executable scripts in `~/.config/bunyan/hooks/<event>` to react to workspace lifecycle. Replaces the previous hardcoded iTerm window flow. See `examples/hooks/`.

## Use Cases

- Managing multiple feature branches simultaneously without stashing or switching
- Running long-lived Claude Code sessions in the background while working on other tasks
- Spinning up containerized dev environments per-worktree with port forwarding
- Scripting workspace creation and session management via the CLI
- Keeping a persistent overview of all active repos, worktrees, and running sessions

## How It Works

Bunyan runs an HTTP server (default port 3333) that both the desktop GUI and CLI connect to. A dedicated tmux server on the `bunyan` socket provides the session backbone — each repo maps to a tmux session, each worktree to a window, and each process (Claude or shell) to a pane. SQLite stores repo and workspace metadata. Git worktrees and cloned repos live on disk under `~/bunyan/`.

Sessions persist independently of the GUI. Closing your terminal or quitting the app doesn't kill running processes — Claude keeps working in the background. Archiving a workspace tears down its tmux window, removes the Git worktree, and (if applicable) stops its Docker container.

## Hooks

Bunyan publishes lifecycle events (`workspace.created`, `workspace.ready_to_view`, `workspace.archived`, `claude.started`, `claude.resumed`, `claude.stopped`, etc.) and runs any executable scripts you've placed at `~/.config/bunyan/hooks/<event>` or `~/bunyan/repos/<repo>/.bunyan/hooks/<event>` when they fire. This is how you wire up "open the workspace in iTerm/zellij/etc," per-worktree bootstrap (`mise install`, `npm install`, …), Slack notifications, and anything else you want bunyan to trigger.

Browse `examples/hooks/` for templates — including a drop-in replacement for the legacy iTerm window flow. See `bunyan hooks list <event>` and `bunyan hooks run <event> --workspace <id>` for debugging.

## Runtime backends

Bunyan supervises Claude/shell processes through a pluggable backend. Today it ships with two:

- **`tmux`** (default) — what bunyan has always used. One tmux session per repo, one window per workspace.
- **`zellij`** — first-class for zellij users. One zellij session per repo, one tab per workspace.

Pick one in `~/.config/bunyan/config.toml`:

```toml
[runtime]
backend = "zellij"

# Per-repo override (e.g. stay on tmux when working on bunyan itself):
[runtime.repos.bunyan]
backend = "tmux"
```

If the file's missing or doesn't specify, bunyan uses tmux. Switching is a config edit — no fork required.

## Development

### Prerequisites

- **Rust** (stable toolchain) — [rustup.rs](https://rustup.rs)
- **Node.js 22+** — via [mise](https://mise.jdx.dev), nvm, or direct install
- **tmux** — `brew install tmux`
- **A terminal multiplexer/launcher** — bunyan no longer pops terminals itself; configure a `workspace.ready_to_view` hook (see `examples/hooks/`) to open iTerm, zellij, ghostty, etc.
- **Docker** (optional) — required only for container-based workspaces

### Project Structure

The repo is a Cargo workspace with three crates and a React frontend:

```
bunyan-core/       Shared Rust library (models, db, tmux, git, docker, HTTP server)
bunyan-cli/        CLI binary — runs the daemon and talks to the HTTP server
src-tauri/         Tauri desktop app — launches the bunyan daemon and serves the React frontend
src/               React frontend (single-page app, Vite + TypeScript)
```

### Running Locally

**Desktop app (GUI + server):**

The Tauri shell spawns the `bunyan` CLI to run the daemon, so make sure it's installed and on PATH first (see Quick Start).

```sh
npm install
npx tauri dev
```

This starts the Vite dev server and the Tauri app simultaneously. The Rust backend compiles on first run and rebuilds on changes. The React frontend has HMR via Vite.

**CLI only:**

```sh
cargo build -p bunyan-cli
```

The CLI requires a running server. Use `bunyan up` to start the daemon in the background (or `bunyan serve` to run it in the foreground). `bunyan down` stops the daemon.

### Production Build

```sh
npx tauri build
```

Outputs a `.dmg` and `.app` bundle in `src-tauri/target/release/bundle/`.
