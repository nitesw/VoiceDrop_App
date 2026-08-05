# 0004 — Whisper model: downloaded on first run, not bundled

## Context

Phase 2 ([0003-phase2-stt.md](../todos/0003-phase2-stt.md)) needs a whisper.cpp
GGML model file on disk before `Transcriber::load` can run. Two options:
bundle it inside `VoiceDrop.app`, or fetch it on first run.

Model size vs. accuracy (per whisper.cpp's published benchmarks):
- `ggml-small.bin` — ~466 MB, good accuracy/latency balance on CPU, real-time-ish
  on Apple Silicon.
- `ggml-medium.bin` — ~1.5 GB, meaningfully better accuracy (esp. accents,
  noisy audio), noticeably slower on CPU-only inference.

We have not yet benchmarked both on target hardware (M-series Mac, CPU-only
inference — `whisper-rs` Core ML/Metal acceleration is a separate follow-up).
`small` is the default until that benchmarking happens; the model path is
overridable at runtime (`voicedrop_engine_set_model_path`) precisely so
swapping in `medium` doesn't require a code change.

## Decision

Download the model on first run instead of bundling it in the `.app`:

- **Install size**: distribution is [direct-download](0003-direct-download-distribution-on-macos.md),
  not the App Store — but a 466 MB–1.5 GB bundle still makes the initial
  download and every future app-update download that much heavier, for a
  file that never changes across app versions.
- **Swappable without a release**: users (or us, during benchmarking) can
  switch model size without shipping a new build.
- Cost: first launch needs a network connection and a wait; the app must
  handle "model missing" gracefully rather than assuming it's always there.

## Where it lives

`~/Library/Application Support/VoiceDrop/models/ggml-<size>.bin` —
`engine::default_model_path()` in `core/src/engine.rs`. Standard macOS
location for app-managed data that isn't user documents.

## First-run UX (not yet implemented)

Out of scope for Phase 2 itself (which assumes the model file already
exists at the configured path — see `Transcriber::load`'s
`ModelNotFound` error). The Swift shell needs to, on first launch:
1. Check whether the model file exists at `default_model_path()`.
2. If not, download it (from huggingface.co/ggerganov/whisper.cpp or
   equivalent) with visible progress, before the hotkey is usable.
3. Surface a clear error (not a silent hang) if the download fails.

`scripts/download-whisper-model.sh` provides the same fetch for local dev
so `cargo test`/manual runs don't each need to reinvent it.
