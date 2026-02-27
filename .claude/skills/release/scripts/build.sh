#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"

# Check for signing key
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  echo "WARNING: TAURI_SIGNING_PRIVATE_KEY not set. Updater signatures will not be generated." >&2
  echo "Generate keys with: cargo tauri signer generate -w ~/.tauri/bunyan.key" >&2
  echo "Then export: export TAURI_SIGNING_PRIVATE_KEY=\$(cat ~/.tauri/bunyan.key)" >&2
fi

cd "$REPO_ROOT"
npx tauri build 2>&1

# Find and list artifacts
BUNDLE_DIR="$REPO_ROOT/src-tauri/target/release/bundle"

echo ""
echo "=== Build Artifacts ==="

# macOS artifacts
for ext in dmg app.tar.gz app.tar.gz.sig; do
  found=$(find "$BUNDLE_DIR" -name "*.${ext}" 2>/dev/null)
  if [[ -n "$found" ]]; then
    while IFS= read -r f; do
      echo "$f"
    done <<< "$found"
  fi
done
