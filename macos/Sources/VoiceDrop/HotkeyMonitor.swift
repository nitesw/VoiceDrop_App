import ApplicationServices
import CoreGraphics
import CVoiceDropCore
import Foundation
import os

/// macOS virtual keycode for the D key (kVK_ANSI_D from Carbon's HIToolbox,
/// reproduced here to avoid pulling in the Carbon framework for one constant).
private let kVK_ANSI_D: Int64 = 0x02

/// Phase 1 push-to-talk trigger: hold Control+Option+D to record, release to
/// stop.
///
/// F5 was tried first and rejected: the F-row sends a "system-defined" media
/// key event by default (not a standard keyDown/keyUp with a keycode) unless
/// "Use F1, F2, etc. as standard function keys" is enabled in System
/// Settings → Keyboard — which is also why bare F5 triggered Siri/Dictation
/// instead of reaching this tap at all. A plain letter key sidesteps that
/// quirk entirely. Revisit the exact combo (and whether to support F-row
/// keys, which would need detecting/instructing on that system setting) in
/// Phase 5's rebinding UI.
final class HotkeyMonitor {
    private var eventTap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?
    private let engine: OpaquePointer
    private let hud: DictationHUDController
    /// True from a matched (Control+Option held) D key-down until its
    /// corresponding key-up. Lets the key-up handler swallow only the release
    /// that belongs to our hotkey — a plain, unmodified "D" keystroke (e.g.
    /// normal typing elsewhere) must never be swallowed.
    private var hotkeyIsDown = false
    /// Toggled from the Menu Bar Icon's Enable/Disable item via `setEnabled`.
    /// `handle` also checks this directly as a fast early-exit, but the real
    /// suspend/resume work — actually disabling the tap and freeing loaded
    /// models — happens in `setEnabled`.
    private(set) var isEnabled = true
    private var levelTimer: Timer?
    /// When the tap was last disabled — see `handleTapDisabled`. Rate-based
    /// rather than a fixed attempt count: observed behavior after revoking
    /// Accessibility while running is repeated `.tapDisabledByTimeout`
    /// events roughly 9 seconds apart, each one apparently costing that
    /// much system-wide keystroke lag (each re-arm reinstates a tap that's
    /// now slow to process every event, presumably because macOS is
    /// re-validating trust per-event against a revoked-but-still-registered
    /// client). Two disables close together in time means it's spinning —
    /// tear down after the *second* one rather than waiting for an absolute
    /// count, so a rare, isolated timeout months apart (a genuine one-off
    /// slow callback) still recovers fine, but an active spin gets cut off
    /// after roughly one extra ~9s cycle instead of running for several.
    private var lastTapDisableTime: Date?
    private static let tapDisableSpinWindow: TimeInterval = 15
    /// Polls trust status independent of the reactive tap-disable path
    /// above, so revocation is caught proactively instead of only after
    /// the tap has already been killed and re-armed a few times (each
    /// round of which freezes system-wide keystroke delivery for a bit).
    private var trustPollTimer: Timer?
    /// What had focus when the current session started — captured at
    /// key-down, used at key-up once the Cleaned Transcript is ready. Kept
    /// as a property (rather than threaded through the async chain) since
    /// there's only ever one session in flight at a time.
    private var injectionTarget: InjectionTarget?

    init(engine: OpaquePointer, hud: DictationHUDController) {
        self.engine = engine
        self.hud = hud
    }

    /// Returns false if Accessibility/Input Monitoring permission isn't
    /// granted yet — full onboarding UX is Phase 6, this just detects it.
    func start() -> Bool {
        let options: NSDictionary = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true]
        guard AXIsProcessTrustedWithOptions(options) else {
            voiceDropLog.log("Accessibility/Input Monitoring permission not granted yet.")
            return false
        }

        let eventMask = (1 << CGEventType.keyDown.rawValue) | (1 << CGEventType.keyUp.rawValue)
        let selfPtr = Unmanaged.passUnretained(self).toOpaque()

        guard
            let tap = CGEvent.tapCreate(
                tap: .cgSessionEventTap,
                place: .headInsertEventTap,
                // .defaultTap (not .listenOnly) so we can swallow our hotkey's
                // keystrokes — .listenOnly only observes, it can't suppress,
                // which is why the D key was leaking through to whatever app
                // had focus (heard as repeated beeps/noise with no text field
                // to absorb it).
                options: .defaultTap,
                eventsOfInterest: CGEventMask(eventMask),
                callback: { _, type, event, refcon in
                    guard let refcon else { return Unmanaged.passUnretained(event) }
                    let monitor = Unmanaged<HotkeyMonitor>.fromOpaque(refcon).takeUnretainedValue()

                    // macOS disables a tap that doesn't return from its callback
                    // quickly (watchdog), or via .tapDisabledByUserInput. Once
                    // disabled, every keystroke — including ours — passes
                    // through unswallowed, which is exactly the "hotkey leaks
                    // through as raw input" symptom. Re-enable immediately;
                    // this is Apple's documented recovery path. The real fix is
                    // keeping the callback itself fast (see handleKeyUp), this
                    // is just the safety net.
                    if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
                        // Diagnostic: per Apple's docs, .tapDisabledByUserInput is
                        // documented for things like the force-quit dialog / secure
                        // input, not Accessibility revocation specifically — logging
                        // which one actually fires here to check that assumption
                        // rather than keep guessing at the cause of reported lag.
                        let typeName = type == .tapDisabledByTimeout ? "timeout" : "userInput"
                        voiceDropLog.log("Event tap disabled (type=\(typeName, privacy: .public)).")
                        monitor.handleTapDisabled(isTimeout: type == .tapDisabledByTimeout)
                        return Unmanaged.passUnretained(event)
                    }

                    if monitor.handle(type: type, event: event) {
                        return nil  // consumed: don't let it reach the focused app
                    }
                    return Unmanaged.passUnretained(event)
                },
                userInfo: selfPtr
            )
        else {
            voiceDropLog.log("Failed to create event tap.")
            return false
        }

        eventTap = tap
        let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
        runLoopSource = source
        CFRunLoopAddSource(CFRunLoopGetCurrent(), source, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)
        voiceDropLog.log("Control+Option+D hotkey armed. Hold to record, release to stop.")

        trustPollTimer?.invalidate()
        trustPollTimer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            guard let self, self.eventTap != nil, !AXIsProcessTrusted() else { return }
            voiceDropLog.log(
                "Accessibility trust revoked (caught by polling, not a tap-disable event) — tearing down. Re-grant Accessibility and relaunch."
            )
            self.teardownTap()
        }
        return true
    }

    /// Recovery path for a tap the system disabled — but *only* when
    /// re-arming can actually succeed.
    ///
    /// This tap is a `.defaultTap` at `.headInsertEventTap`, meaning every
    /// keystroke system-wide blocks on our callback for a verdict. If
    /// Accessibility trust is revoked while we're running (user removes
    /// VoiceDrop in Privacy & Security), the system kills the tap and will
    /// kill it again immediately after any re-enable. Unconditionally
    /// re-enabling therefore spins: re-arm, killed, callback, re-arm — while
    /// WindowServer eats a timeout on every key event, which is what makes
    /// the whole Mac lag with VoiceDrop still "running" in the background.
    ///
    /// So: never re-enable without trust. And never re-enable at all on a
    /// `.tapDisabledByTimeout` while trust is anything but certain — our
    /// callback is trivially fast and has run for hours without ever
    /// timing out under normal conditions, so a timeout occurring right
    /// after an Accessibility revocation isn't a one-off slow callback,
    /// it's every keystroke hanging inside the tap before it ever reaches
    /// our code (observed: repeated ~9-12s gaps between timeouts, each
    /// presumably one hung keystroke). Re-arming just buys another full
    /// hang cycle for free. `.tapDisabledByUserInput` doesn't have that
    /// same evidence behind it yet, so it still gets one retry via the
    /// rate-limited path below rather than an immediate teardown.
    private func handleTapDisabled(isTimeout: Bool) {
        guard AXIsProcessTrusted() else {
            voiceDropLog.log(
                "Event tap disabled and Accessibility trust is gone — tearing down instead of re-arming. Re-grant Accessibility and relaunch."
            )
            teardownTap()
            return
        }

        if isTimeout {
            voiceDropLog.log(
                "Event tap disabled by timeout — tearing down immediately rather than re-arming into another hang cycle."
            )
            teardownTap()
            return
        }

        let now = Date()
        if let lastTapDisableTime, now.timeIntervalSince(lastTapDisableTime) < Self.tapDisableSpinWindow {
            voiceDropLog.log(
                "Event tap disabled again within \(Self.tapDisableSpinWindow, privacy: .public)s of the last one — tearing down rather than spinning further."
            )
            teardownTap()
            return
        }
        lastTapDisableTime = now

        guard let tap = eventTap else { return }
        voiceDropLog.log("Event tap disabled — re-arming.")
        CGEvent.tapEnable(tap: tap, enable: true)
    }

    /// Real suspend/resume for the Menu Bar Icon's Enable/Disable toggle —
    /// not just gating `handle()`. Disabling actually stops the `CGEventTap`
    /// from intercepting any events (rather than leaving it running and
    /// just ignoring what it sees), and drops any cached Whisper/Cleanup
    /// Pass models so they stop occupying memory while the app sits idle.
    ///
    /// If a session is already in flight (Recording/Processing) when
    /// disabled, its models stay loaded until that session finishes — the
    /// Rust core's state machine only allows `Reset` from a terminal state
    /// (Done/Discarded/Error/NoSpeech), so there's no abort path mid-session
    /// without a larger change there. Not silent: logged either way.
    func setEnabled(_ enabled: Bool) {
        guard isEnabled != enabled else { return }
        isEnabled = enabled

        guard let tap = eventTap else { return }
        CGEvent.tapEnable(tap: tap, enable: enabled)

        guard !enabled else { return }
        if voicedrop_engine_state(engine) == VOICEDROP_STATE_IDLE {
            voicedrop_engine_unload_models(engine)
            voiceDropLog.log("Disabled — event tap stopped and cached models unloaded.")
        } else {
            voiceDropLog.log(
                "Disabled — event tap stopped, but a session is in flight, so its models stay loaded until it finishes."
            )
        }
    }

    /// Fully releases the tap: an un-re-armable tap must not stay installed,
    /// since every system-wide keystroke would keep paying its timeout.
    private func teardownTap() {
        if let tap = eventTap {
            CGEvent.tapEnable(tap: tap, enable: false)
            CFMachPortInvalidate(tap)
        }
        if let source = runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetCurrent(), source, .commonModes)
        }
        eventTap = nil
        runLoopSource = nil
        trustPollTimer?.invalidate()
        trustPollTimer = nil
    }

    /// Returns true if this event was our hotkey and should be swallowed
    /// (not passed through to whatever app has focus).
    private func handle(type: CGEventType, event: CGEvent) -> Bool {
        guard isEnabled else { return false }

        let keyCode = event.getIntegerValueField(.keyboardEventKeycode)
        guard keyCode == kVK_ANSI_D else { return false }

        switch type {
        case .keyDown:
            // Require Control+Option held down at the moment D goes down. If
            // the modifiers aren't held, this is just a plain "D" keystroke —
            // don't swallow it, and don't arm hotkeyIsDown.
            let flags = event.flags
            guard flags.contains(.maskControl), flags.contains(.maskAlternate) else { return false }
            hotkeyIsDown = true
            handleKeyDown()
            return true
        case .keyUp:
            // Only swallow the release if it matches a key-down we actually
            // armed — a plain "D" release (no modifiers held at press time)
            // must pass through untouched.
            guard hotkeyIsDown else { return false }
            hotkeyIsDown = false
            handleKeyUp()
            return true
        default:
            return false
        }
    }

    private func handleKeyDown() {
        // Guard against key-repeat re-triggering while already recording —
        // only attempt the transition from Idle.
        guard voicedrop_engine_state(engine) == VOICEDROP_STATE_IDLE else { return }

        // Capture the Injection Target now — fixed for the session even if
        // focus later changes (see TextInjector's doc).
        injectionTarget = TextInjector.captureCurrentTarget()

        let status = voicedrop_engine_start_recording(engine)
        if status == VOICEDROP_OK {
            voiceDropLog.log("Recording started.")
            startLevelPolling()
        } else {
            voiceDropLog.log("Failed to start recording (status \(status, privacy: .public)).")
        }
    }

    /// Drives the HUD's live waveform while recording. 30Hz is smooth
    /// enough for a small bar meter without being wasteful; each tick is
    /// just a `try_lock` + RMS over the last ~2048 samples on the Rust
    /// side (see `AudioCapture::current_level`), cheap enough to poll from
    /// the main thread's run loop.
    private func startLevelPolling() {
        levelTimer?.invalidate()
        let engine = self.engine
        let hud = self.hud
        levelTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 30.0, repeats: true) { _ in
            let level = voicedrop_engine_current_input_level(engine)
            hud.show(.recording(level: level))
        }
    }

    private func stopLevelPolling() {
        levelTimer?.invalidate()
        levelTimer = nil
    }

    private func handleKeyUp() {
        // Only this state check runs on the tap's callback thread — it's
        // cheap. Everything else (stopping capture, running Whisper, which
        // can take several seconds) is dispatched off this thread. A
        // CGEventTap callback that blocks gets disabled by the system
        // watchdog, which is what silently broke the hotkey after one
        // successful recording: voicedrop_engine_stop_recording used to run
        // Whisper synchronously right here.
        guard voicedrop_engine_state(engine) == VOICEDROP_STATE_RECORDING else { return }

        stopLevelPolling()
        let target = injectionTarget
        hud.show(.processing)

        let engine = self.engine
        let hud = self.hud
        DispatchQueue.global(qos: .userInitiated).async {
            let status = voicedrop_engine_stop_recording(engine)
            if status == VOICEDROP_NO_SPEECH {
                voiceDropLog.log("No speech detected — audio was silent or too short to transcribe.")
                _ = voicedrop_engine_reset(engine)
                DispatchQueue.main.async { hud.show(.noSpeech, autoDismissAfter: 1.5) }
                return
            }
            guard status == VOICEDROP_OK else {
                voiceDropLog.log("Recording stopped with error (status \(status, privacy: .public)).")
                _ = voicedrop_engine_reset(engine)
                DispatchQueue.main.async {
                    hud.show(.error(message: "Something went wrong"), autoDismissAfter: 2.0)
                }
                return
            }

            let wavPath = NSTemporaryDirectory() + "voicedrop_verification.wav"
            let wavStatus = wavPath.withCString { voicedrop_engine_write_verification_wav(engine, $0) }
            if wavStatus == VOICEDROP_OK {
                voiceDropLog.log(
                    "Recording complete. Verification WAV written to: \(wavPath, privacy: .public)")
            } else {
                voiceDropLog.log(
                    "Recording complete, but WAV write failed (status \(wavStatus, privacy: .public)).")
            }

            if let cString = voicedrop_engine_last_raw_transcript(engine) {
                defer { voicedrop_core_free_string(cString) }
                voiceDropLog.log("Raw Transcript: \(String(cString: cString), privacy: .public)")
            }

            guard let cleanedCString = voicedrop_engine_last_transcript(engine) else {
                voiceDropLog.log("No cleaned transcript available.")
                _ = voicedrop_engine_reset(engine)
                DispatchQueue.main.async { hud.hide() }
                return
            }
            let transcript = String(cString: cleanedCString)
            voicedrop_core_free_string(cleanedCString)
            voiceDropLog.log("Cleaned Transcript: \(transcript, privacy: .public)")

            _ = voicedrop_engine_reset(engine)

            DispatchQueue.main.async {
                let outcome = TextInjector.inject(transcript, into: target ?? TextInjector.captureCurrentTarget())
                switch outcome {
                case .injected:
                    hud.hide()
                case .pasted:
                    voiceDropLog.log("Direct AX insertion unavailable — auto-pasted instead.")
                    hud.hide()
                case .fallbackToClipboard(let reason):
                    voiceDropLog.log(
                        "Injection fallback (\(reason, privacy: .public)) — copied to clipboard instead.")
                    hud.show(.fallback(reason: reason), autoDismissAfter: 2.0)
                case .discarded(let reason):
                    voiceDropLog.log(
                        "Transcript discarded, not injected or copied (\(reason, privacy: .public)).")
                    hud.show(.discarded, autoDismissAfter: 1.5)
                }
            }
        }
    }
}
