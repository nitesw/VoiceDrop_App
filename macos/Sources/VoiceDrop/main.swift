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

        // Phase 2 manual language-quality testing aid only (see
        // 0003-phase2-stt.md) — Phase 5's Settings Window is the real,
        // permanent way to set this. VOICEDROP_LANGUAGE takes an ISO 639-1
        // code (e.g. "fr", "uk", "pl"); unset means auto-detect.
        if let lang = ProcessInfo.processInfo.environment["VOICEDROP_LANGUAGE"] {
            let status = lang.withCString { voicedrop_engine_set_language(engine, $0) }
            voiceDropLog.log(
                "Language set to \(lang, privacy: .public) (status \(status, privacy: .public)).")
        }

        // Phase 3 manual Cleanup Pass verification aid only (see
        // 0004-phase3-cleanup-pass.md) — same rationale as VOICEDROP_LANGUAGE
        // above. VOICEDROP_CLEANUP_PROVIDER is "none" (default)/"local"/"cloud".
        // "local" is a self-contained in-process model (VOICEDROP_LOCAL_MODEL_PATH
        // optional, defaults to the engine's built-in path — run
        // scripts/download-cleanup-model.sh first). "cloud" needs
        // VOICEDROP_CLOUD_BASE_URL, _API_KEY, _MODEL — also how to bring your
        // own model via Ollama or another local runner (point base_url at
        // its address).
        if let providerName = ProcessInfo.processInfo.environment["VOICEDROP_CLEANUP_PROVIDER"] {
            let env = ProcessInfo.processInfo.environment
            let kind: Int32?
            switch providerName {
            case "none": kind = Int32(VOICEDROP_CLEANUP_NONE)
            case "local": kind = Int32(VOICEDROP_CLEANUP_LOCAL)
            case "cloud": kind = Int32(VOICEDROP_CLEANUP_CLOUD)
            default:
                voiceDropLog.log("Unknown VOICEDROP_CLEANUP_PROVIDER: \(providerName, privacy: .public)")
                kind = nil
            }
            if let kind {
                let status = voicedrop_engine_set_cleanup_provider(engine, kind)
                voiceDropLog.log(
                    "Cleanup provider set to \(providerName, privacy: .public) (status \(status, privacy: .public)).")

                if providerName == "local", let path = env["VOICEDROP_LOCAL_MODEL_PATH"] {
                    let localStatus = path.withCString {
                        voicedrop_engine_set_cleanup_local_model_path(engine, $0)
                    }
                    voiceDropLog.log(
                        "Local cleanup model path set to \(path, privacy: .public) (status \(localStatus, privacy: .public)).")
                } else if providerName == "cloud" {
                    let baseURL = env["VOICEDROP_CLOUD_BASE_URL"] ?? ""
                    let apiKey = env["VOICEDROP_CLOUD_API_KEY"] ?? ""
                    let model = env["VOICEDROP_CLOUD_MODEL"] ?? ""
                    let cloudStatus = baseURL.withCString { urlPtr in
                        apiKey.withCString { keyPtr in
                            model.withCString { modelPtr in
                                voicedrop_engine_set_cleanup_cloud_config(
                                    engine, urlPtr, keyPtr, modelPtr)
                            }
                        }
                    }
                    voiceDropLog.log("Cloud cleanup config set (status \(cloudStatus, privacy: .public)).")
                }
            }
        }

        // Phase 3 manual Cleanup Strength testing aid only. VOICEDROP_CLEANUP_STRENGTH
        // is "verbatim" (default)/"light"/"formal".
        if let strengthName = ProcessInfo.processInfo.environment["VOICEDROP_CLEANUP_STRENGTH"] {
            let strength: Int32?
            switch strengthName {
            case "verbatim": strength = Int32(VOICEDROP_STRENGTH_VERBATIM_CLEAN)
            case "light": strength = Int32(VOICEDROP_STRENGTH_LIGHT_EDIT)
            case "formal": strength = Int32(VOICEDROP_STRENGTH_FORMAL_REWRITE)
            default:
                voiceDropLog.log("Unknown VOICEDROP_CLEANUP_STRENGTH: \(strengthName, privacy: .public)")
                strength = nil
            }
            if let strength {
                let status = voicedrop_engine_set_cleanup_strength(engine, strength)
                voiceDropLog.log(
                    "Cleanup strength set to \(strengthName, privacy: .public) (status \(status, privacy: .public)).")
            }
        }

        // Phase 3 manual blocklist verification aid only. Comma-separated
        // custom words, merged with the built-in default list.
        if let words = ProcessInfo.processInfo.environment["VOICEDROP_BLOCKLIST"] {
            let status = words.withCString { voicedrop_engine_set_blocklist(engine, $0) }
            voiceDropLog.log(
                "Blocklist words set to \(words, privacy: .public) (status \(status, privacy: .public)).")
        }

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
