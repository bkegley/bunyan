---
name: bunyan-delegate
description: Fire-and-forget agent delegation. Hand off a side-task to a fresh Claude in an isolated worktree, get back a URL, and forget about it. Use whenever the work being requested is incidental to your current task and should not consume your context.
---

# Bunyan: fire-and-forget delegation

You are the *parent* agent. This skill exists so that you can spawn a side-task
to handle something incidental — a bug fix in another file, a docs pass, a
dependency upgrade — and **never think about it again.** The spawned task
runs in its own worktree with its own fresh Claude. You move on.

## When to use this

- The user mentions a thing that should get done but doesn't belong in the
  current changeset ("oh and while you're at it…", "we should also fix X").
- You notice a problem outside your current scope that deserves attention.
- You're about to write "TODO" or note something for later — delegate
  instead.

## When NOT to use this

- The work is part of your current task. Just do it.
- The work needs your judgment as you go. Delegation is for "run to
  completion independently."

## The one tool

```bash
curl -s -X POST http://127.0.0.1:3333/delegate \
  -H 'content-type: application/json' \
  -d @- <<'JSON'
{
  "repo": "<repo-name>",
  "branch": "<new-branch-name>",
  "prompt": "<the full task description you'd hand to a fresh Claude>",
  "from": "<your workspace_id if you have it>"
}
JSON
```

Response (201 Created):

```json
{
  "workspace_id": "ws_5f8e",
  "observation_url": "http://127.0.0.1:3333/workspaces/ws_5f8e"
}
```

**Critical:** Log the `observation_url` somewhere durable (your notes,
the user-visible turn output) so future-you or a reviewer can find the
result. Then move on. Do NOT poll the workspace, do NOT wait for it to
finish, do NOT read its transcript. That's a different skill
(`bunyan-review`) for a different consumer.

## Prompt-writing tips

The spawned Claude has zero context except what you put in `prompt`.
Include:

- The exact thing to do, with file paths if you know them.
- The success criterion ("tests pass", "PR opens", "commits land on
  branch X").
- Any standing conventions that matter (e.g. "this repo prefers small
  atomic commits").
- A note if it should write its outcome to `result.json` at the worktree
  root — observers will read that via `GET /workspaces/:id/result`.

## Guardrails

- The daemon must be running (`curl -s http://127.0.0.1:3333/health`).
  If it isn't, run `bunyan up` first.
- Branch names must be valid git branches. The new worktree is created
  off the repo's default branch.
- `from` is optional but recommended — it links the spawned workspace to
  the parent for lineage queries.
- Delegation depth: a delegated agent can delegate further. Watch out for
  loops.

## What you should NOT do here

- Don't query `/workspaces`, `/sessions/active`, transcripts, or results
  from this skill. Use `bunyan-review` for that.
- Don't try to archive, kill panes, or otherwise manage the spawned
  workspace's lifecycle from this skill. That's also `bunyan-review`'s
  job.
