# Delegating a side-task

You are the *parent* agent. This page exists so that you can spawn a
side-task to handle something incidental — a bug fix in another file, a
docs pass, a dependency upgrade — and **never think about it again.** The
spawned task runs in its own worktree with its own fresh Claude. You move
on.

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
finish, do NOT read its transcript. That's for reviewers (see
[`review.md`](review.md)), not for the parent.

## What bunyan does behind the scenes

So you don't have to ask the spawned agent to do these:

- Creates the worktree off the repo's default branch.
- Injects `.claude/settings.local.json` into the worktree with Stop /
  SubagentStop / Notification / SessionStart hooks that report back to
  bunyan. The spawned agent's prompt has zero awareness of bunyan.
- Captures the Claude session ID on the workspace row, so reviewers can
  `claude --resume <id>` directly.
- On the agent's Stop turn, persists the payload to `last_result` on the
  workspace row. `GET /workspaces/:id/result` reads it back.
- Fires bunyan lifecycle events (`workspace.created`, `claude.started`,
  `claude.stopped`, …) that user hooks and SSE clients receive.

## Prompt-writing tips

The spawned Claude has zero context except what you put in `prompt`.
Include:

- The exact thing to do, with file paths if you know them.
- The success criterion ("tests pass", "PR opens", "commits land on
  branch X", "writes a summary to `summary.md` at the repo root").
- Any standing conventions that matter (e.g. "this repo prefers small
  atomic commits").

You do NOT need to tell the agent to write `result.json` — bunyan auto-
captures the agent's Stop turn and surfaces it through
`GET /workspaces/:id/result` from a DB column, not the filesystem. If you
want a richer artifact for human review, ask the agent to write a
markdown file at the repo root (e.g. `summary.md`) and tell the reviewer
where to find it.

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
  from this flow. Use [`review.md`](review.md) for that.
- Don't archive, kill panes, or otherwise manage the spawned workspace's
  lifecycle. That's also for reviewers.
