# Container Workflows

## Overview

Bunyan can run workspaces inside Docker containers. Each container-mode workspace gets its own Docker container with the worktree mounted, a dedicated network, and Claude installed.

## Prerequisites

- Docker daemon running
- Server running

## Check Docker Availability

```bash
curl -s http://127.0.0.1:3333/docker/status
```

Returns `{"available": true}` or `{"available": false}`.

## Create a Container Workspace

```bash
curl -s -X POST http://127.0.0.1:3333/workspaces \
  -H 'Content-Type: application/json' \
  -d '{
    "repository_id": "<REPO_ID>",
    "directory_name": "container-fix",
    "branch": "fix/container-fix",
    "container_mode": "container"
  }'
```

This:
1. Creates the git worktree
2. Creates a Docker network (`bunyan-<repo-name>`)
3. Creates a container with the worktree mounted
4. Installs Claude Code in the container
5. Returns the workspace with `container_id` set

The container image defaults to `node:22` unless configured in the repo's `config.container.image`.

## Check Container Status

```bash
curl -s http://127.0.0.1:3333/workspaces/<ID>/container/status
```

Returns `{"status": "running"}`, `{"status": "exited"}`, or `{"status": "none"}`.

## Get Port Mappings

```bash
curl -s http://127.0.0.1:3333/workspaces/<ID>/container/ports
```

Returns array of `{"container_port": "3000/tcp", "host_port": "3000", "host_ip": "0.0.0.0"}`.

## Container Config

Set via the repo's `config` field:

```json
{
  "container": {
    "enabled": true,
    "image": "node:22",
    "ports": ["3000:3000"],
    "env": {"NODE_ENV": "development"},
    "shell": "/bin/bash",
    "dangerously_skip_permissions": false
  }
}
```

- `image`: Docker image (default: `node:22`)
- `ports`: Port mappings (host:container format)
- `env`: Environment variables
- `shell`: Shell for interactive sessions
- `dangerously_skip_permissions`: Pass `--dangerously-skip-permissions` to Claude

## Archive Container Workspace

Archiving a container workspace removes the container and cleans up the network (if no other workspaces use it):

```bash
curl -s -X POST http://127.0.0.1:3333/workspaces/<ID>/archive
```

## Error Handling

| Error | Cause | Fix |
|---|---|---|
| 500 Docker error | Docker not running | Start Docker daemon |
| 500 Docker error | Image pull failed | Check image name in repo config |
| `container_id: null` | Container creation failed | Check Docker logs |
