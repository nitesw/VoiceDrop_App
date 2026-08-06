//! A curated catalog of downloadable models — the STT model, and Cleanup
//! Pass candidates for the self-contained in-process `Local` provider (see
//! [ADR-0004](../../docs/adr/0004-whisper-model-download-on-first-run.md),
//! [ADR-0008](../../docs/adr/0008-local-cleanup-in-process-again.md)) —
//! plus a separate list of model names to suggest when the user picks
//! "bring your own via Ollama" through the free-form `CloudProvider`
//! (`OLLAMA_MODELS` below; Ollama owns pulling/storing those itself,
//! VoiceDrop only suggests names).

use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Stt,
    Cleanup,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelCatalogEntry {
    /// Stable identifier — used as the on-disk filename stem and the FFI
    /// lookup key, so it must never change once shipped (existing installs
    /// would silently "lose" their downloaded file).
    pub id: &'static str,
    pub display_name: &'static str,
    pub kind: ModelKind,
    pub url: &'static str,
    pub filename: &'static str,
    /// Approximate download size, for UI display before committing to a
    /// download — not read from the server, so it's a label, not a
    /// contract.
    pub approx_size_bytes: u64,
}

/// The full catalog. STT only has one entry today (`ggml-medium` isn't
/// cataloged yet — hasn't been benchmarked per the still-open Phase 2 todo
/// item). Cleanup has three, all run through `LocalProvider`/llama.cpp:
/// Qwen2.5-0.5B (fastest, but too weak to be useful), Qwen2.5-1.5B — the
/// default (`engine::default_cleanup_model_path`) after manual side-by-side
/// comparison found it the best balance — and Llama-3.2-3B, kept as an
/// available option but NOT labeled "highest quality": it produced more
/// fluent prose but respected the `VerbatimClean` strength's "preserve
/// exact wording" instruction *less* than the smaller models, restructuring
/// sentences even when asked not to. A real benchmark on minimum-spec
/// hardware (vs. this dev machine) is still an open todo item.
pub const CATALOG: &[ModelCatalogEntry] = &[
    ModelCatalogEntry {
        id: "whisper-small",
        display_name: "Whisper Small",
        kind: ModelKind::Stt,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        filename: "ggml-small.bin",
        approx_size_bytes: 487_000_000,
    },
    ModelCatalogEntry {
        id: "qwen2.5-0.5b-instruct",
        display_name: "Qwen2.5 0.5B Instruct (fastest)",
        kind: ModelKind::Cleanup,
        url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf",
        filename: "qwen2.5-0.5b-instruct-q4_k_m.gguf",
        approx_size_bytes: 491_000_000,
    },
    ModelCatalogEntry {
        id: "qwen2.5-1.5b-instruct",
        display_name: "Qwen2.5 1.5B Instruct (recommended)",
        kind: ModelKind::Cleanup,
        url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf",
        filename: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        approx_size_bytes: 986_000_000,
    },
    ModelCatalogEntry {
        id: "llama-3.2-3b-instruct",
        display_name: "Llama 3.2 3B Instruct (largest, less faithful to strength)",
        kind: ModelKind::Cleanup,
        url: "https://huggingface.co/bartowski/Llama-3.2-3B-Instruct-GGUF/resolve/main/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        filename: "Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        approx_size_bytes: 2_020_000_000,
    },
];

pub fn find(id: &str) -> Option<&'static ModelCatalogEntry> {
    CATALOG.iter().find(|e| e.id == id)
}

/// A model name to suggest for the "bring your own via Ollama" path
/// (Ollama-backed cleanup goes through `CloudProvider`'s free-form
/// endpoint, pointed at `http://localhost:11434/v1` — see
/// [ADR-0008](../../docs/adr/0008-local-cleanup-in-process-again.md)).
/// `ollama pull`/the Ollama API manages the actual download; VoiceDrop just
/// suggests names.
#[derive(Debug, Clone, Copy)]
pub struct OllamaModelSuggestion {
    pub ollama_name: &'static str,
    pub display_name: &'static str,
    pub approx_size_bytes: u64,
}

/// Suggested Ollama models, smallest/fastest first. Deliberately short —
/// a guided picker, not an open-ended model browser.
pub const OLLAMA_MODELS: &[OllamaModelSuggestion] = &[
    OllamaModelSuggestion {
        ollama_name: "qwen2.5:0.5b",
        display_name: "Qwen2.5 0.5B (fastest)",
        approx_size_bytes: 397_000_000,
    },
    OllamaModelSuggestion {
        ollama_name: "qwen2.5:1.5b",
        display_name: "Qwen2.5 1.5B (recommended)",
        approx_size_bytes: 986_000_000,
    },
    OllamaModelSuggestion {
        ollama_name: "llama3.2:3b",
        display_name: "Llama 3.2 3B (largest, less faithful to strength)",
        approx_size_bytes: 2_000_000_000,
    },
];

#[derive(Debug)]
pub enum ModelError {
    UnknownId(String),
    NetworkFailed(String),
    IoFailed(String),
}

fn models_dir() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Library/Application Support/VoiceDrop/models")
}

pub fn path_for(entry: &ModelCatalogEntry) -> PathBuf {
    models_dir().join(entry.filename)
}

pub fn is_downloaded(entry: &ModelCatalogEntry) -> bool {
    path_for(entry).is_file()
}

/// Downloads `id`'s model file to its catalog location, streaming to a
/// `.part` file and renaming on success so a failed/cancelled download
/// never leaves a corrupt file at the real path. `on_progress(bytes_read,
/// total_bytes)` is called periodically — `total_bytes` is `None` if the
/// server didn't send a `Content-Length`. This blocks for the whole
/// download; callers (Settings window UI, once it exists) must not run it
/// on a UI-critical thread — see the lesson documented in
/// `HotkeyMonitor.swift` about blocking the CGEventTap callback.
pub fn download(id: &str, mut on_progress: impl FnMut(u64, Option<u64>)) -> Result<(), ModelError> {
    let entry = find(id).ok_or_else(|| ModelError::UnknownId(id.to_string()))?;

    let dir = models_dir();
    std::fs::create_dir_all(&dir).map_err(|e| ModelError::IoFailed(e.to_string()))?;

    let dest = path_for(entry);
    let tmp_dest = dest.with_extension(format!(
        "{}.part",
        dest.extension().and_then(|e| e.to_str()).unwrap_or("tmp")
    ));

    let response = ureq::get(entry.url)
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

/// Deletes `id`'s downloaded model file, if present. Not an error if it
/// was already absent — deleting a not-currently-downloaded model is a
/// no-op, not a failure, from the UI's perspective.
pub fn delete(id: &str) -> Result<(), ModelError> {
    let entry = find(id).ok_or_else(|| ModelError::UnknownId(id.to_string()))?;
    let path = path_for(entry);
    if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| ModelError::IoFailed(e.to_string()))?;
    }
    Ok(())
}

// --- FFI surface -----------------------------------------------------
//
// Global/catalog-scoped, not tied to a `VoiceDropEngine` handle — there's
// only ever one catalog and one models directory per install. Phase 5's
// Settings Window is the intended caller: list the STT catalog / Ollama
// suggestions to populate dropdowns, call download() for STT models (off
// the main thread — this blocks for the whole transfer), call delete() to
// remove one.

use std::os::raw::c_char;

#[repr(i32)]
pub enum ModelStatusCode {
    Ok = 0,
    UnknownId = 1,
    NetworkFailed = 2,
    IoFailed = 3,
}

fn to_status(result: Result<(), ModelError>) -> i32 {
    match result {
        Ok(()) => ModelStatusCode::Ok as i32,
        Err(ModelError::UnknownId(_)) => ModelStatusCode::UnknownId as i32,
        Err(ModelError::NetworkFailed(_)) => ModelStatusCode::NetworkFailed as i32,
        Err(ModelError::IoFailed(_)) => ModelStatusCode::IoFailed as i32,
    }
}

fn to_cstring(s: &str) -> *mut c_char {
    std::ffi::CString::new(s)
        .map(std::ffi::CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

/// Number of entries in the STT model catalog.
#[no_mangle]
pub extern "C" fn voicedrop_model_catalog_count() -> i32 {
    CATALOG.len() as i32
}

/// Returns catalog entry `index`'s stable id as an owned C string, or NULL
/// if `index` is out of range. Caller must free with
/// `voicedrop_core_free_string`. Pass this id to
/// `voicedrop_model_is_downloaded`/`_download`/`_delete`.
#[no_mangle]
pub extern "C" fn voicedrop_model_catalog_id(index: i32) -> *mut c_char {
    CATALOG
        .get(index as usize)
        .map(|e| to_cstring(e.id))
        .unwrap_or(std::ptr::null_mut())
}

/// Returns catalog entry `index`'s human-readable name as an owned C
/// string, or NULL if out of range. Caller must free with
/// `voicedrop_core_free_string`.
#[no_mangle]
pub extern "C" fn voicedrop_model_catalog_display_name(index: i32) -> *mut c_char {
    CATALOG
        .get(index as usize)
        .map(|e| to_cstring(e.display_name))
        .unwrap_or(std::ptr::null_mut())
}

/// Returns catalog entry `index`'s kind: 0 = STT (whisper.cpp), 1 = Cleanup
/// (llama.cpp, self-contained `Local` provider). Returns -1 if `index` is
/// out of range.
#[no_mangle]
pub extern "C" fn voicedrop_model_catalog_kind(index: i32) -> i32 {
    match CATALOG.get(index as usize).map(|e| e.kind) {
        Some(ModelKind::Stt) => 0,
        Some(ModelKind::Cleanup) => 1,
        None => -1,
    }
}

/// Returns catalog entry `index`'s approximate download size in bytes (a
/// UI label, not a guarantee), or 0 if `index` is out of range.
#[no_mangle]
pub extern "C" fn voicedrop_model_catalog_approx_size_bytes(index: i32) -> u64 {
    CATALOG
        .get(index as usize)
        .map(|e| e.approx_size_bytes)
        .unwrap_or(0)
}

/// Returns 1 if `id`'s model file is already downloaded, 0 if not, -1 if
/// `id` is unknown or invalid.
///
/// # Safety
/// `id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_model_is_downloaded(id: *const c_char) -> i32 {
    if id.is_null() {
        return -1;
    }
    let Ok(id_str) = std::ffi::CStr::from_ptr(id).to_str() else {
        return -1;
    };
    match find(id_str) {
        Some(entry) => i32::from(is_downloaded(entry)),
        None => -1,
    }
}

/// Returns the on-disk path `id`'s model file would live at (whether or
/// not it's actually downloaded yet) as an owned C string, or NULL if `id`
/// is unknown. Lets callers feed the result straight into
/// `voicedrop_engine_set_model_path` without reimplementing this crate's
/// filename/directory convention. Caller must free with
/// `voicedrop_core_free_string`.
///
/// # Safety
/// `id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_model_path_for(id: *const c_char) -> *mut c_char {
    if id.is_null() {
        return std::ptr::null_mut();
    }
    let Ok(id_str) = std::ffi::CStr::from_ptr(id).to_str() else {
        return std::ptr::null_mut();
    };
    match find(id_str) {
        Some(entry) => to_cstring(&path_for(entry).to_string_lossy()),
        None => std::ptr::null_mut(),
    }
}

/// Downloads `id`'s model file, blocking until it completes or fails.
/// `on_progress`, if non-NULL, is called periodically with
/// `(bytes_downloaded, total_bytes_or_0_if_unknown, user_data)` — `user_data`
/// is passed through unchanged, letting the caller recover context (e.g. a
/// boxed Swift closure) without any global state on the Rust side.
///
/// Blocks for the entire transfer: callers must run this off any
/// UI-critical thread. See the CGEventTap lesson documented in
/// `HotkeyMonitor.swift` — the exact same "don't block a callback the OS
/// expects back quickly" failure mode applies to any UI thread here too.
///
/// # Safety
/// `id` must be a valid NUL-terminated UTF-8 C string. `on_progress`, if
/// non-NULL, must be safe to call from the calling thread with the given
/// `user_data` for the duration of this call. `user_data` may be NULL and
/// is otherwise opaque to this function.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_model_download(
    id: *const c_char,
    on_progress: Option<extern "C" fn(u64, u64, *mut std::ffi::c_void)>,
    user_data: *mut std::ffi::c_void,
) -> i32 {
    if id.is_null() {
        return ModelStatusCode::UnknownId as i32;
    }
    let Ok(id_str) = std::ffi::CStr::from_ptr(id).to_str() else {
        return ModelStatusCode::UnknownId as i32;
    };

    // Wrap the raw pointer so the closure below can be `Send`-checked by
    // the compiler as a plain value capture — we never move it across
    // threads ourselves, `download` calls it synchronously in-line.
    struct SendPtr(*mut std::ffi::c_void);
    let user_data = SendPtr(user_data);

    let result = download(id_str, |downloaded, total| {
        if let Some(callback) = on_progress {
            callback(downloaded, total.unwrap_or(0), user_data.0);
        }
    });
    to_status(result)
}

/// Deletes `id`'s downloaded model file, if present. Not an error if it
/// was already absent.
///
/// # Safety
/// `id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_model_delete(id: *const c_char) -> i32 {
    if id.is_null() {
        return ModelStatusCode::UnknownId as i32;
    }
    let Ok(id_str) = std::ffi::CStr::from_ptr(id).to_str() else {
        return ModelStatusCode::UnknownId as i32;
    };
    to_status(delete(id_str))
}

/// Number of suggested Ollama Cleanup Pass models.
#[no_mangle]
pub extern "C" fn voicedrop_ollama_model_count() -> i32 {
    OLLAMA_MODELS.len() as i32
}

/// Returns suggestion `index`'s Ollama model name (pass to `ollama pull`,
/// then to `voicedrop_engine_set_cleanup_cloud_config`'s `model` parameter
/// with `base_url` set to `http://localhost:11434/v1`), or NULL if out of
/// range. Caller must free with `voicedrop_core_free_string`.
#[no_mangle]
pub extern "C" fn voicedrop_ollama_model_name(index: i32) -> *mut c_char {
    OLLAMA_MODELS
        .get(index as usize)
        .map(|e| to_cstring(e.ollama_name))
        .unwrap_or(std::ptr::null_mut())
}

/// Returns suggestion `index`'s human-readable display name, or NULL if
/// out of range. Caller must free with `voicedrop_core_free_string`.
#[no_mangle]
pub extern "C" fn voicedrop_ollama_model_display_name(index: i32) -> *mut c_char {
    OLLAMA_MODELS
        .get(index as usize)
        .map(|e| to_cstring(e.display_name))
        .unwrap_or(std::ptr::null_mut())
}

/// Returns suggestion `index`'s approximate download size in bytes (a UI
/// label — Ollama, not VoiceDrop, actually manages the download), or 0 if
/// out of range.
#[no_mangle]
pub extern "C" fn voicedrop_ollama_model_approx_size_bytes(index: i32) -> u64 {
    OLLAMA_MODELS
        .get(index as usize)
        .map(|e| e.approx_size_bytes)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
    }

    #[test]
    fn find_returns_known_entry() {
        assert!(find("whisper-small").is_some());
    }

    #[test]
    fn find_returns_none_for_unknown_id() {
        assert!(find("does-not-exist").is_none());
    }

    #[test]
    fn is_downloaded_false_for_missing_file() {
        // Relies on no test environment coincidentally having this exact
        // filename already present under Application Support.
        let entry = find("llama-3.2-3b-instruct").unwrap();
        if !path_for(entry).is_file() {
            assert!(!is_downloaded(entry));
        }
    }

    #[test]
    fn catalog_has_one_stt_and_multiple_cleanup_candidates() {
        let stt_count = CATALOG.iter().filter(|e| e.kind == ModelKind::Stt).count();
        let cleanup_count = CATALOG
            .iter()
            .filter(|e| e.kind == ModelKind::Cleanup)
            .count();
        assert_eq!(stt_count, 1);
        assert!(
            cleanup_count >= 2,
            "expected multiple candidates to compare"
        );
    }

    #[test]
    fn download_rejects_unknown_id() {
        let result = download("does-not-exist", |_, _| {});
        assert!(matches!(result, Err(ModelError::UnknownId(_))));
    }

    #[test]
    fn delete_rejects_unknown_id() {
        let result = delete("does-not-exist");
        assert!(matches!(result, Err(ModelError::UnknownId(_))));
    }

    #[test]
    fn ollama_models_have_unique_names() {
        let mut names: Vec<&str> = OLLAMA_MODELS.iter().map(|e| e.ollama_name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), OLLAMA_MODELS.len());
    }

    #[test]
    fn ollama_models_list_is_non_empty() {
        assert!(!OLLAMA_MODELS.is_empty());
    }
}

#[cfg(test)]
mod manual_check {
    use super::*;

    /// Read-only sanity check against real files already on this dev
    /// machine (downloaded via scripts/download-whisper-model.sh) —
    /// confirms `path_for`'s filename construction actually matches what's
    /// on disk, without downloading or deleting anything.
    #[test]
    #[ignore]
    fn reports_real_downloaded_state() {
        for entry in CATALOG {
            println!(
                "{}: downloaded={} path={:?}",
                entry.id,
                is_downloaded(entry),
                path_for(entry)
            );
        }
    }
}
