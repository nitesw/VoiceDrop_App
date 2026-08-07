//! C ABI surface for the Swift shell. Wraps `Session` + `AudioCapture` behind
//! an opaque handle. Intended to be driven from a single thread (the Swift
//! main thread reacting to CGEventTap callbacks) — no internal locking is
//! provided beyond what `AudioCapture`'s realtime callback already needs.

use crate::audio::{self, AudioCapture, AudioError, WHISPER_SAMPLE_RATE};
use crate::blocklist::Blocklist;
use crate::cleanup::{
    CleanupError, CleanupProvider, CleanupStrength, CloudProvider, LocalProvider,
};
use crate::session::{Session, SessionEvent, SessionState};
use crate::transcribe::{self, LanguageSetting, TranscribeConfig, TranscribeError, Transcriber};
use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};

#[repr(i32)]
pub enum StatusCode {
    Ok = 0,
    IllegalState = 1,
    NoInputDevice = 2,
    AudioOther = 3,
    NotRecording = 4,
    WavWriteFailed = 5,
    DeviceDisconnected = 6,
    InvalidPath = 7,
    /// Recorded audio was silence-only or too brief to transcribe.
    NoSpeech = 8,
    /// The model failed to load, or whisper.cpp itself failed mid-run.
    TranscriptionFailed = 9,
    /// The Cleanup Pass model failed to load, or inference itself failed.
    CleanupFailed = 10,
    /// The cloud Cleanup Pass provider's request failed (network/timeout).
    CleanupNetworkFailed = 11,
    /// The selected Cleanup Pass provider is missing required config (e.g.
    /// no cloud base URL/API key set).
    CleanupInvalidConfig = 12,
}

/// Runtime provider selection for the Cleanup Pass, per
/// ADR-0002/ADR-0005/ADR-0008. `Local` is a self-contained in-process
/// llama.cpp model (see `cleanup::LocalProvider`'s doc) — "bring your own
/// via Ollama or another local runner" is `Cloud` instead, pointed at a
/// local address. `Apple` is a marker only — Rust can't call Apple's
/// Swift/ObjC-only Foundation Models framework, so when this is selected
/// `stop_recording` stops after STT and leaves cleanup to the Swift shell,
/// which calls `voicedrop_engine_set_cleaned_transcript` once it has a
/// result. See `cleanup.rs`'s module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CleanupProviderKind {
    None,
    Local,
    Apple,
    Cloud,
}

pub struct Engine {
    session: Session,
    capture: Option<AudioCapture>,
    /// Last stopped session's mono 16kHz samples, kept only so a follow-up
    /// call can write the verification WAV without re-capturing.
    last_samples: Option<Vec<f32>>,
    /// The *Raw Transcript*, once STT has produced one — unaffected by
    /// whatever the Cleanup Pass does to it.
    last_raw_transcript: Option<String>,
    /// The transcript ready for injection: the *Cleaned Transcript* once a
    /// transforming provider has run, the *Raw Transcript* unchanged for
    /// `None`, or (for `Apple`) whatever the Swift shell last set via
    /// `voicedrop_engine_set_cleaned_transcript`.
    last_transcript: Option<String>,
    model_path: PathBuf,
    /// Loaded lazily on first use — loading a whisper.cpp model takes
    /// seconds, so it's kept alive across sessions rather than per-utterance.
    transcriber: Option<Transcriber>,
    transcribe_config: TranscribeConfig,

    /// Custom words to always strip from the Raw Transcript (merged with a
    /// small built-in default list), independent of whichever Cleanup Pass
    /// provider — if any — runs afterward. See `blocklist.rs`'s module doc
    /// for why this can't be delegated to the Cleanup Pass.
    blocklist_words: Vec<String>,

    cleanup_provider: CleanupProviderKind,
    cleanup_strength: CleanupStrength,
    cleanup_local_model_path: PathBuf,
    /// Loaded lazily on first use, same rationale as `transcriber`.
    local_cleanup: Option<LocalProvider>,
    cloud_base_url: Option<String>,
    cloud_api_key: Option<String>,
    cloud_model: Option<String>,
    /// Rebuilt lazily from the cloud_* fields above; cleared whenever any of
    /// them change so a stale provider is never reused after a config edit.
    cloud_cleanup: Option<CloudProvider>,
}

impl Engine {
    fn new() -> Self {
        Engine {
            session: Session::new(),
            capture: None,
            last_samples: None,
            last_raw_transcript: None,
            last_transcript: None,
            model_path: default_model_path(),
            transcriber: None,
            transcribe_config: TranscribeConfig::default(),
            blocklist_words: Vec::new(),
            cleanup_provider: CleanupProviderKind::None,
            // Least-destructive default: don't restructure text the user
            // never asked to have restructured until they explicitly pick a
            // stronger strength in Settings (Phase 5).
            cleanup_strength: CleanupStrength::VerbatimClean,
            cleanup_local_model_path: default_cleanup_model_path(),
            local_cleanup: None,
            cloud_base_url: None,
            cloud_api_key: None,
            cloud_model: None,
            cloud_cleanup: None,
        }
    }

    fn start_recording(&mut self) -> StatusCode {
        if self.session.apply(SessionEvent::HotkeyDown).is_err() {
            return StatusCode::IllegalState;
        }
        match AudioCapture::start() {
            Ok(capture) => {
                self.capture = Some(capture);
                StatusCode::Ok
            }
            Err(err) => {
                // Roll the session back — recording never actually started.
                let _ = self.session.apply(SessionEvent::RecordingFailed);
                let _ = self.session.apply(SessionEvent::Reset);
                match err {
                    AudioError::NoInputDevice => StatusCode::NoInputDevice,
                    _ => StatusCode::AudioOther,
                }
            }
        }
    }

    /// Stops recording, resamples to 16kHz mono, then runs it through
    /// Whisper to produce a *Raw Transcript*. Short/silent buffers
    /// short-circuit to `NoSpeech` before Whisper ever runs — the Cleanup
    /// Pass (Phase 3) shouldn't need to handle empty input.
    fn stop_recording(&mut self) -> StatusCode {
        let Some(capture) = self.capture.take() else {
            return StatusCode::NotRecording;
        };
        if self.session.apply(SessionEvent::HotkeyUp).is_err() {
            return StatusCode::IllegalState;
        }

        let samples = match capture.stop() {
            Ok(samples) => samples,
            Err(AudioError::DeviceDisconnected) => {
                let _ = self.session.apply(SessionEvent::ProcessingFailed);
                return StatusCode::DeviceDisconnected;
            }
            Err(_) => {
                let _ = self.session.apply(SessionEvent::ProcessingFailed);
                return StatusCode::AudioOther;
            }
        };
        self.last_samples = Some(samples.clone());

        if transcribe::is_too_short(&samples) {
            let _ = self.session.apply(SessionEvent::NoSpeechDetected);
            return StatusCode::NoSpeech;
        }

        if self.transcriber.is_none() {
            match Transcriber::load(&self.model_path) {
                Ok(t) => self.transcriber = Some(t),
                Err(_) => {
                    let _ = self.session.apply(SessionEvent::ProcessingFailed);
                    return StatusCode::TranscriptionFailed;
                }
            }
        }

        match self
            .transcriber
            .as_ref()
            .expect("just loaded above")
            .transcribe(&samples, &self.transcribe_config)
        {
            Ok(text) if text.is_empty() => {
                let _ = self.session.apply(SessionEvent::NoSpeechDetected);
                StatusCode::NoSpeech
            }
            Ok(text) => {
                // The blocklist filter runs unconditionally, before any
                // Cleanup Pass provider (including `None`) ever sees the
                // text — see `blocklist.rs`'s module doc for why this can't
                // be delegated to the Cleanup Pass.
                let filtered = Blocklist::new(&self.blocklist_words).filter(&text);
                self.last_raw_transcript = Some(filtered.clone());
                self.run_cleanup_pass(filtered)
            }
            Err(TranscribeError::TranscriptionFailed(_))
            | Err(TranscribeError::ModelLoadFailed(_))
            | Err(TranscribeError::ModelNotFound(_)) => {
                let _ = self.session.apply(SessionEvent::ProcessingFailed);
                StatusCode::TranscriptionFailed
            }
        }
    }

    /// Runs the *Raw Transcript* through the selected Cleanup Pass provider
    /// and advances the session accordingly. `Apple` is handled specially:
    /// Rust can't call it (see `CleanupProviderKind::Apple`'s doc), so this
    /// stops after storing the Raw Transcript and lets the Swift shell
    /// finish the job via `voicedrop_engine_set_cleaned_transcript`.
    ///
    /// Per the "Provider failures surface as errors" requirement, a failed
    /// Cleanup Pass does NOT silently fall back to the Raw Transcript —
    /// `last_transcript` is left unset and the session reaches `Error`.
    fn run_cleanup_pass(&mut self, raw_transcript: String) -> StatusCode {
        let language = match &self.transcribe_config.language {
            LanguageSetting::Fixed(code) => Some(code.clone()),
            // Whisper's own auto-detected language isn't surfaced back to
            // us, so there's nothing to pass along here.
            LanguageSetting::Auto => None,
        };

        let cleaned = match self.cleanup_provider {
            CleanupProviderKind::None => Ok(raw_transcript),
            CleanupProviderKind::Apple => {
                self.last_transcript = None;
                let _ = self.session.apply(SessionEvent::ProcessingSucceeded);
                return StatusCode::Ok;
            }
            CleanupProviderKind::Local => {
                if self.local_cleanup.is_none() {
                    match LocalProvider::load(&self.cleanup_local_model_path) {
                        Ok(provider) => self.local_cleanup = Some(provider),
                        Err(err) => return self.fail_cleanup(err),
                    }
                }
                self.local_cleanup
                    .as_ref()
                    .expect("just loaded above")
                    .cleanup(&raw_transcript, self.cleanup_strength, language.as_deref())
            }
            CleanupProviderKind::Cloud => {
                if self.cloud_cleanup.is_none() {
                    match self.build_cloud_provider() {
                        Ok(provider) => self.cloud_cleanup = Some(provider),
                        Err(err) => return self.fail_cleanup(err),
                    }
                }
                self.cloud_cleanup
                    .as_ref()
                    .expect("just built above")
                    .cleanup(&raw_transcript, self.cleanup_strength, language.as_deref())
            }
        };

        match cleaned {
            Ok(text) => {
                self.last_transcript = Some(text);
                let _ = self.session.apply(SessionEvent::ProcessingSucceeded);
                StatusCode::Ok
            }
            Err(err) => self.fail_cleanup(err),
        }
    }

    fn fail_cleanup(&mut self, err: CleanupError) -> StatusCode {
        let _ = self.session.apply(SessionEvent::ProcessingFailed);
        match err {
            CleanupError::NetworkFailed(_) => StatusCode::CleanupNetworkFailed,
            CleanupError::InvalidConfig(_) => StatusCode::CleanupInvalidConfig,
            CleanupError::Timeout
            | CleanupError::InferenceFailed(_)
            | CleanupError::ModelNotFound(_) => StatusCode::CleanupFailed,
        }
    }

    fn build_cloud_provider(&self) -> Result<CloudProvider, CleanupError> {
        let base_url = self.cloud_base_url.clone().ok_or_else(|| {
            CleanupError::InvalidConfig("no cloud cleanup base URL configured".to_string())
        })?;
        let api_key = self.cloud_api_key.clone().ok_or_else(|| {
            CleanupError::InvalidConfig("no cloud cleanup API key configured".to_string())
        })?;
        let model = self.cloud_model.clone().unwrap_or_default();
        CloudProvider::new(base_url, api_key, model)
    }

    fn set_model_path(&mut self, path: PathBuf) {
        self.model_path = path;
        // Force a reload on next use rather than keeping a model built from
        // the previous path around.
        self.transcriber = None;
    }

    fn set_language(&mut self, language: Option<&str>) {
        self.transcribe_config.language = match language {
            Some(code) => LanguageSetting::Fixed(code.to_string()),
            None => LanguageSetting::Auto,
        };
    }

    fn set_fallback_language(&mut self, language: Option<&str>) {
        self.transcribe_config.fallback_language = language.map(str::to_string);
    }

    fn set_vocabulary(&mut self, words: Vec<String>) {
        self.transcribe_config.vocabulary = words;
    }

    fn last_transcript(&self) -> Option<&str> {
        self.last_transcript.as_deref()
    }

    fn last_raw_transcript(&self) -> Option<&str> {
        self.last_raw_transcript.as_deref()
    }

    fn set_cleanup_provider(&mut self, kind: CleanupProviderKind) {
        self.cleanup_provider = kind;
    }

    fn set_cleanup_strength(&mut self, strength: CleanupStrength) {
        self.cleanup_strength = strength;
    }

    fn set_cleanup_local_model_path(&mut self, path: PathBuf) {
        self.cleanup_local_model_path = path;
        self.local_cleanup = None;
    }

    fn set_blocklist_words(&mut self, words: Vec<String>) {
        self.blocklist_words = words;
    }

    fn set_cleanup_cloud_config(&mut self, base_url: String, api_key: String, model: String) {
        self.cloud_base_url = Some(base_url);
        self.cloud_api_key = Some(api_key);
        self.cloud_model = Some(model);
        // Rebuilt lazily on next use with the new config.
        self.cloud_cleanup = None;
    }

    /// Lets the Swift shell hand back a *Cleaned Transcript* it produced
    /// itself via Apple's Foundation Models framework (`CleanupProviderKind::Apple`
    /// — see that variant's doc for why Rust can't do this step itself).
    fn set_cleaned_transcript(&mut self, text: String) -> StatusCode {
        if self.cleanup_provider != CleanupProviderKind::Apple {
            return StatusCode::IllegalState;
        }
        self.last_transcript = Some(text);
        StatusCode::Ok
    }

    fn write_verification_wav(&self, path: &Path) -> StatusCode {
        let Some(samples) = &self.last_samples else {
            return StatusCode::NotRecording;
        };
        match audio::write_verification_wav(samples, WHISPER_SAMPLE_RATE, path) {
            Ok(()) => StatusCode::Ok,
            Err(_) => StatusCode::WavWriteFailed,
        }
    }

    fn reset(&mut self) -> StatusCode {
        match self.session.apply(SessionEvent::Reset) {
            Ok(_) => StatusCode::Ok,
            Err(_) => StatusCode::IllegalState,
        }
    }

    /// Drops the cached Whisper transcriber and Cleanup Pass providers,
    /// freeing whatever memory they hold (the Whisper model, and/or the
    /// local llama.cpp GGUF model, up to a couple GB combined). They're
    /// otherwise cached for the app's lifetime once loaded — see the
    /// `transcriber`/`local_cleanup`/`cloud_cleanup` fields above — so
    /// this exists purely for the Menu Bar Icon's Disable action, which
    /// should actually release resources rather than only gate the
    /// hotkey. Next use after this lazily reloads from disk, same as a
    /// fresh launch.
    fn unload_cached_models(&mut self) {
        self.transcriber = None;
        self.local_cleanup = None;
        self.cloud_cleanup = None;
    }

    fn state(&self) -> SessionState {
        self.session.state()
    }

    /// Current input level (0.0-1.0), for the Dictation HUD's live waveform.
    /// 0.0 whenever not actively recording — there's nothing to meter.
    fn current_input_level(&self) -> f32 {
        self.capture
            .as_ref()
            .map(|c| c.current_level())
            .unwrap_or(0.0)
    }
}

fn state_code(state: SessionState) -> i32 {
    match state {
        SessionState::Idle => 0,
        SessionState::Recording => 1,
        SessionState::Processing => 2,
        SessionState::Done => 3,
        SessionState::Discarded => 4,
        SessionState::Error => 5,
        SessionState::NoSpeech => 6,
    }
}

/// Default location for the bundled/downloaded whisper.cpp model. See
/// `docs/adr/0002-whisper-model-bundling.md` for why this lives under
/// Application Support rather than inside the app bundle.
fn default_model_path() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Library/Application Support/VoiceDrop/models/ggml-small.bin")
}

/// Default location for the local Cleanup Pass GGUF model — same rationale
/// as `default_model_path`, downloaded on first use (ADR-0004/ADR-0008).
/// `scripts/download-cleanup-model.sh` fetches Qwen2.5-1.5B-Instruct here by
/// default — chosen over 0.5B (too weak) and Llama-3.2-3B (ignored the
/// VerbatimClean strength's "preserve exact wording" instruction even more
/// than the smaller models) after manual side-by-side comparison. See the
/// "Local provider" todo in `0004-phase3-cleanup-pass.md`.
fn default_cleanup_model_path() -> PathBuf {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Library/Application Support/VoiceDrop/models/qwen2.5-1.5b-instruct-q4_k_m.gguf")
}

/// # Safety
/// Returned pointer must be freed exactly once via `voicedrop_engine_free`,
/// and all other `voicedrop_engine_*` calls on it must happen before that.
#[no_mangle]
pub extern "C" fn voicedrop_engine_new() -> *mut Engine {
    Box::into_raw(Box::new(Engine::new()))
}

/// # Safety
/// `engine` must be a pointer previously returned by `voicedrop_engine_new`
/// that has not already been freed.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_free(engine: *mut Engine) {
    if engine.is_null() {
        return;
    }
    drop(Box::from_raw(engine));
}

/// # Safety
/// `engine` must be a valid, non-null pointer from `voicedrop_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_start_recording(engine: *mut Engine) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    engine.start_recording() as i32
}

/// # Safety
/// `engine` must be a valid, non-null pointer from `voicedrop_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_stop_recording(engine: *mut Engine) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    engine.stop_recording() as i32
}

/// Writes the most recently captured session's audio to a WAV file at
/// `path` (UTF-8, NUL-terminated) for manual listening verification.
///
/// # Safety
/// `engine` must be valid; `path` must be a valid NUL-terminated UTF-8 C
/// string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_write_verification_wav(
    engine: *mut Engine,
    path: *const c_char,
) -> i32 {
    let Some(engine) = engine.as_ref() else {
        return StatusCode::IllegalState as i32;
    };
    if path.is_null() {
        return StatusCode::InvalidPath as i32;
    }
    let Ok(path_str) = CStr::from_ptr(path).to_str() else {
        return StatusCode::InvalidPath as i32;
    };
    engine.write_verification_wav(Path::new(path_str)) as i32
}

/// # Safety
/// `engine` must be a valid, non-null pointer from `voicedrop_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_reset(engine: *mut Engine) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    engine.reset() as i32
}

/// # Safety
/// `engine` must be a valid, non-null pointer from `voicedrop_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_unload_models(engine: *mut Engine) {
    let Some(engine) = engine.as_mut() else {
        return;
    };
    engine.unload_cached_models();
}

/// # Safety
/// `engine` must be a valid, non-null pointer from `voicedrop_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_state(engine: *const Engine) -> i32 {
    let Some(engine) = engine.as_ref() else {
        return -1;
    };
    state_code(engine.state())
}

/// Current input level (0.0-1.0) for the Dictation HUD's live waveform.
/// Meant to be polled on a UI timer while `voicedrop_engine_state` reports
/// `VOICEDROP_STATE_RECORDING`; returns 0.0 at all other times, and if
/// `engine` is NULL.
///
/// # Safety
/// `engine` must be a valid, non-null pointer from `voicedrop_engine_new`,
/// or NULL.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_current_input_level(engine: *const Engine) -> f32 {
    let Some(engine) = engine.as_ref() else {
        return 0.0;
    };
    engine.current_input_level()
}

/// Overrides the whisper.cpp model file path (defaults to a path under
/// Application Support — see `docs/adr/0002-whisper-model-bundling.md`).
///
/// # Safety
/// `engine` must be valid; `path` must be a valid NUL-terminated UTF-8 C
/// string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_set_model_path(
    engine: *mut Engine,
    path: *const c_char,
) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    if path.is_null() {
        return StatusCode::InvalidPath as i32;
    }
    let Ok(path_str) = CStr::from_ptr(path).to_str() else {
        return StatusCode::InvalidPath as i32;
    };
    engine.set_model_path(PathBuf::from(path_str));
    StatusCode::Ok as i32
}

/// Sets the language Whisper should transcribe in. Pass NULL for
/// auto-detect (falls back to `voicedrop_engine_set_fallback_language` on
/// clips under `transcribe::AUTO_DETECT_MIN_MS`); otherwise pass an ISO
/// 639-1 code such as "en", "fr", "de".
///
/// # Safety
/// `engine` must be valid; `language` must be NULL or a valid
/// NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_set_language(
    engine: *mut Engine,
    language: *const c_char,
) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    if language.is_null() {
        engine.set_language(None);
        return StatusCode::Ok as i32;
    }
    let Ok(lang_str) = CStr::from_ptr(language).to_str() else {
        return StatusCode::InvalidPath as i32;
    };
    engine.set_language(Some(lang_str));
    StatusCode::Ok as i32
}

/// Sets the language used when auto-detect is selected but the clip is too
/// short to auto-detect reliably. Pass NULL to clear it (auto-detect is then
/// attempted regardless of clip length).
///
/// # Safety
/// `engine` must be valid; `language` must be NULL or a valid
/// NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_set_fallback_language(
    engine: *mut Engine,
    language: *const c_char,
) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    if language.is_null() {
        engine.set_fallback_language(None);
        return StatusCode::Ok as i32;
    }
    let Ok(lang_str) = CStr::from_ptr(language).to_str() else {
        return StatusCode::InvalidPath as i32;
    };
    engine.set_fallback_language(Some(lang_str));
    StatusCode::Ok as i32
}

/// Sets the *Custom Vocabulary* bias list, as a comma-separated list of
/// words/phrases. Pass NULL or an empty string to clear it. Foundation-only
/// plumbing for now — Phase 5 owns the editable list UI.
///
/// # Safety
/// `engine` must be valid; `words` must be NULL or a valid NUL-terminated
/// UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_set_vocabulary(
    engine: *mut Engine,
    words: *const c_char,
) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    if words.is_null() {
        engine.set_vocabulary(Vec::new());
        return StatusCode::Ok as i32;
    }
    let Ok(words_str) = CStr::from_ptr(words).to_str() else {
        return StatusCode::InvalidPath as i32;
    };
    let list = words_str
        .split(',')
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    engine.set_vocabulary(list);
    StatusCode::Ok as i32
}

/// Sets custom words to always strip from every Raw Transcript, as a
/// comma-separated list — merged with a small built-in default list (see
/// `blocklist.rs`), applied unconditionally regardless of Cleanup Pass
/// provider (including `None`). Pass NULL or an empty string to clear
/// custom words (the built-in defaults still apply).
///
/// # Safety
/// `engine` must be valid; `words` must be NULL or a valid NUL-terminated
/// UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_set_blocklist(
    engine: *mut Engine,
    words: *const c_char,
) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    if words.is_null() {
        engine.set_blocklist_words(Vec::new());
        return StatusCode::Ok as i32;
    }
    let Ok(words_str) = CStr::from_ptr(words).to_str() else {
        return StatusCode::InvalidPath as i32;
    };
    let list = words_str
        .split(',')
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    engine.set_blocklist_words(list);
    StatusCode::Ok as i32
}

/// Returns the most recent *Raw Transcript* as an owned, NUL-terminated C
/// string, or NULL if no session has produced one yet. Caller must free the
/// result with `voicedrop_core_free_string`.
///
/// # Safety
/// `engine` must be a valid, non-null pointer from `voicedrop_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_last_transcript(engine: *const Engine) -> *mut c_char {
    let Some(engine) = engine.as_ref() else {
        return std::ptr::null_mut();
    };
    match engine.last_transcript() {
        Some(text) => std::ffi::CString::new(text)
            .map(std::ffi::CString::into_raw)
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// Returns the most recent *Raw Transcript* (pre-Cleanup-Pass) as an owned,
/// NUL-terminated C string, or NULL if no session has produced one yet.
/// Caller must free the result with `voicedrop_core_free_string`.
///
/// # Safety
/// `engine` must be a valid, non-null pointer from `voicedrop_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_last_raw_transcript(
    engine: *const Engine,
) -> *mut c_char {
    let Some(engine) = engine.as_ref() else {
        return std::ptr::null_mut();
    };
    match engine.last_raw_transcript() {
        Some(text) => std::ffi::CString::new(text)
            .map(std::ffi::CString::into_raw)
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// Selects the Cleanup Pass provider at runtime — a config change, not a
/// code change, per the "Provider interface" todo. `kind` is one of the
/// `VOICEDROP_CLEANUP_*` constants.
///
/// # Safety
/// `engine` must be a valid, non-null pointer from `voicedrop_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_set_cleanup_provider(
    engine: *mut Engine,
    kind: i32,
) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    let kind = match kind {
        0 => CleanupProviderKind::None,
        1 => CleanupProviderKind::Local,
        2 => CleanupProviderKind::Apple,
        3 => CleanupProviderKind::Cloud,
        _ => return StatusCode::IllegalState as i32,
    };
    engine.set_cleanup_provider(kind);
    StatusCode::Ok as i32
}

/// Sets the *Cleanup Strength* level. `strength` is one of the
/// `VOICEDROP_STRENGTH_*` constants.
///
/// # Safety
/// `engine` must be a valid, non-null pointer from `voicedrop_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_set_cleanup_strength(
    engine: *mut Engine,
    strength: i32,
) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    let strength = match strength {
        0 => CleanupStrength::VerbatimClean,
        1 => CleanupStrength::LightEdit,
        2 => CleanupStrength::FormalRewrite,
        _ => return StatusCode::IllegalState as i32,
    };
    engine.set_cleanup_strength(strength);
    StatusCode::Ok as i32
}

/// Overrides the local Cleanup Pass GGUF model path (defaults to a path
/// under Application Support — see `default_cleanup_model_path`).
///
/// # Safety
/// `engine` must be valid; `path` must be a valid NUL-terminated UTF-8 C
/// string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_set_cleanup_local_model_path(
    engine: *mut Engine,
    path: *const c_char,
) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    if path.is_null() {
        return StatusCode::InvalidPath as i32;
    }
    let Ok(path_str) = CStr::from_ptr(path).to_str() else {
        return StatusCode::InvalidPath as i32;
    };
    engine.set_cleanup_local_model_path(PathBuf::from(path_str));
    StatusCode::Ok as i32
}

/// Configures the cloud Cleanup Pass provider: a free-form base URL (e.g.
/// `https://api.openai.com/v1`, or `http://localhost:11434/v1` to bring
/// your own model via Ollama, or any other local runner's OpenAI-compatible
/// address) plus API key and model name, assumed OpenAI-compatible per
/// ADR-0005. All three are required (non-NULL, non-empty) UTF-8 C strings —
/// for a local server that doesn't check the key, pass any non-empty
/// placeholder.
///
/// # Safety
/// `engine` must be valid; `base_url`, `api_key`, and `model` must each be a
/// valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_set_cleanup_cloud_config(
    engine: *mut Engine,
    base_url: *const c_char,
    api_key: *const c_char,
    model: *const c_char,
) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    if base_url.is_null() || api_key.is_null() || model.is_null() {
        return StatusCode::InvalidPath as i32;
    }
    let (Ok(base_url), Ok(api_key), Ok(model)) = (
        CStr::from_ptr(base_url).to_str(),
        CStr::from_ptr(api_key).to_str(),
        CStr::from_ptr(model).to_str(),
    ) else {
        return StatusCode::InvalidPath as i32;
    };
    engine.set_cleanup_cloud_config(base_url.to_string(), api_key.to_string(), model.to_string());
    StatusCode::Ok as i32
}

/// Lets the Swift shell hand back a *Cleaned Transcript* it produced itself
/// via Apple's Foundation Models framework, when the Cleanup Pass provider
/// is set to `VOICEDROP_CLEANUP_APPLE` (Rust can't call that framework
/// directly — see `CleanupProviderKind::Apple`'s doc in `engine.rs`).
/// Returns `VOICEDROP_ERR_ILLEGAL_STATE` if the provider isn't `Apple`.
///
/// # Safety
/// `engine` must be valid; `text` must be a valid NUL-terminated UTF-8 C
/// string.
#[no_mangle]
pub unsafe extern "C" fn voicedrop_engine_set_cleaned_transcript(
    engine: *mut Engine,
    text: *const c_char,
) -> i32 {
    let Some(engine) = engine.as_mut() else {
        return StatusCode::IllegalState as i32;
    };
    if text.is_null() {
        return StatusCode::InvalidPath as i32;
    }
    let Ok(text_str) = CStr::from_ptr(text).to_str() else {
        return StatusCode::InvalidPath as i32;
    };
    engine.set_cleaned_transcript(text_str.to_string()) as i32
}

/// Returns the shared Cleanup Pass system prompt for a given
/// `VOICEDROP_STRENGTH_*` value, as an owned NUL-terminated C string. Lets
/// the Swift shell's Apple Foundation Models provider reuse the exact same
/// prompt wording as the local/cloud providers instead of duplicating it —
/// see `cleanup::system_prompt`'s doc. Returns NULL for an invalid
/// `strength` value. Caller must free the result with
/// `voicedrop_core_free_string`.
#[no_mangle]
pub extern "C" fn voicedrop_cleanup_prompt_for_strength(strength: i32) -> *mut c_char {
    let strength = match strength {
        0 => CleanupStrength::VerbatimClean,
        1 => CleanupStrength::LightEdit,
        2 => CleanupStrength::FormalRewrite,
        _ => return std::ptr::null_mut(),
    };
    std::ffi::CString::new(crate::cleanup::system_prompt(strength))
        .map(std::ffi::CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}
