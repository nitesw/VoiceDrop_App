# 0005 — Cleanup Pass is fully optional; cloud provider takes a free-form endpoint

Amends [0002-local-first-with-byo-key-cloud-fallback.md](0002-local-first-with-byo-key-cloud-fallback.md).
Tracked in [0004-phase3-cleanup-pass.md](../todos/0004-phase3-cleanup-pass.md).

## Context

Two gaps surfaced once Phase 2 (STT) settled on "download the model on
first run, path is overridable" ([0004](0004-whisper-model-download-on-first-run.md)):

1. ADR-0002's cloud provider was scoped as "a user-supplied key" against a
   named provider (e.g. Anthropic) — implicitly a fixed API shape/base URL
   per provider, hardcoded per integration.
2. The Cleanup Pass had three providers (local llama.cpp, Apple Foundation
   Models, cloud) but no "none" option — a Raw Transcript always got
   cleaned by *something*.

Both are worth loosening before Phase 3 implementation starts.

## Decision

**Cloud provider takes a free-form base URL, not a fixed provider list.**
The user enters an endpoint URL + API key in Settings, not a choice from a
closed dropdown of named vendors. Rationale: an OpenAI-compatible
`/chat/completions` shape is now a de facto standard — self-hosted
inference (Ollama, LM Studio, vLLM), OpenRouter, and most hosted providers
all speak it. Hardcoding "Anthropic" (or any single vendor) forecloses
those without benefit; a free-form URL costs little extra (one text field
+ one request-shape assumption) and covers all of them, including a
same-machine local server the user runs themselves. If a provider needs a
genuinely different request/response shape than the assumed
OpenAI-compatible one, that's a second provider variant to add later, not
a reason to lock the first one down now.

**Cleanup Pass is optional — "None" is a valid provider selection.**
When set to `None`, the Dictation Session pipeline delivers the *Raw
Transcript* straight to the *Injection Target*, skipping the Cleanup Pass
entirely (still subject to injection/fallback logic from Phase 4). This
serves users who don't want *any* processing model on their machine — no
local model download, no cloud key required at all — trading disfluencies/
punctuation/grammar-correction for zero extra dependencies.

**Local llama.cpp model: downloaded on first run, mirroring STT.**
Per the same reasoning as [0004](0004-whisper-model-download-on-first-run.md)
— GGUF cleanup models are large enough (hundreds of MB to a few GB
depending on the candidate chosen) that bundling them would bloat every
direct-download update. Bundle-vs-download was previously listed as
undecided in the Phase 3 todo; it's decided now for consistency, though the
model *file itself* is only fetched if/when the user selects the local
provider — `None` and cloud users never trigger this download.

## Consequences

- Settings needs: provider picker (`None` / Local / Apple Foundation Models
  [macOS only] / Cloud), and for Cloud a base-URL field + API-key field
  instead of a vendor dropdown.
- The `CleanupProvider` trait (Phase 3, [#26](https://github.com/nitesw/VoiceDrop_App/issues/26))
  needs a `None`/no-op implementation alongside the three real providers.
- Cloud provider validation can't rely on vendor-specific response
  shape assumptions beyond "OpenAI-compatible chat completions" — document
  that assumption where the request is built, and surface a clear error
  (not a crash) if a given endpoint doesn't match it.
- First-run download UX (Swift-side, still unbuilt per
  [0004](0004-whisper-model-download-on-first-run.md)) now needs to handle
  two independently-triggered downloads — Whisper model always (STT is not
  optional) and the cleanup GGUF model only when the local Cleanup Pass
  provider is selected.
