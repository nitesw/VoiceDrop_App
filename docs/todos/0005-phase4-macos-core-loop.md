# Phase 4 — macOS Shell: Core Interaction Loop & Injection

Tracks [GitHub issue #5](https://github.com/nitesw/VoiceDrop_App/issues/5). Depends on [0004-phase3-cleanup-pass.md](0004-phase3-cleanup-pass.md) (headless pipeline complete). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md).

Scope: give the pipeline a face. This is the first phase where VoiceDrop is usable end-to-end on macOS — hold hotkey, see feedback, get text injected.

Visual design for every surface below follows [docs/design/VISUAL_STYLE.md](../design/VISUAL_STYLE.md): monochrome (black/white/gray) with a single accent color reserved for the active/recording indicator, native macOS corner radius (no custom radius), no decorative chrome.

## Todos

**First-run model provisioning** ([#67](https://github.com/nitesw/VoiceDrop_App/issues/67)) (tracks the Swift-side gap left open by [ADR-0004](../adr/0004-whisper-model-download-on-first-run.md) and [ADR-0005](../adr/0005-cleanup-pass-optional-and-free-form-endpoint.md))
- [x] On launch, check whether the Whisper model exists at the core's default path (`voicedrop_engine_set_model_path`'s default); if not, download it before the *Push-to-Talk Hotkey* is armed — the hotkey must not be usable while STT has no model to run (`main.swift`'s `provisionModelThenArm`, `ModelProvisioner.swift`)
- [x] Show visible download progress (reuse the *Dictation HUD* chrome, or a dedicated first-run screen — decide which); no silent multi-hundred-MB download with no feedback (`.downloadingModel(progress:)` HUD state)
- [x] Surface a clear, retryable error if the download fails (bad network, disk full) rather than hanging or crashing (5 retries with backoff, then an error HUD state)
- [ ] **Moved to [Phase 5's model picker](0006-phase5-settings-window.md)**: triggering the local Cleanup Pass GGUF download at the point of selection. There's no provider-selection UI yet in Phase 4 — only a dev env var — so "at the point of selection" has nothing to attach to until Phase 5's model picker exists

**Menu Bar Icon** ([#31](https://github.com/nitesw/VoiceDrop_App/issues/31))
- [x] Persistent *Menu Bar Icon* using `NSStatusItem`, as a monochrome template image (auto-adapts to light/dark menu bar, per [VISUAL_STYLE.md](../design/VISUAL_STYLE.md)) — glyph source is `assets/VoiceDrop.svg`, pre-converted to `macos/Sources/VoiceDrop/Resources/MenuBarIcon.png`/`@2x`/`@3x`. **Known blocker resolved**: `MenuBarIconLoader.swift` bypasses `Bundle.module`, loading the PNGs by absolute path from `Bundle.main.resourcePath`; `scripts/build-macos-app.sh` copies them straight into `Contents/Resources/` before codesigning
- [~] Single click opens a dropdown: Enable/Disable toggle, "Settings...", "Quit" — Enable/Disable and Quit work; "Settings..." is present but disabled/dead (no Settings window exists yet — that's Phase 5), so leaving unchecked until it's wired to something
- [x] Icon reflects enabled/disabled state via opacity or a minimal glyph change — not by introducing color (`appearsDisabled` toggle, no color)
- [x] Disabling the app suspends the *Push-to-Talk Hotkey* listener entirely (confirm hotkey does nothing while disabled)
- [x] Disabling is a real resource kill switch, not just a hotkey gate — `HotkeyMonitor.setEnabled` now actually stops the `CGEventTap` (`CGEvent.tapEnable(tap:enable:false)`) rather than just gating `handle()`, and calls the new `voicedrop_engine_unload_models` FFI to drop cached Whisper/Cleanup Pass models from memory. One accepted limitation: if a session is already Recording/Processing when disabled, its models stay loaded until that session finishes — the Rust core's state machine only allows `Reset` from a terminal state, so there's no mid-session abort without a larger change there (logged, not silent)

**Dictation HUD** ([#32](https://github.com/nitesw/VoiceDrop_App/issues/32))
- [x] Floating pill-shaped overlay window (SwiftUI, borderless `NSPanel` or similar, always-on-top), corner radius matching macOS's native default — no custom radius (`.borderless`/`.nonactivatingPanel`, `.level = .floating`, native `Capsule()`)
- [x] Monochrome throughout — **decided**: the waveform is pure white too, no accent color anywhere in the HUD. Supersedes this section's original "accent reserved for the waveform" line from [VISUAL_STYLE.md](../design/VISUAL_STYLE.md); worth updating that doc to match if it isn't already
- [x] Recording state: waveform visualization (accent color) driven by live audio levels from the Rust core (30Hz polling of `voicedrop_engine_current_input_level`)
- [x] Processing state: spinner/progress indicator, grayscale — no color introduced here
- [x] "No speech detected" state: brief notice, then auto-dismiss — grayscale, no color
- [x] Injection-fallback state: brief "copied to clipboard" notice, then auto-dismiss — grayscale, no color
- [ ] Position picker plumbing: HUD reads its screen position (near cursor / bottom of screen / other edges) from config — the picker UI itself is Phase 5, but the HUD must already support being positioned anywhere. `HUDPosition` enum exists with only `.bottomCenter` implemented, and nothing reads a position from config yet — no plumbing in place
- [x] HUD never steals focus from the *Injection Target* app (`.nonactivatingPanel`, `ignoresMouseEvents = true`)
- [x] Keep it minimal: no element that doesn't communicate state or enable an action — if in doubt, cut it

**Text injection** ([#33](https://github.com/nitesw/VoiceDrop_App/issues/33))
- [x] Capture which app/field had focus at `Idle → Recording` transition — this is the *Injection Target*, fixed at session start even if focus later changes (`TextInjector.captureCurrentTarget()`, called from `handleKeyDown`)
- [x] Insert the *Cleaned Transcript* at the current cursor position in the Injection Target (via Accessibility API text insertion, or synthetic paste — pick one and document why) — AX insertion first (`tryDirectInsertion`), auto-paste fallback for apps with incomplete AX trees, both documented in `TextInjector.swift`
- [x] Detect secure/password fields at session start and skip injection entirely for those — **decided**: goes further than "clipboard fallback," discards the text entirely (never touches the clipboard, which is just as readable as a direct paste for a password field) and shows a dedicated `.discarded` HUD notice instead, so the user still sees confirmation without the text landing anywhere. `isSecureField` (via `kAXSecureTextFieldSubrole`) + `HotkeyMonitor`'s `.discarded` case already implement exactly this
- [x] Detect focus changed since session start (user switched apps mid-processing) and treat as an injection-fallback case
- [x] Detect injection failure (target app rejects synthetic input) and fall back gracefully

**Injection fallback** ([#34](https://github.com/nitesw/VoiceDrop_App/issues/34))
- [x] On any injection failure/unsafe condition: copy *Cleaned Transcript* to clipboard instead
- [x] Show the fallback notice on the *Dictation HUD*
- [x] Confirm the transcript is never silently lost — **decided reading**: "never lost" means the user is always told what happened, not literally "always clipboard." The secure-field path discards instead of copying, but shows a visible `.discarded` notice — so nothing vanishes without the user knowing

**Full loop wiring** ([#35](https://github.com/nitesw/VoiceDrop_App/issues/35))
- [x] Hold hotkey → HUD shows recording + waveform → release → HUD shows processing → Rust core runs STT + Cleanup Pass → inject or fallback → HUD dismisses (fully traced end-to-end in `HotkeyMonitor.swift`)
- [x] End-to-end manual test across a handful of real apps (e.g. TextEdit, Notes, a browser text field, Terminal) — confirmed across Terminal (iTerm2), Electron apps (e.g. Antigravity), TextEdit/Notes/browser, plus Finder/Photos/System Settings for the clipboard-fallback path

## Done when

- On first run, the Whisper model downloads with visible progress before the hotkey becomes usable, and failures are surfaced clearly rather than silently hanging
- A user can hold the hotkey anywhere, speak, release, and see the Cleaned Transcript appear at their cursor within a few seconds
- The HUD accurately reflects every session state (recording, processing, no-speech, fallback)
- Injection failure never loses the transcript — it always lands on the clipboard instead
- Disabling via the Menu Bar Icon fully suspends the hotkey
