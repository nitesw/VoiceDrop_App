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
    /// True from a matched (Control+Option held) D key-down until its
    /// corresponding key-up. Lets the key-up handler swallow only the release
    /// that belongs to our hotkey — a plain, unmodified "D" keystroke (e.g.
    /// normal typing elsewhere) must never be swallowed.
    private var hotkeyIsDown = false

    init(engine: OpaquePointer) {
        self.engine = engine
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
                        if let tap = monitor.eventTap {
                            CGEvent.tapEnable(tap: tap, enable: true)
                        }
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
        return true
    }

    /// Returns true if this event was our hotkey and should be swallowed
    /// (not passed through to whatever app has focus).
    private func handle(type: CGEventType, event: CGEvent) -> Bool {
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

        let status = voicedrop_engine_start_recording(engine)
        if status == VOICEDROP_OK {
            voiceDropLog.log("Recording started.")
        } else {
            voiceDropLog.log("Failed to start recording (status \(status, privacy: .public)).")
        }
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

        let engine = self.engine
        DispatchQueue.global(qos: .userInitiated).async {
            let status = voicedrop_engine_stop_recording(engine)
            if status == VOICEDROP_NO_SPEECH {
                voiceDropLog.log("No speech detected — audio was silent or too short to transcribe.")
                _ = voicedrop_engine_reset(engine)
                return
            }
            guard status == VOICEDROP_OK else {
                voiceDropLog.log("Recording stopped with error (status \(status, privacy: .public)).")
                _ = voicedrop_engine_reset(engine)
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

            // Phase 2/3 debug aid only — Phase 4 owns real transcript
            // handling (injection). This just proves STT + Cleanup Pass
            // produced something, via Console.app.
            if let cString = voicedrop_engine_last_raw_transcript(engine) {
                defer { voicedrop_core_free_string(cString) }
                voiceDropLog.log("Raw Transcript: \(String(cString: cString), privacy: .public)")
            }
            if let cString = voicedrop_engine_last_transcript(engine) {
                defer { voicedrop_core_free_string(cString) }
                voiceDropLog.log("Cleaned Transcript: \(String(cString: cString), privacy: .public)")
            } else {
                voiceDropLog.log("No cleaned transcript available.")
            }

            // Phase 3/4 don't exist yet, so reset back to Idle immediately
            // after logging so the hotkey can be pressed again.
            _ = voicedrop_engine_reset(engine)
        }
    }
}
