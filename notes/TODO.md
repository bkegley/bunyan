# TODO

## Repo Management
- Let user customize repo name on clone instead of only deriving from URL
- Repo config: expand beyond setup/run scripts (custom prompts, display_order, etc.)
- Repo deletion should also remove ~/bunyan/repos/<name>/ from disk, not just DB rows

## Workspace Management
- Figure out cleanup/deletion strategy for old archived worktrees
- Auto-run setup script from conductor_config on worktree creation (backend doesn't do this yet)

## Claude Sessions
- Figure out how to display non-bunyan claude sessions (workspace_id: None) — currently we track count but don't show them
