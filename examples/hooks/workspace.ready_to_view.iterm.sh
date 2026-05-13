#!/usr/bin/env bash
# Reproduces bunyan's pre-hooks iTerm behavior. Copy this to
# ~/.config/bunyan/hooks/workspace.ready_to_view to restore the old default.
set -euo pipefail

session_title="Bunyan: $BUNYAN_REPO / $BUNYAN_WORKSPACE"
# $BUNYAN_ATTACH_CMD is set by the active runtime backend (tmux today).
attach_cmd="${BUNYAN_ATTACH_CMD:-tmux -L bunyan attach -t $BUNYAN_REPO}"

# Try to focus an existing window first; only open a new one if none match.
focus_script=$(cat <<EOF
tell application "iTerm"
  set found to false
  repeat with w in windows
    repeat with t in tabs of w
      repeat with s in sessions of t
        if name of s contains "$session_title" then
          select t
          tell w to activate
          set found to true
          exit repeat
        end if
      end repeat
      if found then exit repeat
    end repeat
    if found then exit repeat
  end repeat
  return found
end tell
EOF
)
if [ "$(osascript -e "$focus_script" 2>/dev/null)" = "true" ]; then
  exit 0
fi

osascript <<EOF
tell application "iTerm"
  activate
  set newWindow to (create window with default profile)
  tell current session of newWindow
    set name to "$session_title"
    write text "$attach_cmd"
  end tell
end tell
EOF
