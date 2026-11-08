import CVoiceDropCore
import Foundation

/// First-run model provisioning (ADR-0004). Checks whether the Whisper
/// model exists where the core loads it from; if not, downloads it with
/// visible progress. The hotkey must not be armed until this completes —
/// see main.swift's `provisionModelThenArm`. Which model and where it lives
/// is decided entirely by `core/src/models.rs`.
enum ModelProvisioner {

    /// Calls `onProgress` with a 0.0-1.0 fraction as the download
    /// proceeds (main thread), then `completion` with whether the model
    /// ended up present (main thread) — `true` immediately if it was
    /// already downloaded.
    static func ensureWhisperModel(
        onProgress: @escaping (Double) -> Void,
        completion: @escaping (Bool) -> Void
    ) {
        if voicedrop_model_is_downloaded() == 1 {
            completion(true)
            return
        }

        DispatchQueue.global(qos: .userInitiated).async {
            let box = ProgressBox(onProgress: onProgress)
            let userData = Unmanaged.passRetained(box).toOpaque()

            let status = voicedrop_model_download(
                { downloaded, total, context in
                    guard let context, total > 0 else { return }
                    let box = Unmanaged<ProgressBox>.fromOpaque(context).takeUnretainedValue()
                    let fraction = Double(downloaded) / Double(total)
                    DispatchQueue.main.async { box.onProgress(fraction) }
                },
                userData
            )

            Unmanaged<ProgressBox>.fromOpaque(userData).release()
            DispatchQueue.main.async { completion(status == VOICEDROP_MODEL_OK) }
        }
    }
}

/// Boxes the progress closure so it can cross the C callback boundary via
/// `voicedrop_model_download`'s opaque `user_data` pointer.
private final class ProgressBox {
    let onProgress: (Double) -> Void
    init(onProgress: @escaping (Double) -> Void) {
        self.onProgress = onProgress
    }
}
