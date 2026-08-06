# Phase 6 — Robustness: Voice Commands, Errors & Onboarding

Tracks [GitHub issue #7](https://github.com/nitesw/VoiceDrop_App/issues/7). Depends on [0006-phase5-settings-window.md](0006-phase5-settings-window.md). Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md).

Scope: take the macOS app from "works on the happy path" to "safe to hand to a real user." This is the last macOS-specific phase before cross-platform work begins.

## Todos

**Ad-hoc code signing causes intermittent hotkey failures after rebuild** ([#71](https://github.com/nitesw/VoiceDrop_App/issues/71)) (confirmed during Phase 3 manual testing, not yet a tracked issue — flagging here since it's exactly the risk [0002-phase1-audio-hotkey.md](0002-phase1-audio-hotkey.md) predicted)
- [ ] Switch `scripts/build-macos-app.sh`'s ad-hoc `codesign --sign -` to a self-signed Keychain certificate with a stable identity. Observed failure mode: after `./scripts/build-macos-app.sh` re-signs the binary, the *first* launch sometimes arms the hotkey (logs "Control+Option+D hotkey armed") but never delivers key events — holding the hotkey produces no "Recording started" log at all. Killing and relaunching the same build fixes it. Root cause is believed to be TCC (Accessibility/Input Monitoring permission) keying its grant off the ad-hoc signature hash, which changes on every rebuild — the grant is technically valid but TCC's cache hasn't caught up by the time the tap is created. A stable signing identity means the hash doesn't change between rebuilds, so this class of flakiness should disappear
- [ ] Once fixed, re-verify: rebuild, launch once, hotkey should work on the very first hold — no "kill and relaunch" workaround needed

**Voice Commands** ([#42](https://github.com/nitesw/VoiceDrop_App/issues/42))
- [ ] Extend the Cleanup Pass prompt to recognize the fixed *Voice Command* set (e.g. "scratch that", "new paragraph") from context, not exact phrase-matching
- [ ] "Scratch that": discards the session entirely — no injection, no clipboard fallback, no *Session History* entry
- [ ] "New paragraph" (and any other formatting commands in the fixed set): inserted as the appropriate structural break in the Cleaned Transcript, not as literal text
- [ ] Write test cases where the trigger phrase is spoken as genuine content (e.g. "scratch that itch") and confirm it is NOT treated as a command
- [ ] Document the final fixed command list in [CONTEXT.md](../../CONTEXT.md) once locked in

**Secure fields & focus edge cases** ([#43](https://github.com/nitesw/VoiceDrop_App/issues/43))
- [ ] Confirm secure-field detection (from Phase 4) covers the real cases: password fields, and any other OS-flagged sensitive input types
- [ ] Target app closed entirely mid-processing: confirm this is treated as an injection-fallback case, not a crash
- [ ] Target app force-quit or system sleep mid-recording: confirm the session transitions to `Error`/discarded cleanly rather than hanging

**Inference failure handling** ([#44](https://github.com/nitesw/VoiceDrop_App/issues/44))
- [ ] STT failure (e.g. Whisper crashes or times out): surface as a session error, HUD shows a brief failure notice, no partial/garbled injection
- [ ] Cleanup Pass failure (any provider): same — no injection of a Raw Transcript pretending to be cleaned
- [ ] Define and implement a reasonable timeout per stage (STT, Cleanup Pass) so a hung model doesn't leave the HUD stuck indefinitely

**First-run onboarding** ([#45](https://github.com/nitesw/VoiceDrop_App/issues/45))
- [ ] Detect first launch (no prior permissions granted) and show an onboarding flow
- [ ] Clearly explain why Accessibility and Input Monitoring permissions are required (global hotkey + text injection) before prompting
- [ ] Deep-link or guide the user to System Settings if permissions aren't granted
- [ ] Detect permission revocation after initial grant (user turns it off later) and surface that state clearly rather than failing silently on the next hotkey press

## Done when

- The fixed Voice Command set works reliably and doesn't false-trigger on natural speech containing the same words
- No failure mode (inference error, target app closing, permission revocation) results in a crash, a hang, or a silently lost transcript
- A fresh install walks a new user through granting the permissions the app actually needs, with a clear explanation
