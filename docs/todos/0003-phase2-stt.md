# Phase 2 — Rust Core: Speech-to-Text

Tracks [GitHub issue #3](https://github.com/nitesw/VoiceDrop_App/issues/3). Depends on [0002-phase1-audio-hotkey.md](0002-phase1-audio-hotkey.md) (audio buffer available at end of `Recording`). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md).

Scope: turn a captured audio buffer into a *Raw Transcript*. No filler-word removal, punctuation, or grammar correction happens here — that's the *Cleanup Pass* in Phase 3.

## Todos

**Whisper integration** ([#22](https://github.com/nitesw/VoiceDrop_App/issues/22))
- [x] Add `whisper.cpp` to the Rust core (via `whisper-rs` bindings) — `core/src/transcribe.rs`, `whisper-rs = "0.16"` in `core/Cargo.toml`
- [x] Decide the initial model size — `ggml-small` chosen as the default pending real hardware benchmarking (not yet done); path is runtime-overridable (`voicedrop_engine_set_model_path`) specifically so swapping to `medium` doesn't need a code change. See the ADR below.
- [x] Document the model download/bundling strategy — [0004-whisper-model-download-on-first-run.md](../adr/0004-whisper-model-download-on-first-run.md): downloaded on first run to `~/Library/Application Support/VoiceDrop/models/`, not bundled in the `.app`. Swift-side first-run download flow itself is **not yet implemented** — `scripts/download-whisper-model.sh` covers local dev only.
- [x] Feed the Phase 1 audio buffer into Whisper, get back a *Raw Transcript* string — `Transcriber::transcribe` in `core/src/transcribe.rs`, wired into `Engine::stop_recording` (`core/src/engine.rs`). No timestamps surfaced yet (whisper.cpp segments carry them; not threaded through the FFI boundary since nothing downstream needs them yet — add if Phase 3/4 does). **Manually verified**: real mic input through the built app, repeated hold/release cycles, correct transcripts logged each time (see `HotkeyMonitor.swift` debug logging).

**Language handling** ([#23](https://github.com/nitesw/VoiceDrop_App/issues/23))
- [x] Add a language setting (explicit selection vs. auto-detect) read from core config — `LanguageSetting` in `transcribe.rs`, set via `voicedrop_engine_set_language`
- [x] Verify Whisper's auto-detect behavior on short clips — per whisper.cpp's own known limitation, auto-detect on `Auto` falls back to a configured `fallback_language` (`voicedrop_engine_set_fallback_language`) for clips under `AUTO_DETECT_MIN_MS` (2s); unit-tested in `transcribe::tests`
- [x] Test transcription quality across at least 3 languages the user cares about, not just English — manually verified via `VOICEDROP_LANGUAGE` env var (temporary test hook in `main.swift`, superseded by Phase 5's Settings Window) against English, French, Ukrainian, Polish on `ggml-small`. English/French/Ukrainian came back accurate; Polish had one garbled take on a short clip — plausibly a `ggml-small` accuracy limit rather than a plumbing bug, worth re-checking once `ggml-medium` benchmarking happens

**Silence / no-speech handling** ([#24](https://github.com/nitesw/VoiceDrop_App/issues/24))
- [x] Detect empty or near-empty *Raw Transcript* — `Engine::stop_recording` treats an empty (post-trim) Whisper result the same as too-short audio
- [x] On detection, transition the *Dictation Session* to a distinct "no speech" outcome — new `SessionState::NoSpeech` / `SessionEvent::NoSpeechDetected` in `core/src/session.rs`, reachable only from `Processing`, resets to `Idle` like other terminal states; unit-tested
- [x] Define the threshold for "too short to bother transcribing" — `transcribe::MIN_SPEECH_MS` (300ms); `Engine::stop_recording` skips Whisper entirely below it

**Custom Vocabulary hook (foundation only)** ([#25](https://github.com/nitesw/VoiceDrop_App/issues/25))
- [x] Confirm whisper.cpp's initial-prompt mechanism and accept a *Custom Vocabulary* list — `TranscribeConfig::vocabulary` joined into whisper.cpp's initial-prompt param (`build_initial_prompt` in `transcribe.rs`), settable via `voicedrop_engine_set_vocabulary` (comma-separated). Nothing populates it yet — Phase 5 owns the editable list UI.

## Done when

- [x] A captured audio buffer reliably produces a *Raw Transcript* string via Whisper — manually verified with `scripts/download-whisper-model.sh small` + real mic input; multiple consecutive hold/release cycles all transcribed correctly
- [x] Silence/no-speech input is detected and short-circuited before reaching the Cleanup Pass
- [x] Language selection works for at least the initial target language set — verified against English, French, Ukrainian, Polish
- [x] The core exposes a hook to pass a vocabulary bias list into transcription, even if nothing populates it yet
