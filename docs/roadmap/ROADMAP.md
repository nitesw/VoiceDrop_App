# VoiceDrop — Roadmap

As of 2026-11-08, VoiceDrop is a deliberately small, personal macOS app:
**hold ⌥Space, speak, release, and the text drops at the cursor.** The
reasoning, the full list of what was removed and everything deferred are in
[ADR-0009](../adr/0009-simplify-to-hold-speak-paste.md).

## Done

| Phase | What | Todo doc |
| :--- | :--- | :--- |
| 0 | Repo scaffolding, Rust ↔ Swift FFI | [0001](../todos/0001-initial-setup.md) |
| 1 | Audio capture, push-to-talk hotkey, session state machine | [0002](../todos/0002-phase1-audio-hotkey.md) |
| 2 | Whisper speech-to-text, language handling, silence detection | [0003](../todos/0003-phase2-stt.md) |
| 4 | Menu bar icon, Dictation HUD, text injection with paste/clipboard fallback | [0005](../todos/0005-phase4-macos-core-loop.md) |
| Simplify | Remove the Cleanup Pass and debug scaffolding; Whisper large-v3-turbo on the GPU with beam search; band-limited resampler; ⌥Space / ⌥⇧Space hotkeys with a remembered language; Launch at Login; stable local code signing; install to /Applications | [ADR-0009](../adr/0009-simplify-to-hold-speak-paste.md) |

Phase 3 (the Cleanup Pass, [0004](../todos/0004-phase3-cleanup-pass.md)) was
built, then removed by ADR-0009.

## Next

Nothing is planned. Fix what daily use turns up. If something keeps
bothering you, check the ADR-0009 "Deferred" table before building it: most
ideas already have earlier thinking written down.

## Deferred

Phases 5–10 ([0006](../todos/0006-phase5-settings-window.md) to
[0011](../todos/0011-phase10-testing-qa.md)): Settings Window, Session
History, Voice Commands, onboarding, Windows and Linux shells, distribution
and formal QA. Each todo doc is marked deferred and kept as reference.
