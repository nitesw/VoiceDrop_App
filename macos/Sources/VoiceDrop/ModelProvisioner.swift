import CVoiceDropCore
import Foundation

/// First-run model provisioning — the Swift-side gap left open by
/// ADR-0004/ADR-0005. Checks whether the Whisper STT model exists at the
/// core's default path; if not, downloads it with visible progress. The
/// hotkey must not be armed until this completes — see main.swift's
/// `provisionModelThenArm`. The Cleanup Pass's local GGUF model is a
/// separate, later trigger (when the user actually selects that
/// provider), not bundled into this gate — `None`/cloud users should never
/// see this download.
enum ModelProvisioner {
    private static let whisperModelID = "whisper-small"

    /// Calls `onProgress` with a 0.0-1.0 fraction as the download
    /// proceeds (main thread), then `completion` with whether the model
    /// ended up present (main thread) — `true` immediately if it was
    /// already downloaded.
    static func ensureWhisperModel(
        onProgress: @escaping (Double) -> Void,
        completion: @escaping (Bool) -> Void
    ) {
        let alreadyDownloaded = whisperModelID.withCString { voicedrop_model_is_downloaded($0) }
        if alreadyDownloaded == 1 {
            completion(true)
            return
        }

        DispatchQueue.global(qos: .userInitiated).async {
            let box = ProgressBox(onProgress: onProgress)
            let userData = Unmanaged.passRetained(box).toOpaque()

            let status = whisperModelID.withCString { idPtr in
                voicedrop_model_download(
                    idPtr,
                    { downloaded, total, context in
                        guard let context, total > 0 else { return }
                        let box = Unmanaged<ProgressBox>.fromOpaque(context).takeUnretainedValue()
                        let fraction = Double(downloaded) / Double(total)
                        DispatchQueue.main.async { box.onProgress(fraction) }
                    },
                    userData
                )
            }

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
