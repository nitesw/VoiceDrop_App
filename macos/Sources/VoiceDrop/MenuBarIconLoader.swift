import AppKit

/// Loads the Menu Bar Icon glyph from the two raster sizes bundled directly
/// into Contents/Resources (see `scripts/build-macos-app.sh` — deliberately
/// NOT via `Bundle.module`; see `Package.swift`'s comment for why that
/// path is broken for this app's build setup). Assembles both the @1x and
/// @2x representations into one `NSImage` the way an asset catalog would,
/// so AppKit picks the right one per display, and marks it as a template
/// image so the menu bar auto-adapts it for light/dark and the active/
/// inactive tint.
enum MenuBarIconLoader {
    static func load() -> NSImage? {
        guard let resourcePath = Bundle.main.resourcePath else { return nil }
        guard
            let data1x = FileManager.default.contents(atPath: resourcePath + "/MenuBarIcon.png"),
            let data2x = FileManager.default.contents(atPath: resourcePath + "/MenuBarIcon@2x.png"),
            let rep1x = NSBitmapImageRep(data: data1x),
            let rep2x = NSBitmapImageRep(data: data2x)
        else {
            return nil
        }

        // Both representations share the same logical (point) size — only
        // their backing pixel density differs — which is what lets AppKit
        // choose between them by screen scale factor.
        let logicalSize = NSSize(width: rep1x.pixelsWide, height: rep1x.pixelsHigh)
        rep1x.size = logicalSize
        rep2x.size = logicalSize

        let image = NSImage(size: logicalSize)
        image.addRepresentation(rep1x)
        image.addRepresentation(rep2x)
        image.isTemplate = true
        return image
    }
}
