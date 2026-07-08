#!/usr/bin/env bash
# Stages the two bundled resource directories inside src-tauri/resources/
# so tauri.conf.json's `bundle.resources` (which can't reach outside the
# src-tauri project) has something local to point at. Run after
# agent/build.sh and scripts/fetch-ollama.sh.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_TAURI="$SCRIPT_DIR/../src-tauri"
AGENT_DIST="$SCRIPT_DIR/../../agent/dist/life-update-agent"
OLLAMA_RUNTIME="$SRC_TAURI/ollama-runtime"

if [ ! -d "$AGENT_DIST" ]; then
  echo "error: $AGENT_DIST not found - run agent/build.sh first" >&2
  exit 1
fi
if [ ! -d "$OLLAMA_RUNTIME" ]; then
  echo "error: $OLLAMA_RUNTIME not found - run scripts/fetch-ollama.sh first" >&2
  exit 1
fi

mkdir -p "$SRC_TAURI/resources"
rm -rf "$SRC_TAURI/resources/life-update-agent" "$SRC_TAURI/resources/ollama-runtime"
cp -R "$AGENT_DIST" "$SRC_TAURI/resources/life-update-agent"
cp -R "$OLLAMA_RUNTIME" "$SRC_TAURI/resources/ollama-runtime"

echo "Staged resources at $SRC_TAURI/resources/"
