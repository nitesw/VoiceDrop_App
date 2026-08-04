# Direct-download (notarized) distribution on macOS, not App Store

VoiceDrop's core interaction — a global push-to-talk hotkey and auto-injection into whatever app has focus — depends on macOS Accessibility APIs and unrestricted global hotkey registration. The Mac App Store requires apps to run under the App Sandbox, which blocks or severely restricts exactly these APIs. We considered App Store distribution for the discoverability/trust benefit, but the sandbox would break the app's core interaction model, not just a peripheral feature.

Decided: VoiceDrop ships as a notarized `.app`/`.dmg` distributed directly (outside the App Store), following the same model as Raycast, Rectangle, and CleanShot X.

## Consequences

- Users must manually grant Accessibility/Input Monitoring permissions on first run (no App Store install-time trust); onboarding needs to walk them through this explicitly.
- No App Store discoverability or built-in update mechanism — VoiceDrop needs its own update channel (e.g. Sparkle) and its own distribution/marketing path.
