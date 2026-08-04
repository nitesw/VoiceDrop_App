# VoiceDrop — Context

## Domain Glossary

**Dictation Session**
The full unit of interaction: from the user pressing the Push-to-Talk Hotkey down, through speaking, releasing the key, and processing, ending when a Cleaned Transcript is produced (or the session is cancelled/errors out).

**Push-to-Talk Hotkey**
The global, system-wide key combo the user holds down to record and releases to stop. Works regardless of which app is focused. Configured in the Settings Window. (The Menu Bar Icon is not a recording trigger — see Menu Bar Icon.)

**Menu Bar Icon**
The persistent menu bar presence for VoiceDrop. A single click opens a dropdown with an Enable/Disable toggle for the app, access to the Settings Window, and Quit. It is not a recording trigger — recording is exclusively via the Push-to-Talk Hotkey.

**Raw Transcript**
The unprocessed output of the speech-to-text (STT) engine for a Dictation Session — before filler-word removal, punctuation, or grammar correction.

**Cleanup Pass**
The LLM step that transforms a Raw Transcript into a Cleaned Transcript: strips filler words/disfluencies (e.g. "uh", "um"), adds punctuation, and corrects grammar.

**Cleaned Transcript**
The final output text of a Dictation Session, after the Cleanup Pass. This is what gets delivered to the Injection Target.

**Injection Target**
The focused text field/app the Cleaned Transcript is inserted into automatically, at the current cursor position within that field. Determined by whatever had OS input focus when the Dictation Session began. If injection isn't possible or safe (focus changed, secure field, target rejects input), VoiceDrop falls back to copying the Cleaned Transcript to the clipboard and shows a notice on the Dictation HUD — the transcript is never silently lost.

**Custom Vocabulary**
A user-maintained list of words/phrases (names, product terms, acronyms) fed as a bias/hint to the STT engine and/or Cleanup Pass to improve recognition accuracy on terms the base model wouldn't otherwise get right.

**Voice Command**
A short, fixed set of spoken instructions (e.g. "scratch that" to discard the session, "new paragraph" to insert a break) recognized by the Cleanup Pass LLM from context — distinguished from literal dictated content by whether it reads as a standalone instruction rather than part of the sentence. Not detected by exact phrase-matching.

**Session History**
A local, persistent log of past Dictation Sessions (Raw Transcript, Cleaned Transcript, timestamp), kept so the user can review, re-copy, or recover text if injection failed or landed in the wrong Injection Target. Stored locally only; user-clearable.

**Dictation HUD**
A small floating pill-shaped overlay shown during a Dictation Session, displaying a waveform while recording and a processing indicator while the Cleanup Pass runs. Its screen position (near cursor, bottom of screen, or other screen edges) is a user preference.

**Cleanup Strength**
A global preference (not per-session) controlling how aggressive the Cleanup Pass is. Levels: verbatim-clean (disfluency + grammar only, preserves original wording/structure), light-edit (also merges fragments, tightens wordy phrasing), and formal-rewrite (heavier restructuring). Changed via the Settings Window, not mid-session.

**Settings Window**
The single in-app preferences window for VoiceDrop — the canonical term; not to be confused with system-level OS preferences (e.g. Accessibility permissions), which live outside the app. Reached from the Menu Bar Icon (and optionally its own hotkey). Contains: the Push-to-Talk Hotkey binding, Launch at Login toggle, Dictation HUD position, Cleanup Strength, Custom Vocabulary, cloud opt-in/API key entry, and the Session History view.

**Launch at Login**
A preference in the Settings Window that starts VoiceDrop automatically on system startup, so the Push-to-Talk Hotkey is always available without the user manually opening the app.
