# Phase 3 — Rust Core: Cleanup Pass & Provider Architecture

Tracks [GitHub issue #4](https://github.com/nitesw/VoiceDrop_App/issues/4). Depends on [0003-phase2-stt.md](0003-phase2-stt.md) (a *Raw Transcript* to clean). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md). Architecture rationale: [ADR-0002](../adr/0002-local-first-with-byo-key-cloud-fallback.md), amended by [ADR-0005](../adr/0005-cleanup-pass-optional-and-free-form-endpoint.md) (Cleanup Pass is optional; cloud provider is a free-form endpoint, not a fixed vendor list).

Scope: turn a *Raw Transcript* into a *Cleaned Transcript*. This phase completes the headless pipeline — after this, audio-in produces cleaned-text-out with no UI involved yet.

## Todos

**Provider interface** ([#26](https://github.com/nitesw/VoiceDrop_App/issues/26))
- [ ] Define a `CleanupProvider` trait/interface in Rust with one method: raw transcript + strength + language in, cleaned transcript out
- [ ] Make provider selection a runtime config value (not compile-time): `None` / Local / Apple Foundation Models / Cloud — per [ADR-0002](../adr/0002-local-first-with-byo-key-cloud-fallback.md) and [ADR-0005](../adr/0005-cleanup-pass-optional-and-free-form-endpoint.md)
- [ ] Implement `None` as a real provider variant: returns the *Raw Transcript* unchanged. Selecting it means no cleanup model is ever downloaded and no cloud key is ever required — the pipeline hands the Raw Transcript straight to injection (Phase 4)
- [ ] Define a common error type across providers (timeout, inference failure, network failure for cloud) so the Swift shell doesn't need per-provider error handling later

**Local provider (llama.cpp)** ([#27](https://github.com/nitesw/VoiceDrop_App/issues/27))
- [ ] Add `llama.cpp` bindings to the Rust core (via `llama-cpp-rs` or direct FFI)
- [ ] Select a small quantized instruction-tuned model (GGUF) — benchmark at least two candidates (e.g. Qwen2.5, Llama 3.2) for latency and cleanup quality
- [ ] Download on first *use* (not first app run) to a path under Application Support, mirroring the Whisper model strategy ([ADR-0004](../adr/0004-whisper-model-download-on-first-run.md)) — only triggered when the user actually selects the local provider, since `None` and Cloud users should never pull this file down
- [ ] Build the cleanup prompt template: instructs the model to strip disfluencies, punctuate, and correct grammar without changing meaning
- [ ] Verify inference latency is compatible with the "few seconds of processing" UX target on minimum-spec hardware

**Apple Foundation Models provider (macOS only)** ([#28](https://github.com/nitesw/VoiceDrop_App/issues/28))
- [ ] Integrate Apple's on-device Foundation Models framework as an alternative provider, exposed only on macOS
- [ ] Confirm minimum macOS version required and gracefully disable this option on unsupported systems
- [ ] Compare output quality/latency against the local llama.cpp provider to validate it's worth offering as a choice

**Cloud provider (BYO key, free-form endpoint)** ([#29](https://github.com/nitesw/VoiceDrop_App/issues/29))
- [ ] Implement a cloud provider taking a user-supplied **base URL + API key**, not a fixed vendor list — per [ADR-0005](../adr/0005-cleanup-pass-optional-and-free-form-endpoint.md), assume an OpenAI-compatible `/chat/completions` request/response shape, which covers hosted providers, OpenRouter, and self-hosted/local servers (Ollama, LM Studio, vLLM) alike
- [ ] Document the OpenAI-compatible shape assumption at the request-building call site, and surface a clear error (not a crash) if a given endpoint's response doesn't match it
- [ ] No key/URL stored or transmitted anywhere except directly from the client to that endpoint — no VoiceDrop backend in the loop
- [ ] Handle missing/invalid key, malformed URL, and network failure by surfacing a clear error, not a silent fallback to local or `None` (silent provider-switching would violate the user's explicit opt-in choice)

**Cleanup Strength** ([#30](https://github.com/nitesw/VoiceDrop_App/issues/30))
- [ ] Implement the three *Cleanup Strength* levels (verbatim-clean, light-edit, formal-rewrite) as distinct prompt variants, applied uniformly across the local/Apple/cloud providers (not applicable to `None`, which never transforms the text)
- [ ] Write before/after test cases for each strength level to confirm the boundary is respected (e.g. verbatim-clean must not merge sentences)

## Done when

- Given a *Raw Transcript*, all three transforming providers (local, Apple-native on macOS, cloud) independently produce a *Cleaned Transcript*; `None` passes it through unchanged
- Switching providers is a config change, not a code change
- Selecting `None` never triggers a local model download and never requires a cloud key
- The cloud provider works against any OpenAI-compatible endpoint, not just one hardcoded vendor
- Each *Cleanup Strength* level produces observably different output on the same input (for the transforming providers)
- Provider failures surface as errors rather than silently falling back to a different provider
