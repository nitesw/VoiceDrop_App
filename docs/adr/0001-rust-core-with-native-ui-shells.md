# Shared Rust core with a native UI shell per platform

VoiceDrop must be genuinely native on macOS, Windows, and Linux, but Swift's UI toolkits (SwiftUI/AppKit) only run natively on macOS — there's no single-language path to native UI on all three. We considered building fully separate native apps per platform (no shared code) and keeping Swift as the shared layer (via swift-corelibs), but rejected both: full duplication means writing and debugging STT/cleanup/hotkey/injection logic three times, and Swift's cross-platform tooling for audio and ML inference is far less mature than Rust's.

Decided: all non-UI logic (audio capture, STT inference, Cleanup Pass LLM inference, hotkey capture, text injection) lives in a shared Rust core, compiled to a native library per OS. Each platform gets a thin native UI shell on top: SwiftUI on macOS, WinUI3/C# (or Win32) on Windows, GTK4 on Linux. Logic is written once; only the UI and OS-integration glue is platform-specific.

## Consequences

- Higher upfront cost (FFI boundary, three UI codebases) than a single Electron/web app, but each shell is genuinely native — no Electron/web option was acceptable per the initial ask.
- Hotkey capture and text injection are OS-specific even though they're "in the Rust core" — expect a thin per-OS adapter layer inside the core, not fully shared code.
