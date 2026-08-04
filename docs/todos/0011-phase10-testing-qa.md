# Phase 10 — Testing, Quality Assurance & Performance Tuning

Tracks [GitHub issue #11](https://github.com/nitesw/VoiceDrop_App/issues/11). Deliberately last: exercises everything built in Phases 1–9. Terms in *italics* refer to [CONTEXT.md](../../CONTEXT.md).

Scope: verify the whole system holds up, not just that each phase's own manual checks passed in isolation.

## Todos

**Rust core unit tests** ([#59](https://github.com/nitesw/VoiceDrop_App/issues/59))
- [ ] *Dictation Session* state machine: all valid transitions succeed, all invalid ones are rejected (should already exist from Phase 1 — audit for gaps)
- [ ] STT output handling: silence/no-speech short-circuit, language selection, Custom Vocabulary bias hook
- [ ] Cleanup Pass: prompt construction per *Cleanup Strength* level, per provider; provider error handling (timeout, invalid key, network failure)
- [ ] Custom Vocabulary biasing actually changes recognition/cleanup output on a controlled test case

**Integration tests** ([#59](https://github.com/nitesw/VoiceDrop_App/issues/59))
- [ ] Full pipeline test: fixture audio → *Raw Transcript* → *Cleaned Transcript*, across all supported languages, using pre-recorded audio samples (not live mic input, for reproducibility)
- [ ] Run the full pipeline against each Cleanup Pass provider (local, Apple-native, cloud) with the same fixture input and compare output quality/latency
- [ ] Voice Command disambiguation: a corpus of test utterances where command phrases appear as genuine content (e.g. "scratch that itch") must NOT trigger the command, and clear standalone command utterances must trigger reliably

**Injection test matrix** ([#60](https://github.com/nitesw/VoiceDrop_App/issues/60))
- [ ] Build a matrix of target app types × platforms: native text fields, Electron apps, terminal emulators, secure/password fields, browser text areas
- [ ] For each cell: confirm correct cursor-position insertion, or correct fallback to clipboard where injection isn't supported/safe
- [ ] Focus-changed-mid-processing case tested explicitly, not just assumed to work because it wasn't observed failing

**Manual QA pass per platform** ([#61](https://github.com/nitesw/VoiceDrop_App/issues/61))
- [ ] Hotkey capture reliability (including edge cases: key-repeat, held across an app switch, held longer than expected)
- [ ] Deferred from Phase 1: disconnect the input device (e.g. unplug AirPods/USB mic) mid-recording on each platform and confirm the app surfaces the failure cleanly (no crash) rather than hanging — unit-tested at the state-machine level only, never exercised against real hardware
- [ ] Dictation HUD rendering and position picker across all supported positions, including multi-monitor setups
- [ ] Menu Bar Icon / tray icon toggle behavior (enable/disable actually suspends the hotkey)
- [ ] Settings Window: every preference persists correctly across app restart
- [ ] Session History: review, re-copy, clear-single, clear-all
- [ ] Launch at Login actually launches on a real reboot, not just when manually triggered
- [ ] First-run permissions onboarding walked through on a genuinely clean OS user account, not a dev machine with permissions already granted

**Performance** ([#62](https://github.com/nitesw/VoiceDrop_App/issues/62))
- [ ] Measure end-to-end latency (key-release to injected text) on minimum-spec target hardware per platform, not just dev machines
- [ ] Confirm latency stays within the "few seconds" UX target across all three Cleanup Pass providers
- [ ] Memory/CPU footprint check during idle (hotkey listener running) and during active processing

## Done when

- Every checklist item above has a pass/fail result recorded somewhere durable (not just "seemed fine")
- Any known failures or unsupported combinations (e.g. a specific injection-target type, a Linux display server limitation) are documented as known limitations, not silently shipped
- Latency and resource usage meet the targets on genuinely minimum-spec hardware, not just the dev machine used to build the app
