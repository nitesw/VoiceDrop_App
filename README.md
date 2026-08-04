# VoiceDrop

Push-to-talk dictation with automatic filler-word removal and grammar cleanup. See [CONTEXT.md](CONTEXT.md) for domain vocabulary, [docs/adr/](docs/adr/) for architecture decisions, and [docs/roadmap/ROADMAP.md](docs/roadmap/ROADMAP.md) for the build plan.

## Repo layout

- `core/` — `voicedrop-core`, the shared Rust library (audio, STT, Cleanup Pass, session state — see [ADR-0001](docs/adr/0001-rust-core-with-native-ui-shells.md))
- `macos/` — the SwiftUI/AppKit macOS shell (Swift package), consuming `voicedrop-core` via FFI
- `docs/` — CONTEXT.md-adjacent docs: ADRs, roadmap, per-phase todos

## Building (macOS)

Requires Rust (`rustup`) and Xcode command line tools.

```sh
# 1. Build the Rust core first — the Swift package links against its output.
cargo build --release

# 2. Build and run the macOS shell.
cd macos
swift run
```

On launch, the app runs as a menu-bar-only process (no Dock icon) and logs a round-trip result from the Rust core to the console — confirming the FFI boundary works. Nothing else exists yet.

## Testing

```sh
cargo test --workspace
```
