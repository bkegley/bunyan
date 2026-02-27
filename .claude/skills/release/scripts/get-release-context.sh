#!/usr/bin/env bash
set -euo pipefail

LATEST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")

if [[ -z "$LATEST_TAG" ]]; then
  echo "No previous tags found. Showing all commits."
  echo "---"
  git log --oneline --no-decorate
else
  COMMIT_COUNT=$(git rev-list "$LATEST_TAG"..HEAD --count)
  echo "Changes since $LATEST_TAG ($COMMIT_COUNT commits):"
  echo "---"
  git log --oneline --no-decorate "$LATEST_TAG"..HEAD
fi
