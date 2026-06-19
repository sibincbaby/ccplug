#!/usr/bin/env bash
set -euo pipefail

# Build ccplug from source and install it. No published release yet.
BIN="ccplug"
DEST="${DEST:-$HOME/.local/bin}"

cd "$(dirname "$0")"
echo "Building ${BIN} (release)..."
cargo build --release

mkdir -p "$DEST"
install -m 0755 "target/release/${BIN}" "${DEST}/${BIN}"
echo "Installed ${BIN} to ${DEST}/${BIN}"
case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "note: ${DEST} is not on your PATH" ;;
esac
