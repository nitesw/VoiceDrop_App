# Phase 2 — Rust Core: Speech-to-Text

Tracks [GitHub issue #3](https://github.com/nitesw/VoiceDrop_App/issues/3). Depends on [0002-phase1-audio-hotkey.md](0002-phase1-audio-hotkey.md) (audio buffer available at end of `Recording`). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md).

Scope: turn a captured audio buffer into a *Raw Transcript*. No filler-word removal, punctuation, or grammar correction happens here — that's the *Cleanup Pass* in Phase 3.

## Todos

**Whisper integration** ([#22](https://github.com/nitesw/VoiceDrop_App/issues/22))
- [ ] Add `whisper.cpp` to the Rust core (via `whisper-rs` bindings, or direct FFI if the crate is insufficient)
- [ ] Decide and download the initial model size (small vs. medium) — benchmark both for latency vs. accuracy on target hardware before locking one in
- [ ] Document the model download/bundling strategy (bundled in the app vs. downloaded on first run) — affects install size and first-run UX
- [ ] Feed the Phase 1 audio buffer into Whisper, get back a *Raw Transcript* string with timestamps

**Language handling** ([#23](https://github.com/nitesw/VoiceDrop_App/issues/23))
- [ ] Add a language setting (explicit selection vs. auto-detect) read from core config
- [ ] Verify Whisper's auto-detect behavior on short audio clips (a few seconds) — auto-detect is less reliable on short utterances, decide whether to default to a configured language instead
- [ ] Test transcription quality across at least 3 languages the user cares about, not just English

**Silence / no-speech handling** ([#24](https://github.com/nitesw/VoiceDrop_App/issues/24))
- [ ] Detect empty or near-empty *Raw Transcript* (silence-only or accidental brief tap)
- [ ] On detection, transition the *Dictation Session* to a distinct "no speech" outcome — short-circuits before the *Cleanup Pass* runs (Phase 3 shouldn't need to handle empty input)
- [ ] Define the threshold for "too short to bother transcribing" (e.g. audio buffer under N milliseconds skips Whisper entirely)

**Custom Vocabulary hook (foundation only)** ([#25](https://github.com/nitesw/VoiceDrop_App/issues/25))
- [ ] Confirm whisper.cpp's initial-prompt / vocabulary-biasing mechanism and how it would accept a *Custom Vocabulary* list — implementation of the editable list itself is Phase 5, but the STT-side plumbing to accept a bias list belongs here so Phase 5 isn't blocked on core changes

## Done when

- A captured audio buffer reliably produces a *Raw Transcript* string via Whisper
- Silence/no-speech input is detected and short-circuited before reaching the Cleanup Pass
- Language selection works for at least the initial target language set
- The core exposes a hook to pass a vocabulary bias list into transcription, even if nothing populates it yet
