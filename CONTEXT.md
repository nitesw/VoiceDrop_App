# VoiceDrop — Context

## Domain Glossary

**Dictation Session**
The full unit of interaction: from the user pressing the Push-to-Talk Hotkey, through speaking, releasing the key and transcription, ending when a Transcript is produced and delivered (or the session finds no speech, or errors out).

**Push-to-Talk Hotkey**
⌥ Space. Global and system-wide: works regardless of which app is focused. By default it's held: recording runs while it's held, and releasing it stops recording and starts transcription. With the **Recording Mode** set to tap-to-toggle, one tap starts and the next tap stops. Hardcoded, not rebindable (ADR-0009).

**Recording Mode**
Hold (default) or tap-to-toggle, chosen from the Menu Bar Icon and remembered. Only changes how the Push-to-Talk Hotkey starts and stops a recording.

**Language Hotkey**
⌥ ⇧ Space, tapped. Cycles the Dictation Language. Matched with exact modifiers, so it never starts a recording, and pressing Shift during a recording is ignored.

**Dictation Language**
The language Whisper transcribes in: English (default), Ukrainian, French, or Auto-detect. Remembered across launches. Applied to the engine at the start of each recording. On Auto-detect, clips under 2 seconds use English instead, since detection is unreliable on very short audio.

**Transcript**
Whisper's output for a Dictation Session: the text that gets delivered. There is no separate cleanup step (ADR-0009), so this is the final text.

**Injection Target**
The focused text field/app the Transcript is inserted into, at the cursor position in that field. Fixed at the moment the session began. Delivery is tried in order: direct Accessibility insertion, then auto-paste, then clipboard with a "Copied to clipboard" notice on the Dictation HUD. Secure (password) fields are the exception: the Transcript is discarded, never pasted or copied, and the HUD says so. Either way the user is always told what happened.

**Dictation HUD**
A small floating pill-shaped overlay at the bottom center of the screen. Shows a live waveform while recording, a processing indicator while transcribing, and brief notices (no speech detected, copied to clipboard, secure field, language switched, errors). Never takes focus or intercepts clicks. Visual treatment follows [docs/design/VISUAL_STYLE.md](docs/design/VISUAL_STYLE.md).

**Menu Bar Icon**
The persistent menu bar presence. A click opens a dropdown with Enabled (a real kill switch: stops the hotkeys and unloads the model; a recording in progress is finished first), Tap ⌥Space to Start/Stop (the Recording Mode), Launch at Login, and Quit. It is not a recording trigger.

**Launch at Login**
A menu toggle that starts VoiceDrop automatically at login, so the hotkeys are always available.

## Deferred terms

Defined during the original, larger plan and removed or never built after ADR-0009. Kept so older docs stay readable.

- **Cleanup Pass / Cleaned Transcript / Cleanup Strength**: an LLM step that removed filler words and fixed grammar after speech-to-text, at a chosen strength. Built in Phase 3, then removed.
- **Custom Vocabulary**: a user list of names and terms fed to Whisper as a hint. Its FFI was removed; the hint mechanism is Whisper's initial prompt.
- **Voice Command**: spoken instructions like "scratch that" or "new paragraph", recognized by the Cleanup Pass.
- **Session History**: a local log of past transcripts for review and re-copy.
- **Settings Window**: a preferences window. VoiceDrop has none by design; everything is a hotkey or a menu item.
