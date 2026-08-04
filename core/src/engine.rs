//! C ABI surface for the Swift shell. Wraps `Session` + `AudioCapture` behind
//! an opaque handle. Intended to be driven from a single thread (the Swift
//! main thread reacting to CGEventTap callbacks) — no internal locking is
//! provided beyond what `AudioCapture`'s realtime callback already needs.

use crate::audio::{self, AudioCapture, AudioError, WHISPER_SAMPLE_RATE};
use crate::session::{Session, SessionEvent, SessionState};
use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::Path;

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
}

pub struct Engine {
    session: Session,
    capture: Option<AudioCapture>,
    /// Last stopped session's mono 16kHz samples, kept only so a follow-up
    /// call can write the verification WAV without re-capturing.
    last_samples: Option<Vec<f32>>,
}

impl Engine {
    fn new() -> Self {
        Engine {
            session: Session::new(),
            capture: None,
            last_samples: None,
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

    /// Stops recording, resamples to 16kHz mono, and — since Phase 2/3 don't
    /// exist yet — immediately marks processing as succeeded. Once real STT
    /// and Cleanup Pass wiring lands, this should stop short at `Processing`
    /// and let that pipeline drive the rest of the transition.
    fn stop_recording(&mut self) -> StatusCode {
        let Some(capture) = self.capture.take() else {
            return StatusCode::NotRecording;
        };
        if self.session.apply(SessionEvent::HotkeyUp).is_err() {
            return StatusCode::IllegalState;
        }

        match capture.stop() {
            Ok(samples) => {
                self.last_samples = Some(samples);
                let _ = self.session.apply(SessionEvent::ProcessingSucceeded);
                StatusCode::Ok
            }
            Err(AudioError::DeviceDisconnected) => {
                let _ = self.session.apply(SessionEvent::ProcessingFailed);
                StatusCode::DeviceDisconnected
            }
            Err(_) => {
                let _ = self.session.apply(SessionEvent::ProcessingFailed);
                StatusCode::AudioOther
            }
        }
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
