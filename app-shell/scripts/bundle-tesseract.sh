#!/usr/bin/env bash
# Makes the local Homebrew-built tesseract binary relocatable (via
# dylibbundler) and stages it as a resource directory, the same way
# fetch-ollama.sh stages the Ollama runtime.
#
# Unlike Ollama, tesseract has no official prebuilt redistributable
# tarball, so this bundles whatever's installed via `brew install
# tesseract` on this machine rather than downloading a release asset.
# Requires: brew install tesseract dylibbundler

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST_DIR="$SCRIPT_DIR/../src-tauri/tesseract-runtime"

TESSERACT_BIN="$(command -v tesseract || true)"
if [ -z "$TESSERACT_BIN" ]; then
  echo "error: tesseract not found on PATH - run 'brew install tesseract' first" >&2
  exit 1
fi
if ! command -v dylibbundler >/dev/null; then
  echo "error: dylibbundler not found - run 'brew install dylibbundler' first" >&2
  exit 1
fi

# Resolve the real Cellar path (tesseract on PATH is usually a symlink) so
# we can find its sibling tessdata directory.
TESSERACT_REAL="$(python3 -c "import os,sys; print(os.path.realpath(sys.argv[1]))" "$TESSERACT_BIN")"
TESSDATA_SRC="$(dirname "$TESSERACT_REAL")/../share/tessdata"
if [ ! -d "$TESSDATA_SRC" ]; then
  echo "error: tessdata not found at $TESSDATA_SRC" >&2
  exit 1
fi

rm -rf "$DEST_DIR"
mkdir -p "$DEST_DIR/tessdata"

cp "$TESSERACT_BIN" "$DEST_DIR/tesseract"
chmod +w "$DEST_DIR/tesseract"

echo "Bundling dependencies with dylibbundler..."
(cd "$DEST_DIR" && dylibbundler -od -b -x ./tesseract -d ./libs -p '@executable_path/libs/')

cp "$TESSDATA_SRC/eng.traineddata" "$DEST_DIR/tessdata/"

echo "Done. Bundled tesseract at $DEST_DIR ($(du -sh "$DEST_DIR" | cut -f1))"
