// swift-tools-version:5.9
import PackageDescription

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
            // MenuBarIcon.png/@2x/@3x live in ../Resources (sibling of
            // Sources/, not a SwiftPM target resource): SwiftPM's generated
            // Bundle.module accessor expects its resource bundle directly
            // at the .app root, which breaks `codesign --verify --deep
            // --strict` (see docs/todos/0005-phase4-macos-core-loop.md).
            // scripts/build-macos-app.sh copies them straight into
            // Contents/Resources/ instead, alongside AppIcon.icns;
            // MenuBarIconLoader.swift loads them by direct file path.
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
            ]
        ),
    ]
)
