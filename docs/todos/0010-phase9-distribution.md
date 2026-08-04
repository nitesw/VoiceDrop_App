# Phase 9 — Distribution, Packaging & Auto-Updates

Tracks [GitHub issue #10](https://github.com/nitesw/VoiceDrop_App/issues/10). Depends on all three shells being feature-complete ([0007](0007-phase6-robustness.md), [0008](0008-phase7-windows-shell.md), [0009](0009-phase8-linux-shell.md)). Architecture rationale: [ADR-0003](../adr/0003-direct-download-distribution-on-macos.md).

Scope: get VoiceDrop into users' hands on each platform, with a working update path — no new product features here.

## Todos

**macOS** ([#56](https://github.com/nitesw/VoiceDrop_App/issues/56))
- [ ] Apple Developer ID code signing set up for the built app
- [ ] Notarization pipeline (submit, staple, verify) as part of the release build process
- [ ] `.dmg` packaging with a proper installer background/layout
- [ ] Auto-update channel (e.g. Sparkle) wired up, including an update feed hosted somewhere durable
- [ ] Confirm a notarized, signed build actually launches cleanly on a clean macOS install (no dev-machine-only exceptions masking a signing problem)

**Windows** ([#57](https://github.com/nitesw/VoiceDrop_App/issues/57))
- [ ] Code signing certificate acquired and wired into the build
- [ ] Installer built (MSIX for Store-adjacent distribution, or Inno Setup/similar for a plain installer — decide based on whether Store distribution is ever wanted, otherwise default to the simpler option)
- [ ] Update channel wired up (e.g. Squirrel.Windows, or a custom check-for-updates flow hitting the same feed as macOS if practical)
- [ ] Confirm SmartScreen doesn't flag the signed installer on a clean machine

**Linux** ([#58](https://github.com/nitesw/VoiceDrop_App/issues/58))
- [ ] AppImage packaging as the baseline (works across distros without needing package-manager integration)
- [ ] `.deb` and `.rpm` packages as stretch goals, only after AppImage is solid
- [ ] Flatpak as a further stretch goal — note that Flatpak's sandboxing may reintroduce some of the same restrictions the macOS App Store decision avoided (global hotkeys, injection); evaluate before committing to it
- [ ] Update mechanism appropriate to whichever packaging format(s) ship first (AppImage has no built-in updater — decide whether to build one or leave manual)

**Cross-cutting**
- [ ] Versioning scheme decided and applied consistently across all three platforms
- [ ] Release process documented (build → sign → notarize/package → publish → update feed) so it's repeatable, not tribal knowledge
- [ ] Crash/error reporting decision: is there any opt-in telemetry, or is this fully silent per the local-first privacy stance from [ADR-0002](../adr/0002-local-first-with-byo-key-cloud-fallback.md)? Decide explicitly rather than defaulting into telemetry by accident

## Done when

- A signed, notarized/appropriately-trusted build exists for all three platforms and installs cleanly on a clean machine
- Each platform has a working update path from an older installed version to the current one
- The release process is documented well enough that it doesn't depend on one person's memory
