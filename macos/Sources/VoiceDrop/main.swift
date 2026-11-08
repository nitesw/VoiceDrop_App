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
    private var menuBar: MenuBarController?
    private let hud = DictationHUDController()

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

        let monitor = HotkeyMonitor(engine: engine, hud: hud)
        hotkeyMonitor = monitor

        let menuBar = MenuBarController()
        menuBar.onToggleEnabled = { [weak monitor] enabled in
            monitor?.setEnabled(enabled)
            voiceDropLog.log("Hotkey \(enabled ? "enabled" : "disabled", privacy: .public) via Menu Bar Icon.")
        }
        self.menuBar = menuBar

        provisionModelThenArm(monitor: monitor)
    }

    /// First-run model provisioning (see ModelProvisioner.swift): the
    /// hotkey must not be armed until the Whisper model is actually on
    /// disk, since STT has nothing to run without it. Retries on failure
    /// with backoff rather than giving up after one attempt — a flaky
    /// connection on first launch shouldn't permanently strand the user.
    private func provisionModelThenArm(monitor: HotkeyMonitor, attempt: Int = 1) {
        let maxAttempts = 5
        hud.show(.downloadingModel(progress: 0))

        ModelProvisioner.ensureWhisperModel(
            onProgress: { [weak self] progress in
                self?.hud.show(.downloadingModel(progress: progress))
            },
            completion: { [weak self] success in
                guard let self else { return }
                if success {
                    self.hud.hide()
                    if !monitor.start() {
                        voiceDropLog.log(
                            "Grant Accessibility/Input Monitoring permission in System Settings, then relaunch."
                        )
                    }
                } else if attempt < maxAttempts {
                    let delay = Double(attempt) * 5
                    voiceDropLog.log(
                        "Whisper model download failed — retrying in \(delay, privacy: .public)s (attempt \(attempt + 1, privacy: .public)/\(maxAttempts, privacy: .public))."
                    )
                    DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
                        self.provisionModelThenArm(monitor: monitor, attempt: attempt + 1)
                    }
                } else {
                    voiceDropLog.log(
                        "Whisper model download failed after \(maxAttempts, privacy: .public) attempts. Check your network connection and relaunch VoiceDrop."
                    )
                    self.hud.show(.error(message: "Download failed — check your connection and relaunch"))
                }
            }
        )
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
