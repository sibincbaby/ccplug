#!/usr/bin/env bash
set -euo pipefail

REPO="sibincbaby/ccplug"
BIN="ccplug"
DEST="${DEST:-/usr/local/bin}"

tag=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep -oP '"tag_name":\s*"\K[^"]+')
url="https://github.com/${REPO}/releases/download/${tag}/${BIN}-${tag}-x86_64-linux-musl.tar.gz"

tmp=$(mktemp -d)
echo "Downloading ${BIN} ${tag}..."
curl -fsSL "$url" | tar xz -C "$tmp"
install -m 0755 "$tmp/${BIN}" "${DEST}/${BIN}"
rm -rf "$tmp"
echo "Installed ${BIN} to ${DEST}/${BIN}"
