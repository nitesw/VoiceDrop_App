# Phase 4 — macOS Shell: Core Interaction Loop & Injection

Tracks [GitHub issue #5](https://github.com/nitesw/VoiceDrop_App/issues/5). Depends on [0004-phase3-cleanup-pass.md](0004-phase3-cleanup-pass.md) (headless pipeline complete). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md).

Scope: give the pipeline a face. This is the first phase where VoiceDrop is usable end-to-end on macOS — hold hotkey, see feedback, get text injected.

Visual design for every surface below follows [docs/design/VISUAL_STYLE.md](../design/VISUAL_STYLE.md): monochrome (black/white/gray) with a single accent color reserved for the active/recording indicator, native macOS corner radius (no custom radius), no decorative chrome.

## Todos

**First-run model provisioning** ([#67](https://github.com/nitesw/VoiceDrop_App/issues/67)) (tracks the Swift-side gap left open by [ADR-0004](../adr/0004-whisper-model-download-on-first-run.md) and [ADR-0005](../adr/0005-cleanup-pass-optional-and-free-form-endpoint.md))
- [ ] On launch, check whether the Whisper model exists at the core's default path (`voicedrop_engine_set_model_path`'s default); if not, download it before the *Push-to-Talk Hotkey* is armed — the hotkey must not be usable while STT has no model to run
- [ ] Show visible download progress (reuse the *Dictation HUD* chrome, or a dedicated first-run screen — decide which); no silent multi-hundred-MB download with no feedback
- [ ] Surface a clear, retryable error if the download fails (bad network, disk full) rather than hanging or crashing
- [ ] Separately, if the user selects the local Cleanup Pass provider (Phase 3, [ADR-0005](../adr/0005-cleanup-pass-optional-and-free-form-endpoint.md)), trigger that GGUF model's download the same way, at the point of selection — not bundled into the same first-run gate, since `None`/cloud users should never see it

**Menu Bar Icon** ([#31](https://github.com/nitesw/VoiceDrop_App/issues/31))
- [ ] Persistent *Menu Bar Icon* using `NSStatusItem`, as a monochrome template image (auto-adapts to light/dark menu bar, per [VISUAL_STYLE.md](../design/VISUAL_STYLE.md))
- [ ] Single click opens a dropdown: Enable/Disable toggle, "Settings...", "Quit"
- [ ] Icon reflects enabled/disabled state via opacity or a minimal glyph change — not by introducing color
- [ ] Disabling the app suspends the *Push-to-Talk Hotkey* listener entirely (confirm hotkey does nothing while disabled)

**Dictation HUD** ([#32](https://github.com/nitesw/VoiceDrop_App/issues/32))
- [ ] Floating pill-shaped overlay window (SwiftUI, borderless `NSPanel` or similar, always-on-top), corner radius matching macOS's native default — no custom radius
- [ ] Monochrome throughout; the single accent color is reserved for the recording-state waveform only
- [ ] Recording state: waveform visualization (accent color) driven by live audio levels from the Rust core
- [ ] Processing state: spinner/progress indicator, grayscale — no color introduced here
- [ ] "No speech detected" state: brief notice, then auto-dismiss — grayscale, no color
- [ ] Injection-fallback state: brief "copied to clipboard" notice, then auto-dismiss — grayscale, no color
- [ ] Position picker plumbing: HUD reads its screen position (near cursor / bottom of screen / other edges) from config — the picker UI itself is Phase 5, but the HUD must already support being positioned anywhere
- [ ] HUD never steals focus from the *Injection Target* app
- [ ] Keep it minimal: no element that doesn't communicate state or enable an action — if in doubt, cut it

**Text injection** ([#33](https://github.com/nitesw/VoiceDrop_App/issues/33))
- [ ] Capture which app/field had focus at `Idle → Recording` transition — this is the *Injection Target*, fixed at session start even if focus later changes
- [ ] Insert the *Cleaned Transcript* at the current cursor position in the Injection Target (via Accessibility API text insertion, or synthetic paste — pick one and document why)
- [ ] Detect secure/password fields at session start and skip injection entirely for those (goes straight to clipboard fallback — full secure-field policy is Phase 6, but the detection hook belongs here)
- [ ] Detect focus changed since session start (user switched apps mid-processing) and treat as an injection-fallback case
- [ ] Detect injection failure (target app rejects synthetic input) and fall back gracefully

**Injection fallback** ([#34](https://github.com/nitesw/VoiceDrop_App/issues/34))
- [ ] On any injection failure/unsafe condition: copy *Cleaned Transcript* to clipboard instead
- [ ] Show the fallback notice on the *Dictation HUD*
- [ ] Confirm the transcript is never silently lost — every path either injects or lands on the clipboard

**Full loop wiring** ([#35](https://github.com/nitesw/VoiceDrop_App/issues/35))
- [ ] Hold hotkey → HUD shows recording + waveform → release → HUD shows processing → Rust core runs STT + Cleanup Pass → inject or fallback → HUD dismisses
- [ ] End-to-end manual test across a handful of real apps (e.g. TextEdit, Notes, a browser text field, Terminal)

## Done when

- On first run, the Whisper model downloads with visible progress before the hotkey becomes usable, and failures are surfaced clearly rather than silently hanging
- A user can hold the hotkey anywhere, speak, release, and see the Cleaned Transcript appear at their cursor within a few seconds
- The HUD accurately reflects every session state (recording, processing, no-speech, fallback)
- Injection failure never loses the transcript — it always lands on the clipboard instead
- Disabling via the Menu Bar Icon fully suspends the hotkey
