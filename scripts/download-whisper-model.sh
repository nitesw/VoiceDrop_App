#!/bin/sh
# Downloads a whisper.cpp GGML model to the location voicedrop-core expects
# by default (see docs/adr/0004-whisper-model-download-on-first-run.md).
# Local-dev convenience only; the app's own first-run download flow is a
# separate, not-yet-implemented Swift-side step.
set -eu

SIZE="${1:-small}"
DEST_DIR="$HOME/Library/Application Support/VoiceDrop/models"
DEST="$DEST_DIR/ggml-$SIZE.bin"
URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-$SIZE.bin"

mkdir -p "$DEST_DIR"

if [ -f "$DEST" ]; then
    echo "Already present: $DEST"
    exit 0
fi

echo "==> Downloading ggml-$SIZE.bin"
curl -L --fail --progress-bar -o "$DEST.part" "$URL"
mv "$DEST.part" "$DEST"
echo "==> Saved to $DEST"
