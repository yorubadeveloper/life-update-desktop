#!/usr/bin/env bash
# Freezes the agent into a standalone binary (onedir) at dist/life-update-agent/.
# Bundled by app-shell as a Tauri resource directory - see agent.rs.
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
uv run pyinstaller life-update-agent.spec --noconfirm
echo "Done. Frozen binary at dist/life-update-agent/life-update-agent"
