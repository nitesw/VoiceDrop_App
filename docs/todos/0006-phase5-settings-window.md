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
- [ ] *Launch at Login* toggle using `SMAppService`
- [ ] Persist both settings; confirm they survive app relaunch

**HUD position** ([#38](https://github.com/nitesw/VoiceDrop_App/issues/38))
- [ ] Position picker UI (near cursor / bottom of screen / other screen edges) wired to the HUD positioning support built in Phase 4
- [ ] Live preview when changing position (show the HUD briefly at the new location)

**Cleanup configuration** ([#39](https://github.com/nitesw/VoiceDrop_App/issues/39))
- [ ] *Cleanup Strength* selector (verbatim-clean / light-edit / formal-rewrite), wired to the Phase 3 provider interface
- [ ] Cleanup provider selector: local llama.cpp vs. Apple Foundation Models (macOS only) vs. cloud
- [ ] Cloud opt-in toggle + API key entry field, key stored in Keychain (never plaintext on disk)
- [ ] Validate the entered key (e.g. a lightweight test call) and show clear success/failure feedback

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
