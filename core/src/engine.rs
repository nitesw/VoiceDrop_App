//! C ABI surface for the Swift shell. Wraps `Session` + `AudioCapture` behind
//! an opaque handle. Intended to be driven from a single thread (the Swift
//! side's serial `engineQueue`, kept off the CGEventTap callback because
//! opening the mic can take seconds) — no internal locking is
//! provided beyond what `AudioCapture`'s realtime callback already needs.

use crate::audio::{AudioCapture, AudioError};
use crate::models;
use crate::session::{Session, SessionEvent, SessionState};
use crate::transcribe::{self, LanguageSetting, TranscribeConfig, TranscribeError, Transcriber};
use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::PathBuf;

#[repr(i32)]
pub enum StatusCode {
    Ok = 0,
    IllegalState = 1,
    NoInputDevice = 2,
    AudioOther = 3,
    NotRecording = 4,
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
    /// The last session's transcript, ready to drop at the cursor.
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
            last_transcript: None,
            model_path: models::path(),
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
    /// Whisper to produce the transcript. Short/silent buffers
    /// short-circuit to `NoSpeech` before Whisper ever runs.
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

    fn last_transcript(&self) -> Option<&str> {
        self.last_transcript.as_deref()
    }

    fn reset(&mut self) -> StatusCode {
        match self.session.apply(SessionEvent::Reset) {
            Ok(_) => StatusCode::Ok,
            Err(_) => StatusCode::IllegalState,
        }
    }

    /// Drops the cached Whisper transcriber, freeing the memory the model
    /// holds. It's otherwise cached for the app's lifetime once loaded —
    /// see the `transcriber` field above — so this exists purely for the Menu Bar Icon's Disable action, which
    /// should actually release resources rather than only gate the
    /// hotkey. Next use after this lazily reloads from disk, same as a
    /// fresh launch.
    fn unload_cached_models(&mut self) {
        self.transcriber = None;
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

/// Overrides the whisper.cpp model file path (defaults to `models::path()`,
/// under Application Support — see ADR-0004).
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

/// Returns the most recent transcript as an owned, NUL-terminated C
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
