# VoiceDrop — Visual Style

Single source of truth for VoiceDrop's visual language. Every UI surface (*Dictation HUD*, *Menu Bar Icon*, *Settings Window*, and their per-platform equivalents) follows this — referenced from the relevant phase todo docs rather than restated per-platform.

## Palette

- **Monochrome only**: black and white (plus grays for depth/disabled states). No multi-color iconography, no gradients-as-decoration.
- **One accent color**, used sparingly — only for the single active/recording state indicator (e.g. the waveform while recording) and any primary call-to-action. Not used for general chrome, text, or backgrounds.
- Must hold up in both light and dark system appearance — verify both, don't just design for one and assume the other inverts cleanly.

## Shape & chrome

- Minimal: no unnecessary borders, drop shadows, or decorative elements. If an element doesn't communicate state or enable an action, cut it.
- Corner radius follows macOS's native default (standard `NSVisualEffectView`/window corner radius) on **every** platform — Windows and Linux shells use this same radius rather than their own native default, so VoiceDrop looks and feels consistent regardless of platform. Do not invent a custom radius beyond this.
- No custom fonts/icon sets that fight the OS's native look — use system fonts and SF Symbols (or the equivalent native icon set per platform) wherever possible.

## Application

- **Dictation HUD**: monochrome pill, native corner radius, one accent color reserved for the waveform/recording indicator. Processing/no-speech/fallback states are communicated via shape and grayscale (e.g. spinner, icon swap), not additional colors.
- **Menu Bar Icon / tray icon**: monochrome template icon (adapts automatically to light/dark menu bar), state changes (enabled/disabled) shown via opacity or a minimal glyph change — not by introducing color.
- **Settings Window**: standard native window chrome, monochrome content, accent color reserved for one primary action per view at most (e.g. a "Save"/"Test Key" button), not for section headers or general UI.

## Non-goals

- No theming/skinning system in the initial build — one look, applied consistently. Revisit only if there's a real user request for it later.
