#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?Usage: generate-update-manifest.sh <version> <notes>}"
NOTES="${2:-}"
REPO_ROOT="$(git rev-parse --show-toplevel)"
BUNDLE_DIR="$REPO_ROOT/src-tauri/target/release/bundle"
REPO_SLUG="bkegley/bunyan"

# Find the .tar.gz (updater artifact) and its signature
TAR_GZ=$(find "$BUNDLE_DIR" -name "*.app.tar.gz" ! -name "*.sig" 2>/dev/null | head -1)
SIG_FILE="${TAR_GZ}.sig"

if [[ -z "$TAR_GZ" ]]; then
  echo "ERROR: No .app.tar.gz found in $BUNDLE_DIR" >&2
  exit 1
fi

TAR_GZ_NAME=$(basename "$TAR_GZ")
DOWNLOAD_URL="https://github.com/${REPO_SLUG}/releases/download/v${VERSION}/${TAR_GZ_NAME}"

# Read signature if available
SIGNATURE=""
if [[ -f "$SIG_FILE" ]]; then
  SIGNATURE=$(cat "$SIG_FILE")
fi

PUB_DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Determine current architecture
ARCH=$(uname -m)
case "$ARCH" in
  arm64) TAURI_TARGET="darwin-aarch64" ;;
  x86_64) TAURI_TARGET="darwin-x86_64" ;;
  *) TAURI_TARGET="darwin-${ARCH}" ;;
esac

# Build the platform entry
PLATFORM_ENTRY=$(cat <<ENTRY
"${TAURI_TARGET}": {
      "url": "${DOWNLOAD_URL}",
      "signature": "${SIGNATURE}"
    }
ENTRY
)

# Write manifest
OUTPUT="$REPO_ROOT/latest.json"
cat > "$OUTPUT" <<MANIFEST
{
  "version": "${VERSION}",
  "notes": "${NOTES}",
  "pub_date": "${PUB_DATE}",
  "platforms": {
    ${PLATFORM_ENTRY}
  }
}
MANIFEST

echo "$OUTPUT"
