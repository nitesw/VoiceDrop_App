import AppKit
import AVFoundation
import CVoiceDropCore

/// Calls into the Rust core and returns the result as a Swift String,
/// freeing the C string voicedrop-core allocated for the round trip.
func corePing() -> String {
    guard let cString = voicedrop_core_ping() else {
        return "<voicedrop_core_ping returned null>"
    }
    defer { voicedrop_core_free_string(cString) }
    return String(cString: cString)
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var engine: OpaquePointer?
    private var hotkeyMonitor: HotkeyMonitor?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Menu-bar-only app: no Dock icon, no main window.
        NSApp.setActivationPolicy(.accessory)
        voiceDropLog.log("Rust core says: \(corePing(), privacy: .public)")

        // Request microphone access immediately at launch, rather than
        // waiting for cpal to lazily open the input device on the first
        // recording attempt — the user should see this prompt right away,
        // not several seconds into their first hotkey press.
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            voiceDropLog.log("Microphone access granted: \(granted, privacy: .public)")
        }

        guard let engine = voicedrop_engine_new() else {
            voiceDropLog.log("Failed to create engine.")
            return
        }
        self.engine = engine

        let monitor = HotkeyMonitor(engine: engine)
        hotkeyMonitor = monitor
        if !monitor.start() {
            voiceDropLog.log(
                "Grant Accessibility/Input Monitoring permission in System Settings, then relaunch."
            )
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        if let engine {
            voicedrop_engine_free(engine)
        }
    }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()
