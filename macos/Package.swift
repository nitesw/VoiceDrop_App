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
            linkerSettings: [
                // Requires `cargo build --release` to have run first, producing
                // ../target/release/libvoicedrop_core.a (see README build steps).
                .unsafeFlags(["-L../target/release", "-lvoicedrop_core"])
            ]
        ),
    ]
)
