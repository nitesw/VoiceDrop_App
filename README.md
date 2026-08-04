# VoiceDrop

Push-to-talk dictation with automatic filler-word removal and grammar cleanup. See [CONTEXT.md](CONTEXT.md) for domain vocabulary, [docs/adr/](docs/adr/) for architecture decisions, and [docs/roadmap/ROADMAP.md](docs/roadmap/ROADMAP.md) for the build plan.

## Repo layout

- `core/` — `voicedrop-core`, the shared Rust library (audio, STT, Cleanup Pass, session state — see [ADR-0001](docs/adr/0001-rust-core-with-native-ui-shells.md))
- `macos/` — the SwiftUI/AppKit macOS shell (Swift package), consuming `voicedrop-core` via FFI
- `docs/` — CONTEXT.md-adjacent docs: ADRs, roadmap, per-phase todos

## Building (macOS)

Requires Rust (`rustup`) and Xcode command line tools.

```sh
./scripts/build-macos-app.sh          # debug build (pass "release" for a release build)
open macos/.build/VoiceDrop.app
```

This assembles a real `VoiceDrop.app` bundle (`Info.plist`, ad-hoc code signature with a stable `com.voicedrop.app` identifier) rather than a bare `swift run` binary — required so macOS attributes permission prompts (microphone, Accessibility/Input Monitoring) to VoiceDrop itself instead of to the launching shell, and so grants survive rebuilds.

`swift run` from `macos/` still works for quick iteration (it prints the FFI round-trip result straight to your terminal), but won't have a stable permission identity — use the bundled `.app` for anything touching the hotkey or microphone.

On launch, the app runs as a menu-bar-only process (no Dock icon) and calls into the Rust core — confirming the FFI boundary works. Nothing else exists yet.

## Testing

```sh
cargo test --workspace
```
