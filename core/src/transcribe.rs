//! Speech-to-text via whisper.cpp (`whisper-rs`). Turns the mono 16 kHz
//! buffer `audio::AudioCapture::stop` produces into a *Raw Transcript*
//! string. No filler-word removal, punctuation, or grammar correction here
//! — that's the *Cleanup Pass* in Phase 3.

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
    /// *Custom Vocabulary* bias list — fed to whisper.cpp as an initial
    /// prompt. Populated by Phase 5's editable list; empty for now.
    pub vocabulary: Vec<String>,
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

/// Joins the *Custom Vocabulary* bias list into whisper.cpp's initial-prompt
/// mechanism. `None` if there's nothing to bias — the plumbing Phase 5 will
/// populate, unused until then.
fn build_initial_prompt(vocabulary: &[String]) -> Option<String> {
    if vocabulary.is_empty() {
        None
    } else {
        Some(vocabulary.join(", "))
    }
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
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| TranscribeError::ModelLoadFailed(e.to_string()))?;
        Ok(Transcriber { ctx })
    }

    /// Runs Whisper over `samples` (mono, `WHISPER_SAMPLE_RATE`, already
    /// validated by `is_too_short` at the caller) and returns the *Raw
    /// Transcript* text. An empty/whitespace-only result means near-silent
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

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        if let Some(lang) = resolve_language(config, samples) {
            params.set_language(Some(lang));
        }
        if let Some(prompt) = build_initial_prompt(&config.vocabulary) {
            params.set_initial_prompt(&prompt);
        }

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

        Ok(text.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            vocabulary: vec![],
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
            vocabulary: vec![],
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
            vocabulary: vec![],
        };
        assert_eq!(
            resolve_language(&config, &samples_of_duration_ms(5_000)),
            None
        );
    }

    #[test]
    fn empty_vocabulary_produces_no_prompt() {
        assert_eq!(build_initial_prompt(&[]), None);
    }

    #[test]
    fn vocabulary_joins_into_prompt() {
        assert_eq!(
            build_initial_prompt(&["VoiceDrop".to_string(), "whisper.cpp".to_string()]),
            Some("VoiceDrop, whisper.cpp".to_string())
        );
    }

    #[test]
    fn missing_model_file_is_reported() {
        let result = Transcriber::load(Path::new("/nonexistent/model.bin"));
        assert!(matches!(result, Err(TranscribeError::ModelNotFound(_))));
    }
}
