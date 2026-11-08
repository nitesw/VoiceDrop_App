#!/bin/sh
# Builds a release VoiceDrop.app, replaces /Applications/VoiceDrop.app with
# it, and launches it. This is the one copy to actually use day to day:
# Launch at Login and the Accessibility permission both attach to it.
set -eu

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILT="$REPO_ROOT/macos/.build/VoiceDrop.app"
INSTALLED="/Applications/VoiceDrop.app"

"$REPO_ROOT/scripts/build-macos-app.sh" release

echo "==> Installing to $INSTALLED"
# Quit the running copy first so the new one can take over the hotkeys.
osascript -e 'quit app "VoiceDrop"' >/dev/null 2>&1 || true
sleep 1
rm -rf "$INSTALLED"
ditto "$BUILT" "$INSTALLED"

echo "==> Launching"
open "$INSTALLED"
echo "Done. Hold ⌥Space to dictate, tap ⌥⇧Space to switch language."
