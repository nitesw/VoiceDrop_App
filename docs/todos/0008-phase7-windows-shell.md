# Phase 7 — Windows Shell Development

Tracks [GitHub issue #8](https://github.com/nitesw/VoiceDrop_App/issues/8). Depends on the macOS app being feature-complete through [0007-phase6-robustness.md](0007-phase6-robustness.md) — this phase should be primarily UI-shell + OS-integration work against an already-proven Rust core (see [ADR-0001](../adr/0001-rust-core-with-native-ui-shells.md)). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md).

Scope: bring the full macOS feature set to Windows, reusing `voicedrop-core` unchanged wherever possible.

## Todos

**Project setup** ([#46](https://github.com/nitesw/VoiceDrop_App/issues/46))
- [ ] Create the Windows shell project (WinUI3, or Win32 if WinUI3 proves too heavy) consuming `voicedrop-core` via FFI (C ABI or the same binding generator used for Swift)
- [ ] Confirm the Rust core builds and links correctly on Windows (audio via `cpal`, whisper.cpp, llama.cpp — check for Windows-specific build issues in each dependency)
- [ ] Basic CI: Windows build added alongside the existing Rust/macOS CI

**Menu Bar Icon equivalent** ([#47](https://github.com/nitesw/VoiceDrop_App/issues/47))
- [ ] System tray icon with the same dropdown contents as macOS: Enable/Disable, Settings, Quit
- [ ] Icon state reflects enabled/disabled

**Hotkey & injection** ([#48](https://github.com/nitesw/VoiceDrop_App/issues/48))
- [ ] Global hotkey capture via Windows APIs (`RegisterHotKey` or low-level keyboard hook, depending on reliability needs), driving the same *Dictation Session* state machine in the Rust core
- [ ] Text injection via UI Automation or `SendInput`, targeting the *Injection Target* at the cursor position, same fallback-to-clipboard behavior as macOS
- [ ] Secure-field detection equivalent for Windows (behavior parity with the macOS Phase 6 work)

**Dictation HUD** ([#49](https://github.com/nitesw/VoiceDrop_App/issues/49))
- [ ] Native overlay window (borderless, always-on-top) reproducing the same states as macOS: recording/waveform, processing, no-speech, fallback notice
- [ ] Position picker parity (near cursor / bottom of screen / other edges)

**Settings Window & History** ([#50](https://github.com/nitesw/VoiceDrop_App/issues/50))
- [ ] Port the full Settings Window feature set: hotkey rebinding, Launch at Login, HUD position, Cleanup Strength, provider selection, Custom Vocabulary, cloud key entry, Session History view
- [ ] Launch at Login via Windows startup registration (registry `Run` key or Task Scheduler, whichever is more robust)
- [ ] Session History persistence — reuse the same Rust-core-backed storage layer as macOS if feasible, rather than reimplementing

**Onboarding**
- [ ] First-run permission/setup flow equivalent to macOS (Windows doesn't require the same Accessibility grant, but confirm what if anything needs explicit user consent, e.g. microphone access)

## Done when

- Feature parity with the macOS app: same core loop, same Settings Window contents, same HUD behavior
- No Rust core changes were needed beyond genuine Windows-specific adapters (hotkey capture, injection) — if core logic needed forking, that's a signal the Phase 1–3 abstractions weren't clean enough and should be revisited
- Manual QA pass across a few real Windows apps (Notepad, a browser, a terminal)
