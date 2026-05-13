#!/usr/bin/env bash
# Open the workspace in zellij: one session per repo, one tab per workspace.
# Copy to ~/.config/bunyan/hooks/workspace.ready_to_view to use.
set -euo pipefail

session="$BUNYAN_REPO"
tab="$BUNYAN_WORKSPACE"

# Pick a layout: per-repo if it exists, fall back to a generic one.
layout="$HOME/.config/zellij/layouts.local/$BUNYAN_REPO.kdl"
[ -f "$layout" ] || layout="$HOME/.config/zellij/layouts/repo.kdl"
layout_arg=""
[ -f "$layout" ] && layout_arg="--layout $layout"

if zellij list-sessions -s 2>/dev/null | grep -qx "$session"; then
  zellij --session "$session" action new-tab \
    --name "$tab" \
    $layout_arg \
    --cwd "$BUNYAN_PATH"
else
  # No session yet; start one in a new terminal window. Adjust the launcher
  # to taste (ghostty, wezterm, kitty, etc.).
  ghostty --command "zellij $layout_arg -s '$session'" &
fi
