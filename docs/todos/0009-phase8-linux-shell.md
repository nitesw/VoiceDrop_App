# Phase 8 — Linux Shell Development

Tracks [GitHub issue #9](https://github.com/nitesw/VoiceDrop_App/issues/9). Depends on the macOS app being feature-complete through [0007-phase6-robustness.md](0007-phase6-robustness.md); can run in parallel with [0008-phase7-windows-shell.md](0008-phase7-windows-shell.md) since both consume the same core independently. Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md).

Scope: bring the full feature set to Linux. This is the platform most likely to need scope trade-offs (X11 vs. Wayland) — call those out explicitly rather than silently degrading functionality.

## Todos

**Project setup** ([#51](https://github.com/nitesw/VoiceDrop_App/issues/51))
- [ ] Create the GTK4 shell project consuming `voicedrop-core` via FFI
- [ ] Confirm the Rust core builds on Linux (audio via `cpal`/ALSA/PulseAudio/PipeWire, whisper.cpp, llama.cpp)
- [ ] Basic CI: Linux build added alongside existing CI

**Display server scoping (do this before building the rest)** ([#51](https://github.com/nitesw/VoiceDrop_App/issues/51))
- [ ] Determine and document what's actually supportable on X11 vs. Wayland for: global hotkey capture, text injection, and always-on-top overlay windows
- [ ] Decide the initial target: X11-first with best-effort Wayland, or Wayland-first via portals with reduced functionality — write this down, since it constrains everything below
- [ ] If Wayland has hard limitations (e.g. no arbitrary global hotkeys without compositor support, restricted synthetic input), document exactly what degrades and how the user is informed

**Tray icon equivalent** ([#52](https://github.com/nitesw/VoiceDrop_App/issues/52))
- [ ] Tray icon via `libappindicator` (or the current recommended equivalent, since this ecosystem shifts) with the same dropdown: Enable/Disable, Settings, Quit
- [ ] Confirm tray icon actually renders across at least the major desktop environments in scope (e.g. GNOME, KDE) — GNOME in particular has historically needed an extension for tray icons

**Hotkey & injection** ([#53](https://github.com/nitesw/VoiceDrop_App/issues/53))
- [ ] Global hotkey capture per the X11/Wayland decision above, driving the same *Dictation Session* state machine
- [ ] Text injection: `xdotool`-style synthetic input on X11; portal-based (more limited) on Wayland — implement per the scoping decision, with clipboard fallback as the universal safety net regardless of platform limitations
- [ ] Secure-field detection equivalent, to the extent the display server exposes this information

**Dictation HUD** ([#54](https://github.com/nitesw/VoiceDrop_App/issues/54))
- [ ] Native overlay window reproducing the same states as macOS/Windows
- [ ] Position picker parity, accounting for any multi-monitor/compositor quirks

**Settings Window & History** ([#55](https://github.com/nitesw/VoiceDrop_App/issues/55))
- [ ] Port the full Settings Window feature set (same list as Windows: hotkey, Launch at Login, HUD position, Cleanup Strength, provider selection, Custom Vocabulary, cloud key, Session History)
- [ ] Launch at Login via `.desktop` autostart entry
- [ ] Session History persistence, reusing the core-backed storage layer

## Done when

- Feature parity with macOS/Windows on at least the primary supported display server (per the scoping decision above)
- Any deliberately unsupported or degraded functionality on the secondary display server is documented, not silently broken
- Manual QA pass on at least one X11 and one Wayland desktop environment
