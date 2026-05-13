#!/usr/bin/env bash
# Stash any notes the user left in the worktree before tear-down.
set -euo pipefail

notes_dir="$BUNYAN_PATH/notes"
if [ -d "$notes_dir" ] && [ "$(ls -A "$notes_dir" 2>/dev/null)" ]; then
  archive="$HOME/.local/share/bunyan/notes-archive/$BUNYAN_REPO/$BUNYAN_WORKSPACE"
  mkdir -p "$archive"
  rsync -a "$notes_dir/" "$archive/"
  echo "archived notes -> $archive"
fi
