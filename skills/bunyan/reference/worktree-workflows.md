# Worktree Workflows

## Overview

Bunyan workspaces are git worktrees. Each workspace gets its own directory, branch, and optionally a Docker container. Use worktrees for isolated side-fixes without disrupting your main work.

## Prerequisites

- Server running (`curl -s http://127.0.0.1:3333/health`)
- Repository already registered (`GET /repos`)

## Create a Local Worktree

```bash
# 1. Find the repo ID
curl -s http://127.0.0.1:3333/repos | jq '.[] | {id, name}'

# 2. Create the workspace
curl -s -X POST http://127.0.0.1:3333/workspaces \
  -H 'Content-Type: application/json' \
  -d '{
    "repository_id": "<REPO_ID>",
    "directory_name": "fix-login-bug",
    "branch": "fix/login-bug",
    "container_mode": "local"
  }'
```

The workspace directory will be at `~/bunyan/workspaces/<repo-name>/fix-login-bug`.

## Create a Container Worktree

```bash
curl -s -X POST http://127.0.0.1:3333/workspaces \
  -H 'Content-Type: application/json' \
  -d '{
    "repository_id": "<REPO_ID>",
    "directory_name": "fix-login-bug",
    "branch": "fix/login-bug",
    "container_mode": "container"
  }'
```

This creates the worktree AND spins up a Docker container with the code mounted.

## List Workspaces

```bash
# All workspaces
curl -s http://127.0.0.1:3333/workspaces

# Filter by repo
curl -s http://127.0.0.1:3333/workspaces?repo_id=<REPO_ID>
```

## Archive a Workspace

Archives removes the worktree, kills tmux panes, and removes any container:

```bash
curl -s -X POST http://127.0.0.1:3333/workspaces/<ID>/archive
```

## View a Workspace

Focus the workspace in iTerm (creates tmux window if needed):

```bash
curl -s -X POST http://127.0.0.1:3333/workspaces/<ID>/view
```

## Side-Fix Workflow

Complete workflow for making a fix in an isolated worktree:

```bash
# 1. Check health
curl -s http://127.0.0.1:3333/health

# 2. Find repo
REPO_ID=$(curl -s http://127.0.0.1:3333/repos | jq -r '.[0].id')

# 3. Create worktree
WS=$(curl -s -X POST http://127.0.0.1:3333/workspaces \
  -H 'Content-Type: application/json' \
  -d "{
    \"repository_id\": \"$REPO_ID\",
    \"directory_name\": \"side-fix\",
    \"branch\": \"fix/side-fix\"
  }")
WS_ID=$(echo $WS | jq -r '.id')

# 4. Navigate to worktree and make changes
# The path is in the workspace response or derived:
# ~/bunyan/workspaces/<repo-name>/side-fix

# 5. Archive when done
curl -s -X POST http://127.0.0.1:3333/workspaces/$WS_ID/archive
```

## Validation

After creating a workspace, verify:
- Response contains `id`, `directory_name`, `branch`, `state: "ready"`
- For container mode: `container_id` is set
- The directory exists on disk

## Error Handling

| Error | Cause | Fix |
|---|---|---|
| 404 | Repo ID not found | Check `GET /repos` for valid IDs |
| 500 Git error | Branch already exists or invalid name | Use a unique branch name |
| 500 Docker error | Docker not running (container mode) | Check `GET /docker/status` |
