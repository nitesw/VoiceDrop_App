// swift-tools-version:5.9
import Foundation
import PackageDescription

// Absolute path to `target/<config>` — computed here (rather than a
// relative `-L`) because linker search paths are one thing, but the
// matching `-rpath` below must resolve at *run* time from wherever the
// final binary ends up (bare `swift build` binary vs. bundled .app), and a
// relative rpath can't do that reliably. This is a dev-only wiring: Phase 9
// (distribution) needs to copy these dylibs into
// VoiceDrop.app/Contents/Frameworks with proper install_name/rpath fixups
// instead of pointing at the build directory.
let repoRoot = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
let targetReleaseDir = repoRoot.appendingPathComponent("target/release").path

let package = Package(
    name: "VoiceDrop",
    platforms: [.macOS(.v13)],
    targets: [
        // Modulemap here points at the hand-written header in core/include
        // so Swift can import the voicedrop-core Rust static library.
        .systemLibrary(name: "CVoiceDropCore"),
        .executableTarget(
            name: "VoiceDrop",
            dependencies: ["CVoiceDropCore"],
            // MenuBarIcon.png/@2x/@3x (Resources/) — a monochrome template
            // image staged for Phase 4's Menu Bar Icon (NSStatusItem), not
            // wired up yet. Access via Bundle.module once that phase starts.
            resources: [.copy("Resources")],
            linkerSettings: [
                // Requires `cargo build --release` to have run first, producing
                // ../target/release/libvoicedrop_core.a (see README build steps).
                .unsafeFlags(["-L../target/release", "-lvoicedrop_core"]),
                // cpal (inside voicedrop-core) drives CoreAudio's AudioUnit
                // APIs directly on macOS.
                .linkedFramework("AudioToolbox"),
                .linkedFramework("CoreAudio"),
                // whisper.cpp (inside voicedrop-core, via whisper-rs) is C++
                // and uses Accelerate's vDSP/BLAS for CPU inference.
                .linkedLibrary("c++"),
                .linkedFramework("Accelerate"),
                // llama.cpp (inside voicedrop-core, via llama-cpp-2 — the
                // self-contained local Cleanup Pass, see
                // docs/adr/0008-local-cleanup-in-process-again.md) is built
                // with the `dynamic-link` feature specifically to avoid
                // this: whisper-rs and llama-cpp-2 each vendor their own
                // copy of ggml, and statically linking both into one binary
                // produces ~600 duplicate-symbol linker errors (ggml_init,
                // gguf_get_val_*, etc. defined twice — see
                // docs/adr/0006-shared-ggml-symbol-collision-and-model-catalog.md).
                // Dynamic linking keeps llama's ggml in its own dylibs
                // instead of merging its object code into
                // libvoicedrop_core.a, so there's only one ggml copy in the
                // final image's static portion.
                .unsafeFlags([
                    "-L\(targetReleaseDir)",
                    "-lllama", "-lllama-common",
                    "-Xlinker", "-rpath", "-Xlinker", targetReleaseDir,
                ]),
            ]
        ),
    ]
)
