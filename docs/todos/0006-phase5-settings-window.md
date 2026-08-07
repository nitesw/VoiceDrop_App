# Phase 5 — macOS Shell: Settings Window & Persisted History

Tracks [GitHub issue #6](https://github.com/nitesw/VoiceDrop_App/issues/6). Depends on [0005-phase4-macos-core-loop.md](0005-phase4-macos-core-loop.md) (core loop works with hardcoded defaults). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md).

Scope: move every hardcoded default from Phase 4 into user-configurable preferences, and add persisted *Session History*. No new pipeline behavior — this phase is entirely about surfacing and storing configuration.

Visual design follows [docs/design/VISUAL_STYLE.md](../design/VISUAL_STYLE.md): standard native window chrome, monochrome content, the single accent color reserved for at most one primary action per view (e.g. "Save"/"Test Key") — not for section headers, icons, or general UI.

## Todos

**Settings Window shell** ([#36](https://github.com/nitesw/VoiceDrop_App/issues/36))
- [ ] Build the *Settings Window* (SwiftUI), reachable from the *Menu Bar Icon*'s "Settings..." item
- [ ] (Optional) a dedicated hotkey to open Settings directly, per earlier discussion
- [ ] Tabbed/sectioned layout: General, Cleanup, Vocabulary, Cloud, History — or equivalent grouping
- [ ] Minimal, monochrome layout throughout — no decorative icons/color used just to differentiate sections; rely on native section/tab chrome instead

**Hotkey & startup** ([#37](https://github.com/nitesw/VoiceDrop_App/issues/37))
- [ ] Push-to-Talk Hotkey rebinding UI, with conflict detection against existing system/app shortcuts
- [ ] F-row/media-key handling (discovered in Phase 1: bare F5 didn't reach `CGEventTap` at all on this dev machine, and turned out to be bound to the system Siri/Dictation shortcut). Two distinct constraints, only one of which we can actually detect:
  - **Standard-function-keys mode** — whether "Use F1, F2, etc. as standard function keys" is enabled in System Settings → Keyboard. This *is* checkable (`defaults read -g com.apple.keyboard.fnState` or equivalent) — if a user picks an F-row key in the rebinding UI while this is off, detect it and prompt them to enable it or pick a different key.
  - **System shortcut reservations** — individual keys can be bound to a system action (Mission Control, Siri, brightness, etc.) in System Settings → Keyboard → Keyboard Shortcuts, consumed by the OS before any third-party tap ever sees them. There's no public API to query which key a given system shortcut currently occupies, so this is **not programmatically detectable** — accepted limitation. The only fallback UX is: if a newly-bound hotkey silently never fires despite the tap being armed, show generic guidance ("this key may be reserved by a system shortcut — check System Settings → Keyboard Shortcuts") rather than trying to pinpoint the conflict automatically
- [ ] *Launch at Login* toggle using `SMAppService`
- [ ] Persist both settings; confirm they survive app relaunch

**HUD position** ([#38](https://github.com/nitesw/VoiceDrop_App/issues/38))
- [ ] Position picker UI (near cursor / bottom of screen / other screen edges) wired to the HUD positioning support built in Phase 4 — as of Phase 4, `DictationHUD.swift`'s `HUDPosition` enum only implements `.bottomCenter`, and nothing reads any position preference (there's no persisted config yet). This phase needs to both add the other position cases to the enum *and* build the actual config-read/persist plumbing — Phase 4 only got as far as the enum existing, not real plumbing
- [ ] Live preview when changing position (show the HUD briefly at the new location)

**Cleanup configuration** ([#39](https://github.com/nitesw/VoiceDrop_App/issues/39))
- [ ] *Cleanup Strength* selector (verbatim-clean / light-edit / formal-rewrite), wired to the Phase 3 provider interface
- [ ] Cleanup provider selector, three clearly labeled options per [ADR-0008](../adr/0008-local-cleanup-in-process-again.md) (naming matters here — don't blur these together): **"None"** (no processing) / **"Built-in"** (self-contained local model, downloads automatically, no external app — this is `Local`/llama.cpp) / **"Custom endpoint"** (BYO via Ollama, LM Studio, vLLM, or a cloud API — this is `Cloud`'s free-form URL). Apple Foundation Models is a fourth option on macOS only. `None` is a real, fully-supported choice (no model download, no cloud key), not just an edge case to tolerate
- [ ] "Custom endpoint" config is a **free-form base URL** field + API key field, not a vendor dropdown — per [ADR-0005](../adr/0005-cleanup-pass-optional-and-free-form-endpoint.md), assumed OpenAI-compatible (`voicedrop_engine_set_cleanup_cloud_config`); works against hosted providers, OpenRouter, or a local server (Ollama/LM Studio/vLLM) alike. When pointed at Ollama, prefill the model field from `voicedrop_ollama_model_*` suggestions. API key stored in Keychain (never plaintext on disk)
- [ ] Validate the entered URL/key (e.g. a lightweight test call) and show clear success/failure feedback

**Model picker** ([#69](https://github.com/nitesw/VoiceDrop_App/issues/69)) (backing Rust plumbing already built in Phase 3 — `core/src/models.rs`'s `voicedrop_model_*` FFI, see [ADR-0008](../adr/0008-local-cleanup-in-process-again.md))
- [ ] For "Built-in": dropdown listing `voicedrop_model_catalog_*` entries (one Whisper/STT model, several Cleanup Pass candidates: Qwen2.5-0.5B/1.5B, Llama-3.2-3B — filter by `voicedrop_model_catalog_kind`), showing name + approximate size + downloaded/not-downloaded state (`voicedrop_model_is_downloaded`)
- [ ] "Download" button per not-yet-downloaded entry, calling `voicedrop_model_download` **off the main thread** — it blocks for the whole transfer (same rule as any other long-running core call, see the CGEventTap lesson in `HotkeyMonitor.swift`) — with a progress bar driven by the `on_progress` callback
- [ ] "Delete" button per downloaded entry, calling `voicedrop_model_delete`; confirm before deleting the model currently in use
- [ ] Selecting a model calls `voicedrop_model_path_for` then `voicedrop_engine_set_model_path`/`voicedrop_engine_set_cleanup_local_model_path` with the result
- [ ] Handle "selected model isn't downloaded yet" — prompt to download rather than silently failing at the next Dictation Session. Carried over from Phase 4's first-run-provisioning todo: this is also where the local Cleanup Pass GGUF download-on-selection belongs — Phase 4 only gates the Whisper model before the hotkey is armed, and there was no provider-selection UI yet for a Cleanup Pass model to be triggered from. Now that this picker is the actual selection point, use it: picking "Built-in" for the first time (or picking a not-yet-downloaded catalog entry) should trigger the same visible-progress download flow as Whisper's, scoped to just that model — not bundled into the app-launch gate, since `None`/cloud users should never see it
- [ ] For "Custom endpoint" pointed at Ollama specifically: suggest names from `voicedrop_ollama_model_*` (Ollama itself handles the actual `ollama pull`, VoiceDrop doesn't manage that download)

**Word blocklist** ([#70](https://github.com/nitesw/VoiceDrop_App/issues/70)) (backing Rust plumbing already built in Phase 3 — `voicedrop_engine_set_blocklist`, `core/src/blocklist.rs`)
- [ ] Editable list UI for custom "always remove" words, shown alongside (but visually distinct from) *Custom Vocabulary* below — opposite purposes, easy to conflate if not labeled clearly
- [ ] Show the built-in default words exist (without necessarily listing profanity in the UI by default) and that custom words are additive, not a replacement

**Custom Vocabulary** ([#40](https://github.com/nitesw/VoiceDrop_App/issues/40))
- [ ] Editable list UI: add/remove words or phrases
- [ ] Wire the list into both the STT bias hook (Phase 2) and the Cleanup Pass prompt (Phase 3)
- [ ] Persist the list locally

**Session History** ([#41](https://github.com/nitesw/VoiceDrop_App/issues/41))
- [ ] Local persistence layer for *Session History* (e.g. SQLite, or a simpler flat-file store behind a Rust core interface) — store Raw Transcript, Cleaned Transcript, timestamp per session
- [ ] Write a history entry for every completed session (excluding sessions discarded via a "scratch that" *Voice Command*, per Phase 6)
- [ ] History view UI: chronological list, re-copy action per entry
- [ ] Clear-history action (single entry and clear-all)
- [ ] Decide and document a retention policy (unbounded vs. capped count/age) before this ships

## Done when

- Every preference introduced conceptually in earlier phases is editable in the Settings Window and persists across restarts
- Session History records every real session and supports review, re-copy, and clearing
- Cloud API key is stored securely and validated before use
