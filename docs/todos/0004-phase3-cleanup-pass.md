# Phase 3 — Rust Core: Cleanup Pass & Provider Architecture

Tracks [GitHub issue #4](https://github.com/nitesw/VoiceDrop_App/issues/4). Depends on [0003-phase2-stt.md](0003-phase2-stt.md) (a *Raw Transcript* to clean). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md). Architecture rationale: [ADR-0002](../adr/0002-local-first-with-byo-key-cloud-fallback.md).

Scope: turn a *Raw Transcript* into a *Cleaned Transcript*. This phase completes the headless pipeline — after this, audio-in produces cleaned-text-out with no UI involved yet.

## Todos

**Provider interface** ([#26](https://github.com/nitesw/VoiceDrop_App/issues/26))
- [ ] Define a `CleanupProvider` trait/interface in Rust with one method: raw transcript + strength + language in, cleaned transcript out
- [ ] Make provider selection a runtime config value (not compile-time), since the user picks between local/Apple/cloud per [ADR-0002](../adr/0002-local-first-with-byo-key-cloud-fallback.md)
- [ ] Define a common error type across providers (timeout, inference failure, network failure for cloud) so the Swift shell doesn't need per-provider error handling later

**Local provider (llama.cpp)** ([#27](https://github.com/nitesw/VoiceDrop_App/issues/27))
- [ ] Add `llama.cpp` bindings to the Rust core (via `llama-cpp-rs` or direct FFI)
- [ ] Select and bundle/download a small quantized instruction-tuned model (GGUF) — benchmark at least two candidates (e.g. Qwen2.5, Llama 3.2) for latency and cleanup quality
- [ ] Build the cleanup prompt template: instructs the model to strip disfluencies, punctuate, and correct grammar without changing meaning
- [ ] Verify inference latency is compatible with the "few seconds of processing" UX target on minimum-spec hardware

**Apple Foundation Models provider (macOS only)** ([#28](https://github.com/nitesw/VoiceDrop_App/issues/28))
- [ ] Integrate Apple's on-device Foundation Models framework as an alternative provider, exposed only on macOS
- [ ] Confirm minimum macOS version required and gracefully disable this option on unsupported systems
- [ ] Compare output quality/latency against the local llama.cpp provider to validate it's worth offering as a choice

**Cloud provider (BYO key)** ([#29](https://github.com/nitesw/VoiceDrop_App/issues/29))
- [ ] Implement a cloud provider that calls an external API (e.g. Anthropic) using a user-supplied key
- [ ] No key stored/transmitted anywhere except to the provider's own API directly from the client — no VoiceDrop backend in the loop
- [ ] Handle missing/invalid key and network failure by surfacing a clear error, not a silent fallback to local (silent provider-switching would violate the user's explicit opt-in choice)

**Cleanup Strength** ([#30](https://github.com/nitesw/VoiceDrop_App/issues/30))
- [ ] Implement the three *Cleanup Strength* levels (verbatim-clean, light-edit, formal-rewrite) as distinct prompt variants, applied uniformly across all three providers
- [ ] Write before/after test cases for each strength level to confirm the boundary is respected (e.g. verbatim-clean must not merge sentences)

## Done when

- Given a *Raw Transcript*, all three providers (local, Apple-native on macOS, cloud) independently produce a *Cleaned Transcript*
- Switching providers is a config change, not a code change
- Each *Cleanup Strength* level produces observably different output on the same input
- Provider failures surface as errors rather than silently falling back to a different provider
