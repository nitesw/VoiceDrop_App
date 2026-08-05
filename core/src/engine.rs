//! C ABI surface for the Swift shell. Wraps `Session` + `AudioCapture` behind
//! an opaque handle. Intended to be driven from a single thread (the Swift
//! main thread reacting to CGEventTap callbacks) — no internal locking is
//! provided beyond what `AudioCapture`'s realtime callback already needs.

use crate::audio::{self, AudioCapture, AudioError, WHISPER_SAMPLE_RATE};
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
}

pub struct Engine {
    session: Session,
    capture: Option<AudioCapture>,
    /// Last stopped session's mono 16kHz samples, kept only so a follow-up
    /// call can write the verification WAV without re-capturing.
    last_samples: Option<Vec<f32>>,
    /// The most recent *Raw Transcript*, once STT has produced one.
    last_transcript: Option<String>,
    model_path: PathBuf,
    /// Loaded lazily on first use — loading a whisper.cpp model takes
    /// seconds, so it's kept alive across sessions rather than per-utterance.
    transcriber: Option<Transcriber>,
    transcribe_config: TranscribeConfig,
}

impl Engine {
    fn new() -> Self {
        Engine {
            session: Session::new(),
            capture: None,
            last_samples: None,
            last_transcript: None,
            model_path: default_model_path(),
            transcriber: None,
            transcribe_config: TranscribeConfig::default(),
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
                self.last_transcript = Some(text);
                let _ = self.session.apply(SessionEvent::ProcessingSucceeded);
                StatusCode::Ok
            }
            Err(TranscribeError::TranscriptionFailed(_))
            | Err(TranscribeError::ModelLoadFailed(_))
            | Err(TranscribeError::ModelNotFound(_)) => {
                let _ = self.session.apply(SessionEvent::ProcessingFailed);
                StatusCode::TranscriptionFailed
            }
        }
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

    fn state(&self) -> SessionState {
        self.session.state()
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
pub unsafe extern "C" fn voicedrop_engine_state(engine: *const Engine) -> i32 {
    let Some(engine) = engine.as_ref() else {
        return -1;
    };
    state_code(engine.state())
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
