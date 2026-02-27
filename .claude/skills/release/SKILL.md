---
name: release
description: This skill should be used when the user asks to "create a release", "publish a release", "draft release notes", "bump version", "make a new version", "write a changelog", or wants to publish a GitHub release for the project.
disable-model-invocation: true
argument-hint: [patch|minor|major]
---

# Release

Create versioned GitHub releases with AI-drafted release notes for the Bunyan project.

## Workflow

### Step 1: Preflight Checks

Check for uncommitted changes with `git status --porcelain`. If there are any, warn the user and ask whether to proceed or stash first.

### Step 2: Determine Bump Type

Parse the bump type from `$ARGUMENTS`. Valid values: `patch`, `minor`, `major`. If no argument is provided, default to `patch`.

If the argument is ambiguous or missing, confirm the bump type with the user before proceeding.

### Step 3: Dry Run — Preview Version and Draft Notes

This step gathers all information and drafts release notes **without modifying any files**.

1. Compute the new version by running the bump script in dry-run mode:
   ```bash
   bash .claude/skills/release/scripts/bump-version.sh --dry-run <bump-type>
   ```
   This prints the version transition (e.g. `0.1.0 -> 0.2.0`) without modifying files.

2. Gather commit history:
   ```bash
   bash .claude/skills/release/scripts/get-release-context.sh
   ```

3. Analyze the commit list and draft release notes. Organize into sections based on commit content:

   - **Added** — new features, capabilities, integrations
   - **Fixed** — bug fixes, corrections
   - **Changed** — modifications to existing behavior, refactors, redesigns
   - **Removed** — removed features or deprecated items

   Guidelines for drafting:
   - Omit empty sections.
   - Write from the user's perspective, not the developer's. Focus on what changed, not how.
   - Combine related commits into single bullet points when they represent one logical change.
   - Keep bullets concise — one line each.
   - Do not include commit hashes in the release notes.
   - Use present tense ("Add container support" not "Added container support").
   - Skip trivial commits (typo fixes, gitignore changes) unless they're the only commits.

### Step 4: Review

Present the drafted release notes to the user for review. Include:

1. The version transition (e.g. `v0.1.0 → v0.2.0`)
2. The full drafted release notes
3. A reminder that **no files have been modified yet**

Ask the user to approve, edit, or reject the draft. If the user provides edits, incorporate them and present the updated draft. If the user rejects, stop — nothing was changed.

### Step 5: Apply Version Bump

Only after the user approves the release notes, run the bump script for real:

```bash
bash .claude/skills/release/scripts/bump-version.sh <bump-type>
```

If the script fails, consult `references/versioning.md` for the file locations and fix the issue manually.

### Step 6: Commit and Tag

1. Update `Cargo.lock` by running `cargo check`, then stage everything:
   ```bash
   cargo check --manifest-path src-tauri/Cargo.toml
   git add package.json src-tauri/tauri.conf.json bunyan-core/Cargo.toml bunyan-cli/Cargo.toml src-tauri/Cargo.toml Cargo.lock
   ```

2. Commit with the message: `release: v<new-version>`

3. Create an annotated git tag:
   ```bash
   git tag -a v<new-version> -m "v<new-version>"
   ```

### Step 7: Publish GitHub Release

Confirm with the user before publishing. Then create the GitHub release:

```bash
gh release create v<new-version> --title "v<new-version>" --notes "$(cat <<'EOF'
<release notes here>
EOF
)"
```

After publishing, print the release URL returned by `gh`.

Remind the user to push the commit and tag:
```bash
git push && git push --tags
```

## Error Handling

- If `gh` is not authenticated, instruct the user to run `gh auth login`.
- If the version bump script fails, read `references/versioning.md` and update files manually.
- If there are uncommitted changes before starting, warn the user and ask whether to proceed or stash first.

## Additional Resources

### Reference Files

- **`references/versioning.md`** — all version file locations, line numbers, and formats

### Scripts

- **`scripts/bump-version.sh`** — bumps version across all project files
- **`scripts/get-release-context.sh`** — extracts commit history since last tag
