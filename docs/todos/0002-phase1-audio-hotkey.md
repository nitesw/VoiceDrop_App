# Phase 1 — Rust Core: Audio Capture & Hotkey Foundations

Tracks [GitHub issue #2](https://github.com/nitesw/VoiceDrop_App/issues/2). Depends on [0001-initial-setup.md](0001-initial-setup.md) (working FFI boundary). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md).

Scope: prove that holding the *Push-to-Talk Hotkey* and releasing it reliably captures a raw audio buffer in the Rust core, driven by a real session state machine — no STT or cleanup yet, just audio in, buffer out.

## Todos

**Dictation Session state machine** ([#19](https://github.com/nitesw/VoiceDrop_App/issues/19))
- [ ] Define the *Dictation Session* states in Rust: `Idle`, `Recording`, `Processing`, `Done`, `Discarded`, `Error`
- [ ] Define the valid transitions only (e.g. `Idle → Recording` on hotkey-down, `Recording → Processing` on hotkey-up, `Processing → Done`/`Error`)
- [ ] Expose state-change events across the FFI boundary so the Swift shell can react (e.g. update the *Dictation HUD* later)
- [ ] Unit tests: illegal transitions are rejected (e.g. can't go `Idle → Processing` directly)

**Audio capture** ([#20](https://github.com/nitesw/VoiceDrop_App/issues/20))
- [ ] Add `cpal` (or equivalent cross-platform audio crate) to `voicedrop-core`
- [ ] Enumerate and open the default input device; handle "no input device available" gracefully
- [ ] Stream PCM buffers into an in-memory buffer for the duration of `Recording` state
- [ ] Decide and document buffer format (sample rate, channels, bit depth) that Whisper expects downstream, so Phase 2 doesn't need to reformat
- [ ] Handle mid-recording device disconnection (e.g. AirPods dropping) without crashing — transition to `Error`

**Hotkey capture (macOS)** ([#21](https://github.com/nitesw/VoiceDrop_App/issues/21))
- [ ] Implement global hotkey listener in the Swift shell using Accessibility/Input Monitoring APIs (per [ADR-0001](../adr/0001-rust-core-with-native-ui-shells.md), hotkey capture is a per-OS adapter, not shared Rust code)
- [ ] Key-down calls into Rust core to transition `Idle → Recording` and start audio capture
- [ ] Key-up calls into Rust core to transition `Recording → Processing` and stop audio capture, handing off the finished buffer
- [ ] Debounce/guard against key-repeat events re-triggering recording while already `Recording`
- [ ] Handle the permission-not-granted case: detect missing Accessibility/Input Monitoring permission and surface it (full onboarding flow is Phase 6, but the detection hook belongs here)

## Done when

- Holding the configured hotkey anywhere on macOS starts audio capture; releasing it stops and hands a complete PCM buffer to the Rust core
- The *Dictation Session* state machine enforces valid transitions and rejects invalid ones (covered by unit tests)
- A brief accidental tap or losing the input device doesn't crash the app
- No STT/Cleanup Pass wiring yet — buffer handoff can be verified via a debug log/test harness
