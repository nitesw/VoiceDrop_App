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

On launch, the app runs as a menu-bar-only process (no Dock icon). Holding **Control+Option+D** records; releasing it stops, runs Whisper, and logs the *Raw Transcript*. See "Manual STT verification" below to try this yourself.

## Testing

```sh
cargo test --workspace
```

## Manual STT verification

Phase 2 (speech-to-text) doesn't have a UI yet — no Dictation HUD, no injection, no Settings Window (those are Phase 4/5). Until then, verifying it end-to-end means fetching a whisper.cpp model and watching Console logs. These are debug aids meant to be removed once the real UI exists — see the "temporary" notes below.

**1. Fetch a model** (one-time, ~466 MB, needs network):

```sh
./scripts/download-whisper-model.sh small
```

Saves to `~/Library/Application Support/VoiceDrop/models/ggml-small.bin` — the default path `voicedrop_engine_set_model_path` falls back to. Pass `medium` instead of `small` to try the larger model (path becomes `ggml-medium.bin`; requires calling `voicedrop_engine_set_model_path` at runtime to point at it, since the app doesn't currently do this itself).

**2. Build and launch:**

```sh
./scripts/build-macos-app.sh release
open macos/.build/VoiceDrop.app
```

**3. Watch logs** in another terminal:

```sh
log stream --predicate 'subsystem == "com.voicedrop.app"' --level debug
```

Hold Control+Option+D, speak, release. You'll see `Recording started.`, then either `Raw Transcript: ...` or `No speech detected...` if the clip was silent/too short.

**Testing a specific language** — set `VOICEDROP_LANGUAGE` (ISO 639-1 code, e.g. `fr`, `uk`, `pl`) before launching the binary directly (`open` doesn't forward env vars into a bundled `.app`):

```sh
killall VoiceDrop 2>/dev/null
VOICEDROP_LANGUAGE=fr macos/.build/VoiceDrop.app/Contents/MacOS/VoiceDrop &
```

Unset (default) means Whisper auto-detects the spoken language per utterance.

**Temporary, to be removed:** the `Raw Transcript:`/`No transcript available.` logging in `HotkeyMonitor.swift` and the `VOICEDROP_LANGUAGE` env-var read in `main.swift` are Phase 2 debug scaffolding — Phase 3/4 replace them with real Cleanup Pass + injection wiring, and Phase 5's Settings Window replaces the env var with a real language picker. Don't build on top of either; they're going away.
