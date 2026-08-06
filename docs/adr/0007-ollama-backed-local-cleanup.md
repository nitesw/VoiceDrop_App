# 0007 — Local Cleanup Pass runs through Ollama, not an in-process model

**Superseded by [ADR-0008](0008-local-cleanup-in-process-again.md)**: after
product direction that "Local" should mean zero extra installs (like
Whisper), the local Cleanup Pass moved back to a self-contained in-process
model, and the `llama-cpp-2` dynamic-linking approach from ADR-0006 came
back with it. What survives from this ADR: `CloudProvider` remains the
documented "bring your own via Ollama or any other local runner" path
(this ADR's `OllamaProvider` wrapper was removed as redundant — pointing
`CloudProvider` at `http://localhost:11434/v1` does the same thing), and
the deterministic blocklist filter described below is unaffected by either
pivot. Kept for the historical record.

Supersedes the in-process `llama-cpp-2` approach documented in
[ADR-0006](0006-shared-ggml-symbol-collision-and-model-catalog.md).
Amends [ADR-0002](0002-local-first-with-byo-key-cloud-fallback.md) /
[ADR-0005](0005-cleanup-pass-optional-and-free-form-endpoint.md).
Tracked in [0004-phase3-cleanup-pass.md](../todos/0004-phase3-cleanup-pass.md).

## Context

ADR-0006 got an in-process llama.cpp Cleanup Pass working via `llama-cpp-2`,
but only after fixing a real linker collision (whisper-rs and llama-cpp-2
each vendor their own ggml) with a `dynamic-link` build feature — dev-only
wiring that still owed Phase 9 a proper `Contents/Frameworks` bundling
story, and added a second native ML dependency with its own build/update
risk going forward.

Separately, manual testing surfaced that small local models (Qwen2.5-0.5B)
produce mediocre Cleanup Pass output — mediocre enough that the expected
guidance to users is "don't bother with local cleanup unless you need it,
`None` is a perfectly good default." Given that, maintaining custom
in-process inference machinery for a path many users won't use is a bad
trade.

## Decision

The local Cleanup Pass provider now talks to a locally-running
[Ollama](https://ollama.com) server instead of loading a model in-process.
Mechanically this reuses the exact same OpenAI-compatible
`/v1/chat/completions` request/response handling `CloudProvider` already
has (ADR-0005) — `OllamaProvider` (`core/src/cleanup.rs`) is a thin wrapper
that defaults `base_url` to `http://localhost:11434/v1` and skips requiring
an API key (Ollama's local server doesn't check one; a placeholder is sent
so the shared request-building code doesn't need a special case).

This removes `llama-cpp-2` from `voicedrop-core` entirely:
- No more vendored-ggml collision with `whisper-rs` — ADR-0006's
  `dynamic-link` workaround and its Package.swift linker flags are gone,
  not just relocated.
- Ollama owns model pulling and storage. `core/src/models.rs`'s GGUF
  download/delete machinery now only serves the Whisper STT model;
  Cleanup Pass models are a plain suggested-name list
  (`models::OLLAMA_MODELS`) users (or a future Settings UI) hand to
  `ollama pull`.
- One fewer native ML runtime to keep building across macOS/Windows/Linux
  as the other shells come online (Phase 7/8).

## Consequences

- **New runtime dependency**: local Cleanup Pass now requires the user to
  have Ollama installed and running, plus the chosen model pulled
  (`ollama serve` + `ollama pull <model>`). This is a real UX cost
  compared to a self-contained in-process model — Phase 4/5 needs to
  detect "Ollama not reachable" clearly (`OllamaProvider::cleanup` already
  maps a refused connection to `CleanupError::InvalidConfig` with a
  message naming the address it tried, rather than a generic network
  error) and guide the user to install/start it, not just fail silently.
- Suggested models are Ollama model names (e.g. `qwen2.5:0.5b`), not GGUF
  file paths — anywhere UI or docs referenced "the local model file",
  that's now "the selected Ollama model."
- `CleanupProviderKind::Local` in `core/src/engine.rs` keeps its name and
  position in the provider enum (a config value, not a code change per the
  "Provider interface" todo) — only what backs it changed.
- Verified against a real local Ollama server (`ollama pull qwen2.5:0.5b`,
  cleanup request over `http://localhost:11434/v1`), not just in theory.

## Deterministic word blocklist (same session, related but separate)

Also added: `core/src/blocklist.rs`, a word-removal filter that runs
unconditionally on the *Raw Transcript* immediately after STT — before any
Cleanup Pass provider, including `None`, ever sees the text. This is
deliberately NOT part of the Cleanup Pass: given the guidance above that
many users will run with `None` (or a small, unreliable local model), a
"delete this word no matter what" requirement can't depend on an LLM
actually doing it — it needs to hold regardless of provider choice or
quality. Ships with a small built-in profanity default, extendable via
`voicedrop_engine_set_blocklist` (comma-separated custom words, merged with
the defaults). Whole-word, case-insensitive matching; removes the word
entirely and cleans up the surrounding punctuation/whitespace.
