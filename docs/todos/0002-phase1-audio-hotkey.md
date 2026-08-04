# Phase 1 — Rust Core: Audio Capture & Hotkey Foundations

Tracks [GitHub issue #2](https://github.com/nitesw/VoiceDrop_App/issues/2). Depends on [0001-initial-setup.md](0001-initial-setup.md) (working FFI boundary). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md).

Scope: prove that holding the *Push-to-Talk Hotkey* and releasing it reliably captures a raw audio buffer in the Rust core, driven by a real session state machine — no STT or cleanup yet, just audio in, buffer out.

**Blocker inherited from Phase 0**: `macos/` is a bare SwiftPM executable (`swift run` produces an unbundled Mach-O binary, no Info.plist, no stable signing identity). That was fine for a `print()` smoke test, but Phase 1 needs real TCC-gated permissions (Microphone, Input Monitoring/Accessibility), which key off bundle identity and code-signing — an unbundled binary launched from Terminal gets permission prompts attributed to *Terminal*, not VoiceDrop, and an ad-hoc-signed binary can have its grant invalidated on every rebuild. Fix this first (see "App bundling & permissions" below) before writing any hotkey or audio code — everything else in this phase depends on permissions actually sticking to VoiceDrop.

Suggested order: bundling/permissions → hotkey down/up logging only (no Rust yet) → pure Rust state machine (unit-tested, no FFI) → FFI Swift→Rust for transitions → cpal capture gated on `Recording` → FFI Rust→Swift state-change callbacks → buffer format/resampling → edge cases.

## Todos

**App bundling & permissions (do first)** ([#65](https://github.com/nitesw/VoiceDrop_App/issues/65))
- [x] Assemble a real `VoiceDrop.app` bundle (`Contents/MacOS/VoiceDrop`, `Contents/Info.plist`) as part of the build — done via `scripts/build-macos-app.sh` wrapping `cargo build` + `swift build`, rather than moving to a full Xcode project. Revisit if this proves too fragile once the project grows (e.g. once real Xcode-only tooling like asset catalogs is needed).
- [x] `Info.plist` includes `NSMicrophoneUsageDescription` and a stable `CFBundleIdentifier` (`com.voicedrop.app`)
- [x] Code-sign the bundle with a consistent identity (`codesign --force --deep --sign - --identifier com.voicedrop.app`) — confirmed via `codesign -dv` and the unified log showing identity resolved as `application.com.voicedrop.app`, process name `VoiceDrop` (not Terminal). Ad-hoc signing is used for now; if permission grants prove to not survive rebuilds in practice, switch to a self-signed Keychain certificate.
- [x] Verify the microphone-specific prompt: confirmed via unified log — `AVCaptureDevice.requestAccess` fires a `TCCAccessRequest` immediately at launch (not deferred to first recording) and resolves against VoiceDrop's identity, not Terminal's
- [x] Update the README build instructions to build/launch the bundle instead of `swift run`

**Dictation Session state machine** ([#19](https://github.com/nitesw/VoiceDrop_App/issues/19))
- [x] Define the *Dictation Session* states in Rust: `Idle`, `Recording`, `Processing`, `Done`, `Discarded`, `Error` (`core/src/session.rs`)
- [x] Define the valid transitions only (e.g. `Idle → Recording` on hotkey-down, `Recording → Processing` on hotkey-up, `Processing → Done`/`Error`)
- [x] `Discard` events (`Recording`/`Processing` → `Discarded`) are defined and unit-tested directly, even though no real caller reaches them until Phase 6 ("scratch that") or Phase 2 (no-speech) wire up the events that trigger them
- [ ] Expose state-change events across the FFI boundary so the Swift shell can react — **not actually implemented**. What exists today is `voicedrop_engine_state()`, a poll-based getter Swift calls on demand (used to guard against key-repeat in `HotkeyMonitor`) — there is no `voicedrop_engine_set_state_callback` or any push-based notification. Polling is sufficient for Phase 1 (Swift already knows the state at the moments it needs it, driven by its own key-down/key-up events), but a real callback will be needed once the *Dictation HUD* (Phase 4) needs to react to state changes it didn't itself trigger (e.g. `Processing → Done` after STT/Cleanup Pass finish asynchronously)
- [x] Unit tests: illegal transitions are rejected and leave state unchanged (e.g. can't go `Idle → Processing` directly); 7 tests covering happy path, failure path, discard, illegal transitions, and reset guarding

**Audio capture** ([#20](https://github.com/nitesw/VoiceDrop_App/issues/20))
- [x] Add `cpal` to `voicedrop-core` (`core/src/audio.rs`)
- [x] Enumerate and open the default input device; `AudioError::NoInputDevice` returned (and the session rolled back to `Idle`) if none exists
- [x] Stream PCM buffers into an in-memory buffer for the duration of `Recording` state (`f32`/`i16`/`u16` device formats all handled)
- [x] Realtime-thread discipline: the cpal data callback only does a `try_lock` + buffer append, nothing else; no FFI/logging on that thread
- [x] Buffer format: downmixed to mono and resampled to 16 kHz f32 (linear-interpolation resampler — documented as good-enough-for-now, revisit if Phase 2 transcription quality suggests otherwise) before ever reaching the caller
- [x] Mid-recording device disconnection: cpal's error callback sets a flag, `stop()` checks it and returns `AudioError::DeviceDisconnected`; wired to a new `RecordingFailed` state-machine event (`Recording → Error`, unit-tested) — this was a gap in the original transition table, not just an audio-layer concern

**Hotkey capture (macOS)** ([#21](https://github.com/nitesw/VoiceDrop_App/issues/21))
- [x] Global hotkey listener via `CGEventTap` (`macos/Sources/VoiceDrop/HotkeyMonitor.swift`), not Carbon `RegisterEventHotKey`
- [x] Bound key decided: **Control+Option+D** — F5 was tried first and rejected (the F-row sends a "system-defined" media-key event by default, not standard keyDown/keyUp, and bare F5 triggered the system Siri/Dictation shortcut instead of reaching our tap). A plain letter key sidesteps that; Control+Option layers on top purely to avoid colliding with other shortcuts during testing. Scoped to this phase; Phase 5's rebinding UI may revisit both the key and the modifier combo, including whether to support F-row keys at all.
- [x] Tap actively swallows the hotkey's keystrokes (`.defaultTap`, not `.listenOnly`) so Control+Option+D never leaks through to whatever app has focus — `.listenOnly` was tried first and leaked the D keystroke into focused apps with no text field to absorb it (audible as repeated system beeps). A `hotkeyIsDown` flag ensures only the matching key-up is swallowed, so plain unmodified "D" keystrokes elsewhere are unaffected.
- [x] Key-down transitions `Idle → Recording` and starts audio capture (via `voicedrop_engine_start_recording`)
- [x] Key-up transitions `Recording → Processing` and stops capture (via `voicedrop_engine_stop_recording`) — then, since Phase 2/3 don't exist yet, the engine immediately marks processing succeeded (`Processing → Done`) as a stand-in; revisit once real STT/Cleanup Pass wiring lands so this stops short at `Processing`
- [x] Debounce: key-down is ignored unless the engine is currently `Idle` (guards against key-repeat re-triggering)
- [x] Permission-not-granted case: `AXIsProcessTrustedWithOptions` checked (with prompt) before installing the tap; `HotkeyMonitor.start()` returns `false` and logs guidance if not granted
- [x] Verified on a real keyboard/mic: holding Control+Option+D starts recording, releasing stops it and writes the verification WAV, with no leaked keystrokes into other apps

## Done when

- [x] Holding the hotkey anywhere on macOS starts audio capture; releasing it stops and hands a complete PCM buffer to the Rust core — confirmed via Console.app logs (`Recording started.` / `Recording complete. Verification WAV written to: ...`)
- [x] The *Dictation Session* state machine enforces valid transitions and rejects invalid ones (covered by unit tests — 16 tests passing across `session`/`audio`/FFI round-trip)
- [x] A brief accidental tap doesn't crash the app (confirmed via repeated real testing). Losing the input device mid-recording is unit-tested at the state-machine level (`RecordingFailed` → `Error`) but **not yet exercised against a real device unplug** — deferred to manual QA in [0011-phase10-testing-qa.md](0011-phase10-testing-qa.md) rather than blocking this phase; add it there explicitly.
- [x] **Verification, not just a log line**: the verification WAV was opened and listened to — recording is intelligible, not silent, not garbled.
- [x] No STT/Cleanup Pass wiring yet — buffer handoff verified via unit tests (`core/src/audio.rs`, `core/src/engine.rs`); real-world verification is the manual step above
