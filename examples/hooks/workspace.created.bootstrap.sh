#!/usr/bin/env bash
# Per-repo bootstrap example. Drop a customized copy of this at
# ~/bunyan/repos/<repo>/.bunyan/hooks/workspace.created so each new worktree
# is ready to develop in immediately.
#
# This hook gets a longer timeout (5 minutes) than the default events,
# because bootstrap can be slow.
set -euo pipefail
cd "$BUNYAN_PATH"

# Tool versions, if mise is in use:
command -v mise >/dev/null 2>&1 && mise install

# Common per-stack bootstraps — keep only what applies:
[ -f package-lock.json ] && npm install
[ -f yarn.lock ] && yarn install
[ -f pnpm-lock.yaml ] && pnpm install
[ -f Cargo.toml ] && cargo fetch
[ -f Gemfile ] && bundle install

# Seed .env from .env.example if both exist and .env is missing.
[ -f .env.example ] && [ ! -f .env ] && cp .env.example .env

echo "[$BUNYAN_REPO/$BUNYAN_WORKSPACE] bootstrap complete"
