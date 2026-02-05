# Project Setup

## Prerequisites (macOS)

### Xcode Command Line Tools

Desktop-only development doesn't need the full Xcode install:

```bash
xcode-select --install
```

### Rust

```bash
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

Restart your terminal after install. Verify with `rustc --version`.

### Node.js

Required for the React frontend. Install the LTS version from [nodejs.org](https://nodejs.org) or via a version manager like `mise` or `fnm`. Verify:

```bash
node -v
npm -v
```

## Create the Project

The official scaffolding tool handles everything:

```bash
npm create tauri-app@latest
```

When prompted:
- **Project name**: pick your name
- **Identifier**: e.g. `com.bunyan.app`
- **Frontend language**: TypeScript / JavaScript
- **Package manager**: npm (or pnpm/yarn/bun — your choice)
- **UI template**: React
- **UI flavor**: TypeScript

Other package manager variants:

```bash
pnpm create tauri-app
yarn create tauri-app
bun create tauri-app
```

## Install Dependencies and Run

```bash
cd <project-name>
npm install
npm run tauri dev
```

First run compiles the Rust backend, which takes a while. Subsequent runs are fast. A native window should open with the React app inside.

## Project Structure

After scaffolding, you'll have:

```
<project-name>/
├── package.json              # JS dependencies and scripts
├── index.html                # Web entry point
├── src/                      # React frontend
│   ├── App.tsx
│   ├── main.tsx
│   └── ...
├── src-tauri/                # Rust backend
│   ├── tauri.conf.json       # Tauri config (app id, window settings, dev server url)
│   ├── Cargo.toml            # Rust dependencies
│   ├── Cargo.lock
│   ├── build.rs              # Tauri build system hook
│   ├── capabilities/         # Security permissions for IPC commands
│   │   └── default.json
│   ├── icons/                # App icons (png, icns, ico)
│   └── src/
│       ├── main.rs           # Desktop entry point (thin — calls lib.rs)
│       └── lib.rs            # App setup, Tauri commands, plugin registration
└── vite.config.ts            # Vite config (dev server for the React app)
```

**Frontend** (`src/`) — standard React+Vite app. Calls the Rust backend via `invoke()` from `@tauri-apps/api`.

**Backend** (`src-tauri/src/lib.rs`) — where you define `#[tauri::command]` functions and register them with the app builder. This is where SQLite setup, shell command execution, and process detection logic will live.

**Config** (`src-tauri/tauri.conf.json`) — app identifier, window title/size, dev server URL, bundle settings. Tauri uses this to find the Rust project and configure the webview.

**Capabilities** (`src-tauri/capabilities/`) — Tauri v2's security model. Commands exposed to the frontend must be explicitly permitted here. You'll need to add permissions for any shell/process commands and IPC.

## Key Rust Dependencies to Add

After scaffolding, add these to `src-tauri/Cargo.toml` for our use case:

```toml
[dependencies]
rusqlite = { version = "0.32", features = ["bundled"] }  # SQLite
serde = { version = "1", features = ["derive"] }          # Serialization
serde_json = "1"                                           # JSON handling
uuid = { version = "1", features = ["v4"] }                # UUID generation
```

Install them by running `cargo build` from the `src-tauri/` directory or just `npm run tauri dev` (which triggers the Rust build).

## Useful Commands

| Command | What it does |
|---------|-------------|
| `npm run tauri dev` | Run the app in dev mode with hot reload |
| `npm run tauri build` | Build a production .app bundle |
| `npm run tauri icon <path>` | Generate all icon sizes from a source image |

## References

- [Tauri v2 — Create a Project](https://v2.tauri.app/start/create-project/)
- [Tauri v2 — Prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri v2 — Project Structure](https://v2.tauri.app/start/project-structure/)
