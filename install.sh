#!/usr/bin/env bash
set -euo pipefail

REPO="bkegley/bunyan"
BINARY="bunyan"

color() { printf "\033[1;%dm%s\033[0m\n" "$1" "$2"; }
info()  { color 34 "==> $1"; }
ok()    { color 32 "==> $1"; }
fail()  { color 31 "==> $1" >&2; exit 1; }

# Detect OS + arch
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
  arm64|aarch64) ARCH=aarch64 ;;
  x86_64|amd64)  ARCH=x86_64 ;;
  *) fail "Unsupported architecture: $ARCH" ;;
esac

case "$OS" in
  darwin) ;;
  linux)  fail "Linux builds are not yet published. Build from source: cargo install --git https://github.com/${REPO} bunyan-cli" ;;
  *) fail "Unsupported OS: $OS" ;;
esac

ASSET="${BINARY}-${OS}-${ARCH}"

# Resolve version
VERSION="${BUNYAN_VERSION:-}"
if [[ -z "$VERSION" ]]; then
  info "Fetching latest release tag"
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep -m1 '"tag_name"' \
    | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
  [[ -n "$VERSION" ]] || fail "Could not determine latest version"
fi

URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"

# Pick install dir
if [[ -n "${BUNYAN_INSTALL_DIR:-}" ]]; then
  INSTALL_DIR="$BUNYAN_INSTALL_DIR"
elif [[ -n "${XDG_BIN_DIR:-}" ]]; then
  INSTALL_DIR="$XDG_BIN_DIR"
elif [[ -d "$HOME/.local/bin" ]]; then
  INSTALL_DIR="$HOME/.local/bin"
else
  INSTALL_DIR="$HOME/.bunyan/bin"
fi
mkdir -p "$INSTALL_DIR"

# Download
info "Downloading ${ASSET} ${VERSION}"
TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT
curl -fSL --progress-bar "$URL" -o "$TMP" \
  || fail "Download failed: $URL"

install -m 0755 "$TMP" "$INSTALL_DIR/${BINARY}"
ok "Installed ${BINARY} ${VERSION} to ${INSTALL_DIR}/${BINARY}"

# PATH check
case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *) color 33 "${INSTALL_DIR} is not on your PATH. Add this to your shell rc:"
     echo "    export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
esac

# Verify
if command -v "$BINARY" >/dev/null 2>&1 && [[ "$(command -v "$BINARY")" == "${INSTALL_DIR}/${BINARY}" ]]; then
  "$INSTALL_DIR/${BINARY}" --help >/dev/null 2>&1 && ok "Run 'bunyan --help' to get started"
fi
