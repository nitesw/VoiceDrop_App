//! Speech-to-text via whisper.cpp (`whisper-rs`). Turns the mono 16 kHz
//! buffer `audio::AudioCapture::stop` produces into the transcript that gets
//! dropped at the cursor. There is no separate cleanup step (see ADR-0009):
//! Whisper's own punctuation is what the user gets.

use std::path::{Path, PathBuf};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::audio::WHISPER_SAMPLE_RATE;

/// Below this duration, audio is skipped entirely — not worth the cost of
/// loading Whisper for what's almost certainly an accidental brief tap.
/// See `0003-phase2-stt.md` "Silence / no-speech handling".
pub const MIN_SPEECH_MS: u64 = 300;

/// Below this duration, Whisper's language auto-detect is unreliable (not
/// enough audio to fingerprint), so auto-detect falls back to the
/// configured default language instead of guessing.
pub const AUTO_DETECT_MIN_MS: u64 = 2_000;

#[derive(Debug)]
pub enum TranscribeError {
    ModelNotFound(PathBuf),
    ModelLoadFailed(String),
    TranscriptionFailed(String),
}

/// Explicit selection vs. auto-detect, per the "Language handling" todo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LanguageSetting {
    #[default]
    Auto,
    Fixed(String),
}

/// Everything Whisper needs beyond the raw samples themselves.
#[derive(Debug, Clone, Default)]
pub struct TranscribeConfig {
    pub language: LanguageSetting,
    /// Used instead of auto-detect when the clip is under `AUTO_DETECT_MIN_MS`.
    pub fallback_language: Option<String>,
}

/// Duration of a mono buffer sampled at `WHISPER_SAMPLE_RATE`, in ms.
pub fn duration_ms(samples: &[f32]) -> u64 {
    (samples.len() as u64 * 1000) / WHISPER_SAMPLE_RATE as u64
}

/// True if `samples` is too short to bother transcribing at all.
pub fn is_too_short(samples: &[f32]) -> bool {
    duration_ms(samples) < MIN_SPEECH_MS
}

/// Resolves which language code (if any) to pass to whisper.cpp, applying
/// the short-clip auto-detect fallback described in the todo.
fn resolve_language<'a>(config: &'a TranscribeConfig, samples: &[f32]) -> Option<&'a str> {
    match &config.language {
        LanguageSetting::Fixed(code) => Some(code.as_str()),
        LanguageSetting::Auto => {
            if duration_ms(samples) < AUTO_DETECT_MIN_MS {
                config.fallback_language.as_deref()
            } else {
                None
            }
        }
    }
}

/// Known literal placeholder tokens whisper.cpp emits *as transcribed text*
/// for silent/non-speech audio, instead of just returning nothing — e.g.
/// `[BLANK_AUDIO]` observed in practice. Without filtering these, they slip
/// past the empty-string check as if they were real speech and get injected
/// verbatim, which is never correct output.
const BLANK_AUDIO_MARKERS: &[&str] = &[
    "[blank_audio]",
    "[silence]",
    "[no speech]",
    "[no audio]",
    "(silence)",
    "(inaudible)",
    "[inaudible]",
];

/// True if `text` (already trimmed) is nothing but one of Whisper's known
/// non-speech placeholder markers, or generically looks like one: entirely
/// wrapped in a single matching pair of bracket-like delimiters with
/// nothing outside them. The generic check is a catch-all for marker
/// variants not in the curated list above — this whole family of Whisper
/// output shares that wrapped shape, whether it's `[BLANK_AUDIO]`,
/// `(inaudible)`, or `*laughter*`/`*maniacal chatter*` (Whisper's
/// asterisk-wrapped style for describing non-speech sounds — laughter,
/// music, background chatter — instead of transcribing words).
fn is_blank_audio_marker(text: &str) -> bool {
    let lower = text.to_lowercase();
    if BLANK_AUDIO_MARKERS.contains(&lower.as_str()) {
        return true;
    }
    const DELIMITER_PAIRS: &[(char, char)] = &[('[', ']'), ('(', ')'), ('*', '*')];
    DELIMITER_PAIRS.iter().any(|&(open, close)| {
        if !text.starts_with(open) || !text.ends_with(close) || text.len() <= 1 {
            return false;
        }
        // When open == close (asterisk-style), the pair shares one count;
        // otherwise count each delimiter separately — either way, "nothing
        // outside the wrapping pair" means exactly 2 delimiter characters
        // total.
        let delimiter_count = if open == close {
            text.matches(open).count()
        } else {
            text.matches(open).count() + text.matches(close).count()
        };
        delimiter_count == 2
    })
}

/// Loads a whisper.cpp model once and transcribes buffers against it.
/// Loading is expensive (seconds), so callers should keep one `Transcriber`
/// alive across sessions rather than reloading per-utterance.
pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    pub fn load(model_path: &Path) -> Result<Self, TranscribeError> {
        if !model_path.is_file() {
            return Err(TranscribeError::ModelNotFound(model_path.to_path_buf()));
        }
        // GPU (Metal) is on by default when built with the `metal` feature;
        // flash attention is a further speedup on it.
        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.flash_attn(true);
        let ctx = WhisperContext::new_with_params(model_path, ctx_params)
            .map_err(|e| TranscribeError::ModelLoadFailed(e.to_string()))?;
        Ok(Transcriber { ctx })
    }

    /// Runs Whisper over `samples` (mono, `WHISPER_SAMPLE_RATE`, already
    /// validated by `is_too_short` at the caller) and returns the
    /// transcript text. An empty/whitespace-only result means near-silent
    /// audio that produced no intelligible speech — callers should treat
    /// that the same as "too short", per the "Silence / no-speech handling"
    /// todo.
    pub fn transcribe(
        &self,
        samples: &[f32],
        config: &TranscribeConfig,
    ) -> Result<String, TranscribeError> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| TranscribeError::TranscriptionFailed(e.to_string()))?;

        // Beam search keeps several candidate transcriptions and picks the
        // most likely one, instead of committing to the first guess for
        // every token. More accurate, and cheap enough on the GPU.
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        });
        // Don't emit non-speech tokens like "[music]" or "(laughs)".
        params.set_suppress_nst(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Always set this explicitly: whisper-rs defaults to "en" when it's
        // never called, so leaving it unset for auto-detect silently forced
        // English — and Whisper given non-English audio with language "en"
        // *translates* it instead of transcribing. `None` means auto-detect.
        params.set_language(resolve_language(config, samples));

        state
            .full(params, samples)
            .map_err(|e| TranscribeError::TranscriptionFailed(e.to_string()))?;

        let num_segments = state.full_n_segments();

        let mut text = String::new();
        for i in 0..num_segments {
            let segment = state
                .get_segment(i)
                .ok_or_else(|| {
                    TranscribeError::TranscriptionFailed(format!("missing segment {i}"))
                })?
                .to_str_lossy()
                .map_err(|e| TranscribeError::TranscriptionFailed(e.to_string()))?;
            text.push_str(&segment);
        }

        let text = text.trim().to_string();
        if is_blank_audio_marker(&text) {
            return Ok(String::new());
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_known_blank_audio_markers_case_insensitively() {
        assert!(is_blank_audio_marker("[BLANK_AUDIO]"));
        assert!(is_blank_audio_marker("[blank_audio]"));
        assert!(is_blank_audio_marker("(inaudible)"));
    }

    #[test]
    fn recognizes_unlisted_bracketed_markers_generically() {
        assert!(is_blank_audio_marker("[background noise]"));
        assert!(is_blank_audio_marker("(coughing)"));
    }

    #[test]
    fn recognizes_asterisk_wrapped_markers() {
        assert!(is_blank_audio_marker("*laughter*"));
        assert!(is_blank_audio_marker("*Maniacal chatter*"));
        assert!(is_blank_audio_marker("*music playing*"));
    }

    #[test]
    fn does_not_flag_text_containing_asterisks_mid_sentence() {
        // A real sentence that merely mentions asterisks shouldn't be
        // mistaken for a wrapped marker: it doesn't start AND end with '*',
        // and/or has more than two '*' characters total.
        assert!(!is_blank_audio_marker("use a * to multiply, like 2 * 3"));
    }

    #[test]
    fn does_not_flag_real_speech() {
        assert!(!is_blank_audio_marker("Hello, how are you?"));
        assert!(!is_blank_audio_marker("I said (by the way) that it works"));
    }

    #[test]
    fn does_not_flag_empty_string() {
        // Empty is handled separately (already means no-speech) — the
        // marker check is about non-empty placeholder text specifically.
        assert!(!is_blank_audio_marker(""));
    }

    fn samples_of_duration_ms(ms: u64) -> Vec<f32> {
        vec![0.0; (WHISPER_SAMPLE_RATE as u64 * ms / 1000) as usize]
    }

    #[test]
    fn short_clip_is_too_short() {
        assert!(is_too_short(&samples_of_duration_ms(100)));
    }

    #[test]
    fn clip_at_threshold_is_not_too_short() {
        assert!(!is_too_short(&samples_of_duration_ms(MIN_SPEECH_MS)));
    }

    #[test]
    fn empty_buffer_is_too_short() {
        assert!(is_too_short(&[]));
    }

    #[test]
    fn fixed_language_wins_regardless_of_duration() {
        let config = TranscribeConfig {
            language: LanguageSetting::Fixed("fr".to_string()),
            fallback_language: Some("en".to_string()),
        };
        assert_eq!(
            resolve_language(&config, &samples_of_duration_ms(5_000)),
            Some("fr")
        );
    }

    #[test]
    fn auto_on_short_clip_falls_back_to_configured_language() {
        let config = TranscribeConfig {
            language: LanguageSetting::Auto,
            fallback_language: Some("en".to_string()),
        };
        assert_eq!(
            resolve_language(&config, &samples_of_duration_ms(500)),
            Some("en")
        );
    }

    #[test]
    fn auto_on_long_clip_lets_whisper_detect() {
        let config = TranscribeConfig {
            language: LanguageSetting::Auto,
            fallback_language: Some("en".to_string()),
        };
        assert_eq!(
            resolve_language(&config, &samples_of_duration_ms(5_000)),
            None
        );
    }

    #[test]
    fn missing_model_file_is_reported() {
        let result = Transcriber::load(Path::new("/nonexistent/model.bin"));
        assert!(matches!(result, Err(TranscribeError::ModelNotFound(_))));
    }
}
