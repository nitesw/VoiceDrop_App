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

**Temporary, to be removed:** the `Raw Transcript:`/`Cleaned Transcript:` logging in `HotkeyMonitor.swift` and the `VOICEDROP_LANGUAGE`/`VOICEDROP_CLEANUP_PROVIDER`/`VOICEDROP_CLOUD_*` env-var reads in `main.swift` are Phase 2/3 debug scaffolding — Phase 4 replaces the logging with real injection wiring, and Phase 5's Settings Window replaces the env vars with real pickers. Don't build on top of either; they're going away.

## Manual Cleanup Pass verification

Phase 3 adds a Cleanup Pass on top of the Raw Transcript from Phase 2. Same caveat as above: no Settings Window yet, so provider selection is env vars.

**Provider selection** (`VOICEDROP_CLEANUP_PROVIDER`, read at launch — run the binary directly, not via `open`, same reason as `VOICEDROP_LANGUAGE`):

- `none` (default if unset) — Raw Transcript passes through unchanged, no model needed
- `local` — a self-contained local llama.cpp Cleanup Pass, no external app required (see [ADR-0008](docs/adr/0008-local-cleanup-in-process-again.md)). Fetch a model first:
  ```sh
  ./scripts/download-cleanup-model.sh
  ```
  Saves Qwen2.5-1.5B-Instruct to `~/Library/Application Support/VoiceDrop/models/` — the default after manual comparison against Qwen2.5-0.5B (too weak) and Llama-3.2-3B (respected the `VerbatimClean` strength *less* than the smaller models). See `core/src/models.rs`'s `CATALOG` for other candidates — download the file yourself and set `VOICEDROP_LOCAL_MODEL_PATH` to its path to try one.
- `cloud` — needs `VOICEDROP_CLOUD_BASE_URL`, `VOICEDROP_CLOUD_API_KEY`, `VOICEDROP_CLOUD_MODEL` set too. Works against any OpenAI-compatible endpoint (see [ADR-0005](docs/adr/0005-cleanup-pass-optional-and-free-form-endpoint.md)) — a real hosted provider, or **this is also how to bring your own model via Ollama or another local runner** instead of the built-in `local` option above:
  ```sh
  brew install ollama
  ollama serve &
  ollama pull qwen2.5:0.5b
  VOICEDROP_CLEANUP_PROVIDER=cloud \
    VOICEDROP_CLOUD_BASE_URL=http://localhost:11434/v1 \
    VOICEDROP_CLOUD_API_KEY=unused \
    VOICEDROP_CLOUD_MODEL=qwen2.5:0.5b \
    macos/.build/VoiceDrop.app/Contents/MacOS/VoiceDrop &
  ```

Then hold the hotkey, speak, release, and watch the log stream (see above) for both `Raw Transcript:` and `Cleaned Transcript:` lines.

**Word blocklist** (`VOICEDROP_BLOCKLIST`, comma-separated custom words, merged with a small built-in default list): runs unconditionally right after STT, regardless of which Cleanup Pass provider — including `none` — is selected. See `core/src/blocklist.rs`.

**Important build note:** `whisper-rs` (STT) and `llama-cpp-2` (local Cleanup Pass) each vendor their own copy of ggml. Statically linking both causes ~600 duplicate-symbol errors at the *Swift app* link step specifically — `cargo build`/`cargo test` won't catch this, they tolerate it as warnings. `core/Cargo.toml` works around it via `llama-cpp-sys-2`'s `dynamic-link` feature, which needs matching linker flags in `macos/Package.swift` (`-lllama -lllama-common` plus an `-rpath` pointing at `target/release`, where the resulting `.dylib`s land). If you touch either dependency, rebuild the actual `.app` (`./scripts/build-macos-app.sh`) before considering it done — see [ADR-0006](docs/adr/0006-shared-ggml-symbol-collision-and-model-catalog.md) for the full story (and [ADR-0008](docs/adr/0008-local-cleanup-in-process-again.md) for why this workaround is in place rather than avoided). This is dev-only wiring; Phase 9 (distribution) still needs to bundle these dylibs into `Contents/Frameworks` properly.
