# Phase 3 — Rust Core: Cleanup Pass & Provider Architecture

Tracks [GitHub issue #4](https://github.com/nitesw/VoiceDrop_App/issues/4). Depends on [0003-phase2-stt.md](0003-phase2-stt.md) (a *Raw Transcript* to clean). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md). Architecture rationale: [ADR-0002](../adr/0002-local-first-with-byo-key-cloud-fallback.md), amended by [ADR-0005](../adr/0005-cleanup-pass-optional-and-free-form-endpoint.md) (Cleanup Pass is optional; cloud provider is a free-form endpoint, not a fixed vendor list).

Scope: turn a *Raw Transcript* into a *Cleaned Transcript*. This phase completes the headless pipeline — after this, audio-in produces cleaned-text-out with no UI involved yet.

## Todos

**Provider interface** ([#26](https://github.com/nitesw/VoiceDrop_App/issues/26))
- [x] Define a `CleanupProvider` trait/interface in Rust with one method: raw transcript + strength + language in, cleaned transcript out — `core/src/cleanup.rs`
- [x] Make provider selection a runtime config value (not compile-time): `None` / Local / Apple Foundation Models / Cloud — `CleanupProviderKind` in `core/src/engine.rs`, set via `voicedrop_engine_set_cleanup_provider`
- [x] Implement `None` as a real provider variant: returns the *Raw Transcript* unchanged. Selecting it means no cleanup model is ever downloaded and no cloud key is ever required
- [x] Define a common error type across providers (`CleanupError`: `Timeout`/`InferenceFailed`/`NetworkFailed`/`InvalidConfig`/`ModelNotFound`), mapped to three FFI status codes (`VOICEDROP_ERR_CLEANUP_FAILED`/`_NETWORK_FAILED`/`_INVALID_CONFIG`) so the Swift shell doesn't need per-provider handling

**Local provider (llama.cpp)** ([#27](https://github.com/nitesw/VoiceDrop_App/issues/27))
- [x] Add `llama.cpp` bindings to the Rust core via `llama-cpp-2` (`core/src/cleanup.rs`'s `LocalProvider`). **Hit a real blocker along the way**: `llama-cpp-2` and `whisper-rs` (Phase 2) each vendor their own ggml, and statically linking both produced ~600 duplicate-symbol errors at the *Swift app* link step (not caught by `cargo test`, which tolerates it as warnings — always validate native-dep changes with `./scripts/build-macos-app.sh release`). Fixed via `llama-cpp-sys-2`'s `dynamic-link` feature; full story in [ADR-0006](../adr/0006-shared-ggml-symbol-collision-and-model-catalog.md). **Detour**: briefly replaced with an Ollama-backed provider ([ADR-0007](../adr/0007-ollama-backed-local-cleanup.md)) to sidestep this exact collision, then reverted ([ADR-0008](../adr/0008-local-cleanup-in-process-again.md)) once product direction settled on "Local means zero extra installs" — Ollama/other local runners are still reachable, just via the Cloud provider's free-form endpoint instead of a dedicated `Local` implementation
- [x] Select a small quantized instruction-tuned model (GGUF) — manually compared Qwen2.5-0.5B, Qwen2.5-1.5B, and Llama-3.2-3B side-by-side (same input, all three Cleanup Strength levels, via Ollama). **Qwen2.5-1.5B is the default** (`engine::default_cleanup_model_path`): 0.5B was too weak to be useful; 3B produced more fluent prose but respected `VerbatimClean`'s "preserve exact wording" instruction *less* than the smaller models — bigger isn't automatically better for this task. All three verified running end-to-end (real download, real inference, through the actual bundled app). A latency/quality benchmark on minimum-spec (non-dev) hardware is still open — see below
- [x] Download on first *use*, mirroring the Whisper model strategy ([ADR-0004](../adr/0004-whisper-model-download-on-first-run.md)) — `LocalProvider::load` is only invoked when `CleanupProviderKind::Local` is selected and a session actually completes; `core/src/models.rs` additionally provides a curated catalog + download/delete API (`voicedrop_model_*` FFI) as the backing plumbing for a future picker UI (see the new Phase 5 todo item)
- [x] Build the cleanup prompt template: `cleanup::system_prompt`, one variant per *Cleanup Strength* level, shared across local/cloud (and exposed over FFI for a future Apple provider)
- [ ] Verify inference latency is compatible with the "few seconds of processing" UX target on minimum-spec hardware — only tested on this dev machine so far, not across a real "minimum spec" target

**Apple Foundation Models provider (macOS only)** ([#28](https://github.com/nitesw/VoiceDrop_App/issues/28))
- [x] ~~Integrate Apple's on-device Foundation Models framework as an alternative provider~~ — **architecture decision, not deferred work**: this is a Swift/ObjC-only API with no C ABI, so it cannot be called from `voicedrop-core` at all. `CleanupProviderKind::Apple` in `engine.rs` is a marker: `stop_recording` stops after STT when it's selected, and the Swift shell (Phase 4) must call Foundation Models itself and report the result via `voicedrop_engine_set_cleaned_transcript`. It reuses the exact same prompt wording via `voicedrop_cleanup_prompt_for_strength` rather than duplicating it. See `cleanup.rs`'s module doc and [ADR-0001](../adr/0001-rust-core-with-native-ui-shells.md)'s precedent for OS-specific adapters.
- [ ] (Phase 4, Swift-side) Confirm minimum macOS version required and gracefully disable this option on unsupported systems
- [ ] (Phase 4, Swift-side) Compare output quality/latency against the local llama.cpp provider to validate it's worth offering as a choice

**Cloud provider (BYO key, free-form endpoint)** ([#29](https://github.com/nitesw/VoiceDrop_App/issues/29))
- [x] Implement a cloud provider taking a user-supplied **base URL + API key**, not a fixed vendor list — `core/src/cleanup.rs`'s `CloudProvider`, assumes an OpenAI-compatible `/chat/completions` request/response shape. **Verified against a real self-hosted server**, not just theoretically: installed Ollama locally, pulled `qwen2.5:0.5b`, and ran a real cleanup request against `http://localhost:11434/v1` — confirms the "covers self-hosted servers too" claim in ADR-0005 actually holds
- [x] Document the OpenAI-compatible shape assumption at the request-building call site (`CloudProvider::build_request`/`endpoint_url` doc comments), and surface a clear error (not a crash) if a response doesn't match it (`CleanupError::InferenceFailed` with a descriptive message, not a panic)
- [x] No key/URL stored or transmitted anywhere except directly from the client to that endpoint — no VoiceDrop backend in the loop
- [x] Handle missing/invalid key, malformed URL, and network failure by surfacing a clear error, not a silent fallback — `CloudProvider::new` rejects empty base URL/key upfront (`CleanupError::InvalidConfig`), network failures map to `CleanupError::NetworkFailed`; `Engine::run_cleanup_pass` never falls back to raw text on cleanup failure, it fails the session

**Cleanup Strength** ([#30](https://github.com/nitesw/VoiceDrop_App/issues/30))
- [x] Implement the three *Cleanup Strength* levels (verbatim-clean, light-edit, formal-rewrite) as distinct prompt variants, applied uniformly across local/cloud (and Apple, via the shared FFI export) — `cleanup::system_prompt`; unit-tested that all three differ
- [ ] Write before/after test cases for each strength level to confirm the boundary is respected (e.g. verbatim-clean must not merge sentences) — **tried, doesn't hold with any candidate tested so far**: Qwen2.5-0.5B dropped a clause under `VerbatimClean`; Llama-3.2-3B restructured into a semicolon clause under `VerbatimClean` (worse violation, despite being the largest model); Qwen2.5-1.5B (the chosen default) was the closest to compliant but still made small deviations (dropped "probably"). None of the three actually satisfy "do not merge, split, reorder, or rephrase sentences" under load — this looks like a real limit of small-model instruction-following on this specific constraint, not something more benchmarking alone will fix. Worth trying stronger prompting (e.g. few-shot examples) before assuming a bigger model is the answer

**Word blocklist** (not originally scoped for this phase — added alongside it since it needed to land before `None` became a real, expected default; not tracked under a GitHub issue yet)
- [x] Deterministic "always remove this word" filter (`core/src/blocklist.rs`), run unconditionally on the *Raw Transcript* immediately after STT — before any Cleanup Pass provider, including `None`, ever sees the text. Whole-word, case-insensitive matching; removes the word and cleans up surrounding punctuation/whitespace
- [x] Ships with a small built-in default list (common profanity), extendable via `voicedrop_engine_set_blocklist` (comma-separated custom words, merged with defaults)
- [x] Deliberately NOT part of the Cleanup Pass — since many users are expected to run with `None` (or a small, unreliable local model per the strength-boundary issue below), a "delete this word no matter what" requirement can't depend on an LLM actually doing it reliably
- [ ] Distinguishing this from *Custom Vocabulary* (the STT bias list, Phase 2/5) in the Settings Window UI — both are user-editable word lists with opposite purposes (bias toward vs. always remove), worth flagging so Phase 5 doesn't conflate them

**Model picker** (backing plumbing, UI is Phase 5 — see `docs/todos/0006-phase5-settings-window.md`)
- [x] Curated GGUF catalog for the self-contained `Local` provider (`core/src/models.rs`'s `CATALOG`: Qwen2.5-0.5B/1.5B, Llama-3.2-3B) plus download/delete/is-downloaded FFI (`voicedrop_model_*`)
- [x] Separate suggested-name list for "bring your own via Ollama" (`models::OLLAMA_MODELS`) — Ollama manages the actual pull, VoiceDrop only suggests names to feed into `voicedrop_engine_set_cleanup_cloud_config`

## Done when

- [x] Given a *Raw Transcript*, the local and cloud transforming providers independently produce a *Cleaned Transcript* (both verified against real inference — local via bundled llama.cpp, cloud via a real Ollama server); `None` passes it through unchanged. Apple is Swift-side, not verifiable from this repo's Rust core (see above)
- [x] Switching providers is a config change, not a code change — `voicedrop_engine_set_cleanup_provider`
- [x] Selecting `None` never triggers a local model download and never requires a cloud key
- [x] The cloud provider works against any OpenAI-compatible endpoint, not just one hardcoded vendor — verified against a real self-hosted Ollama server, not just a named cloud vendor
- [ ] Each *Cleanup Strength* level produces observably different output on the same input, **and respects its own stated boundary** — the second half doesn't hold yet with the default small model (see above)
- [x] Provider failures surface as errors rather than silently falling back to a different provider
