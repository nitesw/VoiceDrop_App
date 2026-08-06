# 0008 — Local Cleanup Pass is a self-contained in-process model again

Supersedes [ADR-0007](0007-ollama-backed-local-cleanup.md)'s Ollama-backed
`Local` provider. Restores (with refinements) the approach from
[ADR-0006](0006-shared-ggml-symbol-collision-and-model-catalog.md). Amends
[ADR-0002](0002-local-first-with-byo-key-cloud-fallback.md)/
[ADR-0005](0005-cleanup-pass-optional-and-free-form-endpoint.md).
Tracked in [0004-phase3-cleanup-pass.md](../todos/0004-phase3-cleanup-pass.md).

## Context

ADR-0007 moved the local Cleanup Pass off an in-process model and onto a
locally-running Ollama server, to eliminate the whisper-rs/llama-cpp-2
vendored-ggml symbol collision without maintaining a fragile linker
workaround.

That traded one problem for another: `Local` now meant "have a third-party
app installed and running," which isn't what most users mean by "run it
locally" — they mean "no extra setup." Explicit product direction: the
Cleanup Pass menu should be **off** / **self-contained built-in model
(zero extra installs, like Whisper)** / **bring your own via literally
anything OpenAI-compatible** (Ollama, LM Studio, vLLM, or a real cloud
API) — three clearly distinct choices, not a Local option that secretly
requires external software.

## Decision

`Local` goes back to an in-process `llama-cpp-2` model
(`cleanup::LocalProvider`), auto-downloaded the same way the Whisper model
is (ADR-0004): no external app, no separate server to keep running.

The whisper-rs/llama-cpp-2 ggml collision from ADR-0006 is real again with
this change — same fix as before: `llama-cpp-sys-2`'s `dynamic-link`
feature (`core/Cargo.toml`), plus matching linker flags in
`macos/Package.swift`. Re-verified against the actual bundled `.app` after
reinstating it, not just `cargo test`.

`CloudProvider` (ADR-0005's free-form OpenAI-compatible endpoint) is now
explicitly documented as the "bring your own via Ollama or any other local
runner" path, not a separate `OllamaProvider` type — pointing `base_url` at
`http://localhost:11434/v1` with any placeholder API key already worked
before ADR-0007 introduced a dedicated wrapper, and still does. Removing
the dedicated type keeps the provider surface at exactly three real
implementations (`None`, `Local`, `Cloud`) plus the `Apple` Swift-side
marker, instead of growing a fourth for what's really a configuration of
the third. `models::OLLAMA_MODELS` (model name suggestions) is kept as
metadata for a future picker UI that pre-fills `Cloud` config for the
Ollama case — it doesn't imply a separate code path.

## Consequences

- Reverts ADR-0007's "no extra native ML runtime" benefit — `llama-cpp-2`
  is back, and so is the obligation to validate any future change to it
  (or to `whisper-rs`) against a real `./scripts/build-macos-app.sh
  release` build, not just `cargo test`, per ADR-0006's lesson.
- Users who want a bigger/better cleanup model than what VoiceDrop bundles
  can still run Ollama (or LM Studio, vLLM, etc.) themselves and point
  `Cloud` at it — nothing about that path was removed, it just isn't
  called `Local` anymore.
- `core/src/models.rs`'s GGUF catalog (`CATALOG`) covers both STT and
  Cleanup Pass models again; `OLLAMA_MODELS` remains as a separate,
  smaller list of suggested names for the BYO-via-Ollama case.
- Phase 5's Settings Window (`0006-phase5-settings-window.md`) needs three
  clearly labeled options, not two: "None", "Built-in (downloads
  automatically)", and "Custom endpoint (Ollama, LM Studio, cloud API,
  etc.)" — the naming matters for setting correct user expectations about
  what each one requires.
