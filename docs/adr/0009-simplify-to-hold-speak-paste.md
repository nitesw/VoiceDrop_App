# 0009 — Simplify VoiceDrop to "hold, speak, paste"

**Status: accepted (2026-11-08).** Supersedes the product scope in
[ROADMAP.md](../roadmap/ROADMAP.md) and phases 5–10 in [docs/todos/](../todos/).
Supersedes [ADR-0002](0002-local-first-with-byo-key-cloud-fallback.md) (cloud
fallback), [ADR-0005](0005-cleanup-pass-optional-and-free-form-endpoint.md),
[ADR-0006](0006-shared-ggml-symbol-collision-and-model-catalog.md),
[ADR-0007](0007-ollama-backed-local-cleanup.md) and
[ADR-0008](0008-local-cleanup-in-process-again.md) (all about the Cleanup Pass).

## Why

The original plan was a full product: three cleanup backends, a tabbed Settings
Window, history, voice commands, onboarding, Windows and Linux shells, signed
installers. After phases 0–4 the core loop worked, but the app was too heavy for
what it's actually used for, and it still wasn't usable every day:

- Cleanup was off by default and could only be turned on with environment
  variables from a terminal.
- The llama.cpp libraries were loaded from the repo's `target/release` folder,
  so the app broke if it was moved to `/Applications` or after `cargo clean`.
- Raw speech recognition was noticeably worse than Antigravity's built-in voice
  input. The cleanup layer didn't fix that; a better STT setup does.

The goal now is one personal macOS app that does one thing reliably.

## The product, after this decision

1. Open the app once (or have it launch at login). It lives in the menu bar.
2. **Hold the record hotkey** (⌥ Option + Space, replacing the awkward
   Control+Option+D), speak, **release**: the text is
   dropped at the cursor. Direct Accessibility insertion first, auto-paste when an
   app doesn't support that, and clipboard plus a HUD notice if both fail.
3. **Tap the language hotkey** (⌥ Option + Shift + Space) to cycle the dictation language (English →
   Ukrainian → French → Auto). The HUD briefly shows the new language. English is
   the default, and the choice is remembered.

Dictation only starts on ⌥ Space *without* Shift, so the two hotkeys can't be
confused. Pressing Shift during a recording is ignored. Neither combination
clashes with this Mac's system shortcuts: ⌃ Space and ⌃⌥ Space switch the
keyboard input source, and ⌘ Space is Spotlight.

Everything is driven by hotkeys. The menu bar only keeps Enabled, Launch at Login
and Quit.

### What stays (already built, kept as-is)

- Push-to-talk hotkey via `CGEventTap`, including the tap-disable recovery when
  Accessibility is revoked (`HotkeyMonitor.swift`)
- The Dictation HUD and all its states: waveform, processing, no speech,
  copied to clipboard, discarded, error (`DictationHUD.swift`)
- Text injection: AX insertion, auto-paste fallback, clipboard fallback,
  focus-change detection (`TextInjector.swift`)
- Secure/password field detection: the text is discarded, never pasted or copied
- First-run Whisper model download with progress (`ModelProvisioner.swift`)
- Menu bar icon with the Enabled toggle as a real kill switch (stops the tap,
  unloads the model)
- Rust core: audio capture, session state machine, Whisper transcription,
  silence detection
- Fallback language for short clips (`set_fallback_language`), fixed to
  English: when the cycle is on Auto and a clip is too short to detect reliably,
  Whisper uses English instead of guessing

### What changes (small additions, detailed in the implementation plan)

- **Better speech recognition:** Metal GPU enabled for whisper.cpp (currently
  `GGML_METAL=OFF`), the model switched from Whisper small to large-v3-turbo
  (quantized, about 550 MB), the language set explicitly instead of guessed per
  clip, beam search instead of greedy decoding, and a proper resampler instead of
  the Phase 1 linear one.
- **Language-cycle hotkey**, remembered between launches.
- **Launch at Login** via `SMAppService`.
- **Stable self-signed code signature**, so the hotkey works on the first launch
  after a rebuild (was [#71](https://github.com/nitesw/VoiceDrop_App/issues/71)).
- **Self-contained `.app`**: no dylibs pointing back into the repo.

## What gets removed, and why

Nothing is lost: the full pre-simplification code lives on `main` at commit
`a5db9dc` (merge of PR #73). Each item below can be restored from there.

### Rust core

| Removed | Why |
| :--- | :--- |
| `core/src/cleanup.rs`: the Cleanup Pass, `None`/`Local`/`Cloud` providers, Cleanup Strength, prompts | The main source of weight. A larger Whisper model already adds punctuation and mostly drops "um/uh". The local LLMs (0.5–3B) added latency and sometimes rewrote what was said. |
| `llama-cpp-2`, `llama-cpp-sys-2` dependencies | Only used for local cleanup. They also caused the ggml duplicate-symbol problem (ADR-0006) and the non-portable dylib rpath. |
| `encoding_rs`, `serde`, `serde_json` dependencies, `ureq`'s `json` feature | Only used by cleanup (token decoding, cloud JSON). `ureq` itself stays for the model download. |
| `core/src/blocklist.rs`: word blocklist | Post-processing on top of cleanup, with no UI to edit it. |
| `models.rs`: cleanup catalog entries, Ollama suggestions, catalog listing FFI, `voicedrop_model_delete` | There's one model now. The download and `is_downloaded` logic stays for that single Whisper entry. |
| `engine.rs`: `set_cleanup_provider/strength/local_model_path/cloud_config`, `set_cleaned_transcript`, `cleanup_prompt_for_strength`, `set_blocklist`, separate raw/cleaned transcripts | Follows from removing cleanup: one transcript, straight from Whisper. |
| `engine.rs`: `set_vocabulary` FFI (Custom Vocabulary) | No UI to set it. The Whisper `initial_prompt` hook is about 10 lines and is where the deferred "screen context" idea would plug in. |
| `engine.rs`/`audio.rs`: `write_verification_wav`, `hound` dependency | Phase 1 debug aid for listening to captured audio. |

### macOS shell

| Removed | Why |
| :--- | :--- |
| Environment-variable config in `main.swift` (`VOICEDROP_LANGUAGE`, `_CLEANUP_PROVIDER`, `_LOCAL_MODEL_PATH`, `_CLOUD_*`, `_CLEANUP_STRENGTH`, `_BLOCKLIST`) | Debug scaffolding. Language moves to the hotkey, and the rest no longer exists. |
| `Raw Transcript:` / `Cleaned Transcript:` logging and the per-session verification WAV in `HotkeyMonitor.swift` | Debug scaffolding. It also wrote everything you dictate, in plain text, to the system log and the temp folder. |
| Dead "Settings…" menu item | There is no Settings window, by design. |
| `HUDPosition` enum | The HUD is fixed at bottom center. A position picker is deferred. |
| llama dylib linking and the absolute `-rpath` to `target/release` in `Package.swift` | Follows from removing llama.cpp. |

### Scripts and files on disk

| Removed | Why |
| :--- | :--- |
| `scripts/download-cleanup-model.sh` | No cleanup models. |
| `scripts/download-whisper-model.sh` | The app already downloads its model on first run. |
| `~/Library/Application Support/VoiceDrop/models/qwen2.5-*.gguf` (about 1.6 GB) and `ggml-small.bin` | Unused after the switch. These are your files, not repo files: delete them by hand once the new model works. |

### Docs

| Change | Why |
| :--- | :--- |
| ADR-0002, 0005, 0006, 0007, 0008 marked superseded by this ADR | Their decisions no longer apply. They're kept for history. |
| `ROADMAP.md` rewritten to the simplified scope | The old phase plan is no longer the plan. |
| `docs/todos/0006`–`0011` marked deferred, not deleted | They hold the detailed thinking if any item comes back. |
| `CONTEXT.md`: Cleanup Pass, Cleaned Transcript, Cleanup Strength, Settings Window, Session History, Custom Vocabulary and Voice Command marked deferred; Language Hotkey added | Keeps the glossary true to the app. |
| `README.md` "Manual STT/Cleanup verification" sections replaced with build-and-use steps | They describe the removed env vars and debug logs. |

## Deferred: not being built now

These are ideas worth keeping, not commitments. Each says where its earlier
thinking lives.

| Idea | Earlier thinking | Note |
| :--- | :--- | :--- |
| Cleanup Pass (filler removal, grammar, strengths) via local LLM, Apple Foundation Models or a cloud endpoint | ADR-0005/0007/0008, todo 0004, `cleanup.rs` at `a5db9dc` | Revisit only if large-v3-turbo output still needs it. Apple Foundation Models would avoid the llama.cpp weight entirely. |
| Cloud speech-to-text (Gemini, OpenAI, Groq) as an optional higher-accuracy mode | ADR-0002 | Best accuracy, but needs internet and sends audio off the Mac. |
| Screen-context hint: pass the text before the cursor as Whisper's `initial_prompt` so names and terms on screen come out right | — | Small to add, since `TextInjector` already has AX access to the field. The closest equivalent to how Antigravity "knows" a project. |
| Custom Vocabulary list | todo 0006 (#40) | Same Whisper hook as the context hint. |
| Word blocklist | todo 0006 (#70), `blocklist.rs` at `a5db9dc` | |
| Voice Commands ("scratch that", "new paragraph") | todo 0007 (#42) | Depended on the Cleanup Pass LLM. |
| Session History | todo 0006 (#41) | A short "last transcript" re-copy could be a lighter version. |
| Settings Window, hotkey rebinding, HUD position picker | todo 0006 (#36, #37, #38) | Hotkeys stay hardcoded for now. |
| Model picker / multiple Whisper sizes | todo 0006 (#69) | |
| Full first-run onboarding flow | todo 0007 (#45) | A single clear "grant Accessibility" message is still in scope. |
| Inference timeouts and failure-handling hardening | todo 0007 (#43, #44) | Fix specific bugs as they show up in daily use instead. |
| Mixed languages inside one sentence | — | A Whisper limitation (one language per clip). Workaround: switch the language, or record the quote as its own press. |
| Windows and Linux shells | todos 0008, 0009 | |
| Notarization, installers, auto-updates | todo 0010, ADR-0003 | Personal use only for now. |
| Formal QA, injection test matrix, latency benchmarks | todo 0011 | |

## Consequences

- One binary, one model, no external libraries outside the `.app`. It can live in
  `/Applications`, and the build gets faster and simpler.
- No configuration UI: the record hotkey and language-cycle hotkey are hardcoded.
  Changing them means editing code.
- Raw Whisper output is what gets pasted. If filler words or casing turn out to be
  a real annoyance in daily use, the Cleanup Pass is the first deferred item to
  reconsider, ideally without bringing llama.cpp back.
- GitHub issues for deferred items (#36–#45, #69, #70) should be closed or
  labeled `deferred` so the board matches reality. #71 (stable signing) stays
  open because it's part of this work. That's done in the implementation step,
  not here.
