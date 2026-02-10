# Project Workflows

## Overview

Repos are the top-level entities in Bunyan. Each repo represents a git repository that can have multiple worktree-based workspaces.

## Register a Repository

```bash
curl -s -X POST http://127.0.0.1:3333/repos \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "my-project",
    "remote_url": "git@github.com:user/my-project.git",
    "root_path": "/Users/me/bunyan/repos/my-project",
    "default_branch": "main",
    "remote": "origin"
  }'
```

This clones the repo to `root_path` and registers it in the database.

## List Repositories

```bash
curl -s http://127.0.0.1:3333/repos
```

## Get a Repository

```bash
curl -s http://127.0.0.1:3333/repos/<ID>
```

## Update a Repository

```bash
curl -s -X PUT http://127.0.0.1:3333/repos/<ID> \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "new-name",
    "default_branch": "develop",
    "config": {"container": {"enabled": true, "image": "node:22"}}
  }'
```

Only specified fields are updated.

## Delete a Repository

```bash
curl -s -X DELETE http://127.0.0.1:3333/repos/<ID>
```

Cascades to all workspaces for that repo.

## Repository Config

The `config` field is a JSON blob. The `container` key controls container behavior:

```json
{
  "container": {
    "enabled": true,
    "image": "node:22",
    "ports": ["3000:3000", "5432:5432"],
    "env": {"NODE_ENV": "development"},
    "shell": "/bin/bash",
    "dangerously_skip_permissions": false
  }
}
```

## Settings

Global settings stored as key-value pairs:

```bash
# List all
curl -s http://127.0.0.1:3333/settings

# Get one
curl -s http://127.0.0.1:3333/settings/theme

# Set one
curl -s -X PUT http://127.0.0.1:3333/settings/theme \
  -H 'Content-Type: application/json' \
  -d '{"value": "dark"}'
```
