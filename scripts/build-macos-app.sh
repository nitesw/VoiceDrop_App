#!/bin/sh
# Builds a real VoiceDrop.app bundle instead of a bare `swift run` binary.
#
# Why this exists: an unbundled SwiftPM executable has no Info.plist and no
# stable code-signing identity, so macOS attributes TCC permission prompts
# (microphone, Accessibility/Input Monitoring) to the launching shell instead
# of to VoiceDrop, and an ad-hoc signature can be invalidated on every
# rebuild. Bundling + signing with a consistent identifier fixes both.
set -eu

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MACOS_DIR="$REPO_ROOT/macos"
CONFIG="${1:-debug}"

APP_NAME="VoiceDrop"
BUNDLE_ID="com.voicedrop.app"
APP_DIR="$MACOS_DIR/.build/$APP_NAME.app"

echo "==> Building voicedrop-core (release)"
(cd "$REPO_ROOT" && cargo build --release)

echo "==> Building Swift executable ($CONFIG)"
(cd "$MACOS_DIR" && swift build -c "$CONFIG")

BIN_PATH="$MACOS_DIR/.build/$CONFIG/$APP_NAME"
if [ ! -f "$BIN_PATH" ]; then
    echo "error: expected binary not found at $BIN_PATH" >&2
    exit 1
fi

echo "==> Assembling $APP_DIR"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$BIN_PATH" "$APP_DIR/Contents/MacOS/$APP_NAME"
cp "$MACOS_DIR/Info.plist" "$APP_DIR/Contents/Info.plist"
cp "$MACOS_DIR/AppIcon.icns" "$APP_DIR/Contents/Resources/AppIcon.icns"
cp "$MACOS_DIR/Resources/"MenuBarIcon*.png "$APP_DIR/Contents/Resources/"

# A fixed certificate keeps the signature's identity stable across rebuilds,
# so macOS keeps the Accessibility permission (see scripts/setup-signing.sh).
# Ad-hoc is the fallback: it works, but the permission is lost on every
# rebuild.
SIGNING_IDENTITY="VoiceDrop Local Signing"
if security find-certificate -c "$SIGNING_IDENTITY" >/dev/null 2>&1; then
    echo "==> Code-signing with \"$SIGNING_IDENTITY\" (identifier: $BUNDLE_ID)"
    codesign --force --deep --sign "$SIGNING_IDENTITY" --identifier "$BUNDLE_ID" "$APP_DIR"
else
    echo "==> Code-signing ad-hoc (identifier: $BUNDLE_ID)"
    echo "    warning: run ./scripts/setup-signing.sh once, or macOS will forget"
    echo "    the Accessibility permission after every rebuild."
    codesign --force --deep --sign - --identifier "$BUNDLE_ID" "$APP_DIR"
fi

echo "==> Done: $APP_DIR"
echo "Run with: open \"$APP_DIR\"  (or ./scripts/install.sh to install it to /Applications)"
