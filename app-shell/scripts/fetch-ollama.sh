#!/usr/bin/env bash
# Downloads the official Ollama macOS release and extracts it into
# src-tauri/ollama-runtime/ as a bundled resource directory.
#
# This is NOT a single-file binary - the release tarball ships the `ollama`
# CLI/server alongside llama-server and a set of GGML/MLX shared libraries
# it loads at runtime, so the whole directory has to travel together
# (see agent.rs / ollama_process.rs, which resolve the `ollama` binary
# inside this directory rather than treating it as a Tauri externalBin).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_TAURI_DIR="$SCRIPT_DIR/../src-tauri"
DEST_DIR="$SRC_TAURI_DIR/ollama-runtime"
VERSION="${OLLAMA_VERSION:-latest}"

if [ "$VERSION" = "latest" ]; then
  URL="https://github.com/ollama/ollama/releases/latest/download/ollama-darwin.tgz"
else
  URL="https://github.com/ollama/ollama/releases/download/${VERSION}/ollama-darwin.tgz"
fi

TMP_TGZ="$(mktemp -t ollama-darwin.XXXXXX.tgz)"
trap 'rm -f "$TMP_TGZ"' EXIT

echo "Downloading Ollama macOS runtime from $URL ..."
curl -sL -o "$TMP_TGZ" "$URL"

rm -rf "$DEST_DIR"
mkdir -p "$DEST_DIR"
tar -xzf "$TMP_TGZ" -C "$DEST_DIR"
chmod +x "$DEST_DIR/ollama"

curl -sL -o "$DEST_DIR/LICENSE" "https://raw.githubusercontent.com/ollama/ollama/main/LICENSE"

echo "Done. Ollama runtime extracted to $DEST_DIR ($(du -sh "$DEST_DIR" | cut -f1))"
