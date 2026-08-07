#!/bin/sh
# Regenerates macos/AppIcon.icns from the Icon Composer export.
#
# Why the flatten step: Icon Composer's exported PNG has real transparency
# outside the rounded-square shape (correct for a squircle icon on its
# own), but macOS's small icon "chip" containers (e.g. the Screen
# Recording/Accessibility permission lists in System Settings) put their
# own background behind every app's icon — so our transparent corners let
# that system background show through, reading as a second nested square
# around our icon. Apps whose icons fill their full canvas edge-to-edge
# with no transparency don't show this. Flattening onto an opaque square
# matching our own background color (sampled from the design) fixes it:
# zero transparency left for anything to peek through.
set -eu

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$REPO_ROOT/assets/VoiceDrop Exports/VoiceDrop-iOS-Default-1024@1x.png"
ICONSET="$REPO_ROOT/macos/AppIcon.iconset"
FLATTENED="$REPO_ROOT/macos/.build/AppIcon-flattened.png"

if [ ! -f "$SRC" ]; then
    echo "error: expected export not found at $SRC" >&2
    echo "Re-export the 'Default' 1024x1024 variant from Icon Composer first." >&2
    exit 1
fi

mkdir -p "$(dirname "$FLATTENED")"

echo "==> Flattening onto an opaque background (removing corner transparency)"
cat > /tmp/voicedrop-flatten-icon.swift << 'EOF'
import AppKit
let inputPath = CommandLine.arguments[1]
let outputPath = CommandLine.arguments[2]
guard let img = NSImage(contentsOfFile: inputPath) else { exit(1) }
let size = img.size
let flattened = NSImage(size: size)
flattened.lockFocus()
// Sampled from the design's own squircle fill color — keep in sync if the
// logo's background color changes.
NSColor(calibratedRed: 0.0509804, green: 0.0470588, blue: 0.0509804, alpha: 1.0).setFill()
NSRect(origin: .zero, size: size).fill()
img.draw(in: NSRect(origin: .zero, size: size))
flattened.unlockFocus()
guard let tiff = flattened.tiffRepresentation,
      let rep = NSBitmapImageRep(data: tiff),
      let pngData = rep.representation(using: .png, properties: [:]) else { exit(1) }
try! pngData.write(to: URL(fileURLWithPath: outputPath))
EOF
swift /tmp/voicedrop-flatten-icon.swift "$SRC" "$FLATTENED"
rm -f /tmp/voicedrop-flatten-icon.swift

echo "==> Building iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
sips -z 16 16     "$FLATTENED" --out "$ICONSET/icon_16x16.png" >/dev/null
sips -z 32 32     "$FLATTENED" --out "$ICONSET/icon_16x16@2x.png" >/dev/null
sips -z 32 32     "$FLATTENED" --out "$ICONSET/icon_32x32.png" >/dev/null
sips -z 64 64     "$FLATTENED" --out "$ICONSET/icon_32x32@2x.png" >/dev/null
sips -z 128 128   "$FLATTENED" --out "$ICONSET/icon_128x128.png" >/dev/null
sips -z 256 256   "$FLATTENED" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
sips -z 256 256   "$FLATTENED" --out "$ICONSET/icon_256x256.png" >/dev/null
sips -z 512 512   "$FLATTENED" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
sips -z 512 512   "$FLATTENED" --out "$ICONSET/icon_512x512.png" >/dev/null
sips -z 1024 1024 "$FLATTENED" --out "$ICONSET/icon_512x512@2x.png" >/dev/null

echo "==> Building AppIcon.icns"
iconutil -c icns "$ICONSET" -o "$REPO_ROOT/macos/AppIcon.icns"
rm -rf "$ICONSET" "$FLATTENED"

echo "==> Done: $REPO_ROOT/macos/AppIcon.icns"
echo "Run ./scripts/build-macos-app.sh to bundle it into the app."
