# Bunyan

A development environment manager for macOS that orchestrates Git worktrees, tmux sessions, Docker containers, and Claude Code from a desktop app or CLI. Built with Tauri (Rust) and React (TypeScript).

## Features

- **Worktree-based workflows** — Each task gets its own Git worktree with an isolated branch, dependencies, and state. No more stashing or context-switching.
- **Background sessions** — Claude Code and shell sessions run in tmux behind the scenes. They persist across app restarts and survive closing your terminal.
- **Container isolation** — Optionally run workspaces inside Docker containers with automatic volume mounts, port forwarding, and per-repo network isolation.
- **Desktop GUI** — Two-panel interface with a tree sidebar for repos and worktrees and a detail panel showing active panes, port mappings, and session history.
- **CLI** — Full-featured `bunyan` command for headless and scripted usage. Talks to the same backend as the GUI.
- **iTerm integration** — Automatically manages iTerm windows (one per repo) with tmux title propagation for easy identification.

## Use Cases

- Managing multiple feature branches simultaneously without stashing or switching
- Running long-lived Claude Code sessions in the background while working on other tasks
- Spinning up containerized dev environments per-worktree with port forwarding
- Scripting workspace creation and session management via the CLI
- Keeping a persistent overview of all active repos, worktrees, and running sessions

## How It Works

Bunyan runs an HTTP server (default port 3333) that both the desktop GUI and CLI connect to. A dedicated tmux server on the `bunyan` socket provides the session backbone — each repo maps to a tmux session, each worktree to a window, and each process (Claude or shell) to a pane. SQLite stores repo and workspace metadata. Git worktrees and cloned repos live on disk under `~/bunyan/`.

Sessions persist independently of the GUI. Closing iTerm or quitting the app doesn't kill running processes — Claude keeps working in the background. Archiving a workspace tears down its tmux window, removes the Git worktree, and (if applicable) stops its Docker container.

## Development

### Prerequisites

- **Rust** (stable toolchain) — [rustup.rs](https://rustup.rs)
- **Node.js 22+** — via [mise](https://mise.jdx.dev), nvm, or direct install
- **tmux** — `brew install tmux`
- **iTerm2** — Bunyan uses AppleScript to manage iTerm windows
- **Docker** (optional) — required only for container-based workspaces

### Project Structure

The repo is a Cargo workspace with three crates and a React frontend:

```
bunyan-core/       Shared Rust library (models, db, tmux, git, docker, HTTP server)
bunyan-cli/        CLI binary — talks to the HTTP server
src-tauri/         Tauri desktop app — wraps bunyan-core, serves the React frontend
src/               React frontend (single-page app, Vite + TypeScript)
```

### Running Locally

**Desktop app (GUI + server):**

```sh
npm install
npx tauri dev
```

This starts the Vite dev server and the Tauri app simultaneously. The Rust backend compiles on first run and rebuilds on changes. The React frontend has HMR via Vite.

**CLI only:**

```sh
cargo build -p bunyan-cli
```

The CLI requires a running server. You can either start the desktop app or run the server headlessly with `bunyan serve`.

### Production Build

```sh
npx tauri build
```

Outputs a `.dmg` and `.app` bundle in `src-tauri/target/release/bundle/`.

### Data Locations

- **SQLite DB**: `~/Library/Application Support/com.bunyan.app/bunyan.db`
- **Repos**: `~/bunyan/repos/<name>/`
- **Worktrees**: `~/bunyan/workspaces/<repo>/<worktree>/`
- **Server port file**: `~/.bunyan/server.port`
