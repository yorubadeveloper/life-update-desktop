#!/usr/bin/env bash
# Compiles the Swift bridge to Apple Intelligence (FoundationModels) and
# stages it as a Tauri resource. Requires Xcode Command Line Tools with the
# macOS 26 SDK (swiftc targeting arm64-apple-macosx26+).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$SCRIPT_DIR/../swift/LifeUpdateAI.swift"
DEST_DIR="$SCRIPT_DIR/../src-tauri/resources/ai-helper"

mkdir -p "$DEST_DIR"
swiftc -O -parse-as-library "$SRC" -o "$DEST_DIR/life-update-ai"
codesign --force --sign - "$DEST_DIR/life-update-ai" 2>/dev/null || true

echo "Built AI helper at $DEST_DIR/life-update-ai"
