# VoiceDrop — Initial Setup

Scope: get the repo into a buildable, runnable skeleton with the Rust-core-to-Swift-shell FFI boundary proven end-to-end. Nothing here touches audio, STT, or the Cleanup Pass — that's [Phase 1](../roadmap/ROADMAP.md#phase-1--rust-core-audio-capture--hotkey-foundations) onward, in [0002-phase1-audio-hotkey.md](0002-phase1-audio-hotkey.md).

Tracks [GitHub issue #1 — Phase 0: Repo & Workspace Scaffolding](https://github.com/nitesw/VoiceDrop_App/issues/1).

## Todos

- [x] Initialize git repo, `.gitignore` (Rust + Xcode + build artifacts) ([#12](https://github.com/nitesw/VoiceDrop_App/issues/12))
- [x] Create Cargo workspace with a `voicedrop-core` lib crate (empty, just compiles) ([#13](https://github.com/nitesw/VoiceDrop_App/issues/13))
- [x] Set up `cargo fmt` / `clippy` lint config ([#14](https://github.com/nitesw/VoiceDrop_App/issues/14))
- [x] Create Xcode project for the macOS shell (`macos/`): SwiftUI app target, configured as a menu-bar-only app (`LSUIElement`) ([#15](https://github.com/nitesw/VoiceDrop_App/issues/15))
- [x] Wire up FFI: build `voicedrop-core` as a static/dynamic lib, generate Swift bindings (`uniffi` or a hand-written C header + Swift wrapper) ([#16](https://github.com/nitesw/VoiceDrop_App/issues/16))
- [x] Smoke test: a single Rust function (e.g. `ping() -> String`) called from the Swift app on launch, result logged to console — proves the FFI boundary works before any real logic is built on top of it ([#17](https://github.com/nitesw/VoiceDrop_App/issues/17))
- [x] Basic CI: build the Rust workspace + run `cargo test` on push ([#18](https://github.com/nitesw/VoiceDrop_App/issues/18))

## Done when

- `git clone` + a documented build command produces a running macOS menu bar app
- That app calls into the Rust core at launch and the round-trip result is visible (console log is enough, no UI needed yet)
- CI is green on a fresh push
