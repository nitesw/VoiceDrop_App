//! The one model VoiceDrop needs: the Whisper speech-to-text model,
//! downloaded on first run (see
//! [ADR-0004](../../docs/adr/0004-whisper-model-download-on-first-run.md))
//! into Application Support. This is the single source of truth for its
//! filename and URL — `engine` loads from `path()`, and the Swift shell's
//! first-run download goes through the FFI below — so the file that gets
//! downloaded and the file that gets loaded can never drift apart.

use std::io::{Read, Write};
use std::os::raw::c_void;
use std::path::PathBuf;

pub struct WhisperModel {
    pub url: &'static str,
    pub filename: &'static str,
}

/// Whisper large-v3-turbo, 5-bit quantized (~574 MB): far more accurate
/// than `small` at a similar download size, and fast on the GPU (ADR-0009).
pub const WHISPER_MODEL: WhisperModel = WhisperModel {
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
    filename: "ggml-large-v3-turbo-q5_0.bin",
};

#[derive(Debug)]
pub enum ModelError {
    NetworkFailed(String),
    IoFailed(String),
}

fn models_dir() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Library/Application Support/VoiceDrop/models")
}

/// Where the Whisper model lives on disk, downloaded or not.
pub fn path() -> PathBuf {
    models_dir().join(WHISPER_MODEL.filename)
}

pub fn is_downloaded() -> bool {
    path().is_file()
}

/// Downloads the Whisper model, streaming to a `.part` file and renaming on
/// success so a failed download never leaves a corrupt file at the real
/// path. `on_progress(bytes_read, total_bytes)` is called periodically —
/// `total_bytes` is `None` if the server didn't send a `Content-Length`.
/// Blocks for the whole download: never call it on a UI-critical thread
/// (see the CGEventTap lesson in `HotkeyMonitor.swift`).
pub fn download(mut on_progress: impl FnMut(u64, Option<u64>)) -> Result<(), ModelError> {
    let dir = models_dir();
    std::fs::create_dir_all(&dir).map_err(|e| ModelError::IoFailed(e.to_string()))?;

    let dest = path();
    let tmp_dest = dest.with_extension("bin.part");

    let response = ureq::get(WHISPER_MODEL.url)
        .call()
        .map_err(|e| ModelError::NetworkFailed(e.to_string()))?;
    let total_bytes = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    let mut reader = response.into_body().into_reader();
    let mut file =
        std::fs::File::create(&tmp_dest).map_err(|e| ModelError::IoFailed(e.to_string()))?;

    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| ModelError::NetworkFailed(e.to_string()))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| ModelError::IoFailed(e.to_string()))?;
        downloaded += n as u64;
        on_progress(downloaded, total_bytes);
    }
    drop(file);

    std::fs::rename(&tmp_dest, &dest).map_err(|e| ModelError::IoFailed(e.to_string()))?;
    Ok(())
}

// --- FFI surface -----------------------------------------------------

#[repr(i32)]
pub enum ModelStatusCode {
    Ok = 0,
    NetworkFailed = 2,
    IoFailed = 3,
}

/// Returns 1 if the Whisper model is already downloaded, 0 if not.
#[no_mangle]
pub extern "C" fn voicedrop_model_is_downloaded() -> i32 {
    i32::from(is_downloaded())
}

/// Downloads the Whisper model, blocking until it completes or fails.
/// `on_progress`, if non-NULL, is called periodically with
/// `(bytes_downloaded, total_bytes_or_0_if_unknown, user_data)` — `user_data`
/// is passed through unchanged so the caller can recover context (e.g. a
/// boxed Swift closure). Blocks for the entire transfer: call it off any
/// UI-critical thread.
///
/// # Safety
/// `on_progress`, if non-NULL, must be safe to call from the calling thread
/// with the given `user_data` for the duration of this call. `user_data` may
/// be NULL and is otherwise opaque to this function.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_model_download(
    on_progress: Option<extern "C" fn(u64, u64, *mut c_void)>,
    user_data: *mut c_void,
) -> i32 {
    let result = download(|downloaded, total| {
        if let Some(callback) = on_progress {
            callback(downloaded, total.unwrap_or(0), user_data);
        }
    });
    match result {
        Ok(()) => ModelStatusCode::Ok as i32,
        Err(ModelError::NetworkFailed(_)) => ModelStatusCode::NetworkFailed as i32,
        Err(ModelError::IoFailed(_)) => ModelStatusCode::IoFailed as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_ends_with_catalog_filename() {
        assert!(path().ends_with(WHISPER_MODEL.filename));
    }

    #[test]
    fn url_points_at_the_same_file() {
        assert!(WHISPER_MODEL.url.ends_with(WHISPER_MODEL.filename));
    }
}
