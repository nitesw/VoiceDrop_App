# VoiceDrop — Roadmap

This is an overview, not a task list — each phase's actual work lives in its own comprehensive todo doc under [docs/todos/](../todos/), tracked against the GitHub issues below. This file exists to show how the phases fit together and why they're sequenced this way; update it when scope shifts between phases, not when a checkbox gets ticked.

Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md) definitions. Architectural rationale is in [docs/adr/](../adr/).

macOS is built first since it's the reference platform and lets the Rust core be validated end-to-end before porting the UI shell to Windows/Linux (see [ADR-0001](../adr/0001-rust-core-with-native-ui-shells.md)).

## Milestones & Tracked Issues

| Milestone | Issue | Todo doc |
| :--- | :--- | :--- |
| **[v0.1 MVP - Core Pipeline & macOS Shell](https://github.com/nitesw/VoiceDrop_App/milestone/1)** | [#1 Phase 0: Repo & Workspace Scaffolding](https://github.com/nitesw/VoiceDrop_App/issues/1) | [0001-initial-setup.md](../todos/0001-initial-setup.md) |
| | [#2 Phase 1: Rust Core - Audio Capture & Hotkey Foundations](https://github.com/nitesw/VoiceDrop_App/issues/2) | [0002-phase1-audio-hotkey.md](../todos/0002-phase1-audio-hotkey.md) |
| | [#3 Phase 2: Rust Core - Speech-to-Text Integration](https://github.com/nitesw/VoiceDrop_App/issues/3) | [0003-phase2-stt.md](../todos/0003-phase2-stt.md) |
| | [#4 Phase 3: Rust Core - Cleanup Pass & Provider Architecture](https://github.com/nitesw/VoiceDrop_App/issues/4) | [0004-phase3-cleanup-pass.md](../todos/0004-phase3-cleanup-pass.md) |
| | [#5 Phase 4: macOS Shell - Core Interaction Loop & Injection](https://github.com/nitesw/VoiceDrop_App/issues/5) | [0005-phase4-macos-core-loop.md](../todos/0005-phase4-macos-core-loop.md) |
| | [#6 Phase 5: macOS Shell - Settings Window & Persisted History](https://github.com/nitesw/VoiceDrop_App/issues/6) | [0006-phase5-settings-window.md](../todos/0006-phase5-settings-window.md) |
| | [#7 Phase 6: Robustness - Voice Commands, Errors & Onboarding](https://github.com/nitesw/VoiceDrop_App/issues/7) | [0007-phase6-robustness.md](../todos/0007-phase6-robustness.md) |
| **[v0.2 Cross-Platform Shells (Win & Linux)](https://github.com/nitesw/VoiceDrop_App/milestone/2)** | [#8 Phase 7: Windows Shell Development](https://github.com/nitesw/VoiceDrop_App/issues/8) | [0008-phase7-windows-shell.md](../todos/0008-phase7-windows-shell.md) |
| | [#9 Phase 8: Linux Shell Development](https://github.com/nitesw/VoiceDrop_App/issues/9) | [0009-phase8-linux-shell.md](../todos/0009-phase8-linux-shell.md) |
| **[v0.3 Hardening & Distribution](https://github.com/nitesw/VoiceDrop_App/milestone/3)** | [#10 Phase 9: Distribution, Packaging & Auto-Updates](https://github.com/nitesw/VoiceDrop_App/issues/10) | [0010-phase9-distribution.md](../todos/0010-phase9-distribution.md) |
| | [#11 Phase 10: Testing, Quality Assurance & Performance Tuning](https://github.com/nitesw/VoiceDrop_App/issues/11) | [0011-phase10-testing-qa.md](../todos/0011-phase10-testing-qa.md) |

> Project Board: **[Product Roadmap](https://github.com/users/nitesw/projects/4)**

## Phase Narrative

**Phase 0 — Repo & Workspace Scaffolding.** Get a buildable, runnable skeleton: empty Rust workspace, a menu-bar-only SwiftUI shell, and one proven FFI round-trip. No product logic yet.

**Phase 1 — Audio Capture & Hotkey Foundations.** The *Push-to-Talk Hotkey* and the *Dictation Session* state machine come alive: holding the key captures raw audio, releasing it stops — before any STT or cleanup exists to act on that audio.

**Phase 2 — Speech-to-Text.** Whisper turns captured audio into a *Raw Transcript*, including language handling and silence detection. Still no cleanup — the transcript is raw.

**Phase 3 — Cleanup Pass.** The three cleanup backends (local llama.cpp, Apple's on-device model, BYO-key cloud — see [ADR-0002](../adr/0002-local-first-with-byo-key-cloud-fallback.md)) turn a *Raw Transcript* into a *Cleaned Transcript*, respecting *Cleanup Strength*. This completes the headless pipeline: audio in, cleaned text out, no UI yet.

**Phase 4 — macOS Shell: Core Interaction Loop.** The pipeline gets a face: *Menu Bar Icon*, *Dictation HUD*, and text injection into the *Injection Target* at the cursor, with clipboard fallback. This is the first phase where VoiceDrop is usable end-to-end on macOS.

**Phase 5 — Settings Window & Persisted History.** Everything configurable moves out of hardcoded defaults into the *Settings Window*: hotkey rebinding, *Launch at Login*, HUD position, *Cleanup Strength*, *Custom Vocabulary*, cloud key entry, and the *Session History* view.

**Phase 6 — Robustness.** *Voice Command* handling, secure-field detection, mid-session failure handling, and first-run permissions onboarding. This phase is what takes the macOS app from "works on the happy path" to "safe to hand to a real user."

**Phase 7 & 8 — Windows & Linux Shells.** With the Rust core validated on macOS, these phases are primarily UI-shell + OS-integration work (tray icon, global hotkey, injection, autostart) reusing the same core (see [ADR-0001](../adr/0001-rust-core-with-native-ui-shells.md)).

**Phase 9 — Distribution & Packaging.** Signing, notarization, installers, and update channels per platform (see [ADR-0003](../adr/0003-direct-download-distribution-on-macos.md) for why macOS ships outside the App Store).

**Phase 10 — Testing & QA.** Deliberately last: unit and integration tests against the pipeline built in Phases 1–3, an injection-target test matrix, voice-command disambiguation tests, manual QA per platform, and latency benchmarking.
