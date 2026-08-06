#!/bin/sh
# Downloads a GGUF Cleanup Pass model to the location voicedrop-core expects
# by default for the self-contained VOICEDROP_CLEANUP_LOCAL provider (see
# docs/adr/0008-local-cleanup-in-process-again.md and
# docs/todos/0004-phase3-cleanup-pass.md's "Local provider" todo).
# Qwen2.5-1.5B-Instruct: chosen after manual side-by-side comparison against
# Qwen2.5-0.5B (too weak) and Llama-3.2-3B (ignored the VerbatimClean
# strength's "preserve exact wording" instruction even more than the
# smaller models — bigger isn't automatically better here).
# Local-dev convenience only; the app's own first-run/on-selection download
# flow is a separate, not-yet-implemented Swift-side step. Only needed if
# you're testing the local Cleanup Pass provider — VOICEDROP_CLEANUP_NONE
# and VOICEDROP_CLEANUP_CLOUD (including "bring your own via Ollama") never
# need this file.
set -eu

DEST_DIR="$HOME/Library/Application Support/VoiceDrop/models"
DEST="$DEST_DIR/qwen2.5-1.5b-instruct-q4_k_m.gguf"
URL="https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf"

mkdir -p "$DEST_DIR"

if [ -f "$DEST" ]; then
    echo "Already present: $DEST"
    exit 0
fi

echo "==> Downloading qwen2.5-1.5b-instruct-q4_k_m.gguf"
curl -L --fail --progress-bar -o "$DEST.part" "$URL"
mv "$DEST.part" "$DEST"
echo "==> Saved to $DEST"
