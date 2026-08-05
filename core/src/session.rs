//! The *Dictation Session* state machine (see CONTEXT.md). Pure Rust, no FFI —
//! the Swift shell drives this via events crossing the FFI boundary in a
//! later step, but the transition rules themselves don't need to know that.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    Recording,
    Processing,
    Done,
    /// No Phase 1 event reaches this yet — reachable once "scratch that"
    /// exists (Phase 6). Defined now so the state space doesn't need
    /// reshaping later.
    Discarded,
    /// Reached instead of `Done` when the *Raw Transcript* is empty (silence
    /// or an accidental brief tap) — short-circuits before the Cleanup Pass,
    /// which shouldn't need to handle empty input.
    NoSpeech,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    /// Push-to-Talk Hotkey pressed.
    HotkeyDown,
    /// Push-to-Talk Hotkey released.
    HotkeyUp,
    /// Pipeline (STT + Cleanup Pass) finished successfully.
    ProcessingSucceeded,
    /// Pipeline failed at any stage.
    ProcessingFailed,
    /// The *Raw Transcript* came back empty (silence-only audio, or the
    /// buffer was too short to bother running STT on at all).
    NoSpeechDetected,
    /// Audio capture itself failed (e.g. input device disconnected) before
    /// there was anything to process.
    RecordingFailed,
    /// Session was explicitly discarded (e.g. a future "scratch that").
    Discard,
    /// Return to Idle after a terminal state, ready for the next session.
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IllegalTransition {
    pub from: SessionState,
    pub event: SessionEvent,
}

/// A single Dictation Session's state. Owns no audio/transcript data itself —
/// this is purely the state machine; payloads are threaded through by the
/// caller as they become available in later phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    state: SessionState,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Session {
            state: SessionState::Idle,
        }
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Attempts the transition for `event`. On success, updates `self` and
    /// returns the new state. On an illegal transition, `self` is left
    /// unchanged and the error describes what was rejected.
    pub fn apply(&mut self, event: SessionEvent) -> Result<SessionState, IllegalTransition> {
        use SessionEvent::*;
        use SessionState::*;

        let next = match (self.state, event) {
            (Idle, HotkeyDown) => Recording,
            (Recording, HotkeyUp) => Processing,
            (Recording, Discard) => Discarded,
            (Recording, RecordingFailed) => Error,
            (Processing, ProcessingSucceeded) => Done,
            (Processing, ProcessingFailed) => Error,
            (Processing, NoSpeechDetected) => NoSpeech,
            (Processing, Discard) => Discarded,
            (Done, Reset) | (Discarded, Reset) | (Error, Reset) | (NoSpeech, Reset) => Idle,
            _ => {
                return Err(IllegalTransition {
                    from: self.state,
                    event,
                })
            }
        };

        self.state = next;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SessionEvent::*;
    use SessionState::*;

    #[test]
    fn starts_idle() {
        assert_eq!(Session::new().state(), Idle);
    }

    #[test]
    fn happy_path_idle_to_done_to_idle() {
        let mut s = Session::new();
        assert_eq!(s.apply(HotkeyDown), Ok(Recording));
        assert_eq!(s.apply(HotkeyUp), Ok(Processing));
        assert_eq!(s.apply(ProcessingSucceeded), Ok(Done));
        assert_eq!(s.apply(Reset), Ok(Idle));
    }

    #[test]
    fn processing_failure_reaches_error_then_resets() {
        let mut s = Session::new();
        s.apply(HotkeyDown).unwrap();
        s.apply(HotkeyUp).unwrap();
        assert_eq!(s.apply(ProcessingFailed), Ok(Error));
        assert_eq!(s.apply(Reset), Ok(Idle));
    }

    #[test]
    fn discard_from_recording_or_processing_reaches_discarded() {
        let mut s = Session::new();
        s.apply(HotkeyDown).unwrap();
        assert_eq!(s.apply(Discard), Ok(Discarded));

        let mut s2 = Session::new();
        s2.apply(HotkeyDown).unwrap();
        s2.apply(HotkeyUp).unwrap();
        assert_eq!(s2.apply(Discard), Ok(Discarded));
    }

    #[test]
    fn illegal_transitions_are_rejected_and_state_is_unchanged() {
        let mut s = Session::new();
        let err = s.apply(HotkeyUp).unwrap_err();
        assert_eq!(
            err,
            IllegalTransition {
                from: Idle,
                event: HotkeyUp
            }
        );
        // State machine must not have moved on a rejected transition.
        assert_eq!(s.state(), Idle);
    }

    #[test]
    fn cannot_skip_recording_straight_to_processing_success() {
        let mut s = Session::new();
        assert!(s.apply(ProcessingSucceeded).is_err());
        assert_eq!(s.state(), Idle);
    }

    #[test]
    fn recording_failure_e_g_device_disconnect_reaches_error_then_resets() {
        let mut s = Session::new();
        s.apply(HotkeyDown).unwrap();
        assert_eq!(s.apply(RecordingFailed), Ok(Error));
        assert_eq!(s.apply(Reset), Ok(Idle));
    }

    #[test]
    fn no_speech_detected_reaches_no_speech_then_resets() {
        let mut s = Session::new();
        s.apply(HotkeyDown).unwrap();
        s.apply(HotkeyUp).unwrap();
        assert_eq!(s.apply(NoSpeechDetected), Ok(NoSpeech));
        assert_eq!(s.apply(Reset), Ok(Idle));
    }

    #[test]
    fn cannot_reset_from_non_terminal_states() {
        let mut s = Session::new();
        assert!(s.apply(Reset).is_err());
        s.apply(HotkeyDown).unwrap();
        assert!(s.apply(Reset).is_err());
    }
}
