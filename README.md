# VoiceDrop

Push-to-talk dictation for macOS. **Hold ⌥Space, speak, release**: the text
drops at your cursor. Speech recognition runs on your Mac (Whisper
large-v3-turbo on the GPU); nothing leaves the machine.

See [ADR-0009](docs/adr/0009-simplify-to-hold-speak-paste.md) for why the app
is this small and what was deliberately left out, and
[CONTEXT.md](CONTEXT.md) for the vocabulary used in the code.

## Using it

| Do | Hotkey |
| :--- | :--- |
| Dictate | Hold **⌥ Space**, speak, release (or tap to start, tap again to stop: see below) |
| Switch language | Tap **⌥ ⇧ Space**: English → Ukrainian → French → Auto-detect |

- The language you pick is remembered. On Auto, very short clips are treated
  as English.
- Where the text goes: typed straight into the focused field when the app
  supports it, otherwise auto-pasted. If neither works, it's copied to the
  clipboard and the HUD says "Copied to clipboard". Password fields are
  detected and the text is discarded, never pasted or copied.
- **Tap ⌥Space to Start/Stop** (menu bar): if you'd rather not hold the
  keys, turn this on. One tap starts listening, the next tap stops and drops
  the text.
- The menu bar icon has **Enabled** (turns the hotkeys off and frees the
  model's memory), **Tap ⌥Space to Start/Stop**, **Launch at Login** and
  **Quit**.

## Installing (macOS 13+, built and tested on Apple Silicon)

Requires Rust (`rustup`) and the Xcode command line tools.

```sh
./scripts/setup-signing.sh   # once: creates a local code-signing certificate
./scripts/install.sh         # build, copy to /Applications, launch
```

On first launch:

1. **Accessibility:** macOS asks you to allow VoiceDrop in System Settings →
   Privacy & Security → Accessibility. The HUD shows a reminder until you do,
   and the hotkeys start working the moment you allow it.
2. **Microphone:** allow it when asked.
3. **Model:** the Whisper model (~574 MB) downloads once, with progress on
   the HUD, into `~/Library/Application Support/VoiceDrop/models/`.

Run `./scripts/install.sh` again after any code change. Thanks to the stable
signing certificate, macOS keeps the Accessibility permission across
rebuilds. Without it (ad-hoc signing), the permission is silently lost every
time.

## Repo layout

- `core/`: `voicedrop-core`, the Rust library: audio capture and resampling,
  Whisper speech-to-text, the dictation session state machine, the model
  download. Exposed to Swift through a C API (`core/include/voicedrop_core.h`,
  hand-written, keep it in sync).
- `macos/`: the SwiftUI/AppKit menu bar app: hotkeys (`HotkeyMonitor.swift`),
  HUD (`DictationHUD.swift`), text injection (`TextInjector.swift`).
- `scripts/`: build, install, signing setup, app icon.
- `docs/`: ADRs, roadmap, per-phase todo docs (phases 5+ are deferred).

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
./scripts/build-macos-app.sh release   # app bundle in macos/.build/, without installing
```

Logs (no transcript text is ever logged):

```sh
log stream --predicate 'subsystem == "com.voicedrop.app"' --level debug
```
