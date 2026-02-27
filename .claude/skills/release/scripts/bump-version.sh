#!/usr/bin/env bash
set -euo pipefail

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN=true
  shift
fi

BUMP_TYPE="${1:-patch}"
REPO_ROOT="$(git rev-parse --show-toplevel)"

# Parse current version from package.json
CURRENT_VERSION=$(grep -o '"version": "[^"]*"' "$REPO_ROOT/package.json" | head -1 | grep -o '[0-9]\+\.[0-9]\+\.[0-9]\+')

if [[ -z "$CURRENT_VERSION" ]]; then
  echo "ERROR: Could not parse current version from package.json" >&2
  exit 1
fi

IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"

case "$BUMP_TYPE" in
  major)
    MAJOR=$((MAJOR + 1))
    MINOR=0
    PATCH=0
    ;;
  minor)
    MINOR=$((MINOR + 1))
    PATCH=0
    ;;
  patch)
    PATCH=$((PATCH + 1))
    ;;
  *)
    echo "ERROR: Invalid bump type '$BUMP_TYPE'. Use: major, minor, or patch" >&2
    exit 1
    ;;
esac

NEW_VERSION="${MAJOR}.${MINOR}.${PATCH}"

echo "$CURRENT_VERSION -> $NEW_VERSION"

if [[ "$DRY_RUN" == true ]]; then
  exit 0
fi

# Update JSON files (package.json, tauri.conf.json)
for json_file in "$REPO_ROOT/package.json" "$REPO_ROOT/src-tauri/tauri.conf.json"; do
  sed -i '' "s/\"version\": \"$CURRENT_VERSION\"/\"version\": \"$NEW_VERSION\"/" "$json_file"
done

# Update Cargo.toml files
for cargo_file in "$REPO_ROOT/bunyan-core/Cargo.toml" "$REPO_ROOT/bunyan-cli/Cargo.toml" "$REPO_ROOT/src-tauri/Cargo.toml"; do
  sed -i '' "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" "$cargo_file"
done
