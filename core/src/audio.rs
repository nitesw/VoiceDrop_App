//! Audio capture for a single Dictation Session. Captures from the default
//! input device via `cpal`, then downmixes to mono and resamples to the
//! 16 kHz whisper.cpp expects — done here so Phase 2 never has to reformat.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Sample rate whisper.cpp/whisper-rs expects.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug)]
pub enum AudioError {
    NoInputDevice,
    UnsupportedConfig(String),
    StreamBuildFailed(String),
    StreamPlayFailed(String),
    /// The input device reported an error (e.g. disconnected) during capture.
    DeviceDisconnected,
}

struct SharedState {
    samples: Mutex<Vec<f32>>,
    device_error: AtomicBool,
}

/// An in-progress capture. Dropping this (or calling `stop`) tears down the
/// cpal stream.
pub struct AudioCapture {
    stream: cpal::Stream,
    shared: Arc<SharedState>,
    input_sample_rate: u32,
    input_channels: u16,
}

impl AudioCapture {
    /// Opens the default input device and starts streaming immediately.
    pub fn start() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(AudioError::NoInputDevice)?;
        let config = device
            .default_input_config()
            .map_err(|e| AudioError::UnsupportedConfig(e.to_string()))?;

        let sample_format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();
        let input_sample_rate = stream_config.sample_rate.0;
        let input_channels = stream_config.channels;

        let shared = Arc::new(SharedState {
            samples: Mutex::new(Vec::new()),
            device_error: AtomicBool::new(false),
        });

        let err_shared = Arc::clone(&shared);
        let err_fn = move |err: cpal::StreamError| {
            eprintln!("[voicedrop-core] input stream error: {err}");
            err_shared.device_error.store(true, Ordering::SeqCst);
        };

        // NOTE: the data callback below runs on a realtime audio thread. It
        // must do the minimum possible: append samples and return. No FFI
        // calls into Swift, no logging, no allocation beyond the Vec's own
        // amortized growth. Everything else (resampling, notifying Swift of
        // state changes) happens elsewhere, off this thread.
        let data_shared = Arc::clone(&shared);
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    if let Ok(mut buf) = data_shared.samples.try_lock() {
                        buf.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    if let Ok(mut buf) = data_shared.samples.try_lock() {
                        buf.extend(data.iter().map(|&s| s as f32 / i16::MAX as f32));
                    }
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    if let Ok(mut buf) = data_shared.samples.try_lock() {
                        buf.extend(
                            data.iter()
                                .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                        );
                    }
                },
                err_fn,
                None,
            ),
            other => {
                return Err(AudioError::UnsupportedConfig(format!(
                    "unsupported sample format: {other:?}"
                )))
            }
        }
        .map_err(|e| AudioError::StreamBuildFailed(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::StreamPlayFailed(e.to_string()))?;

        Ok(AudioCapture {
            stream,
            shared,
            input_sample_rate,
            input_channels,
        })
    }

    /// Root-mean-square level of the most recently captured samples,
    /// roughly normalized to 0.0-1.0 for a live waveform display (Phase 4's
    /// Dictation HUD). Cheap `try_lock` + read, safe to poll frequently
    /// from a UI timer without contending with the realtime audio thread —
    /// if the lock is briefly held (mid-callback), this just returns the
    /// previous read's staleness for one tick rather than blocking.
    pub fn current_level(&self) -> f32 {
        const WINDOW: usize = 2048;
        let Ok(buf) = self.shared.samples.try_lock() else {
            return 0.0;
        };
        let window = if buf.len() > WINDOW {
            &buf[buf.len() - WINDOW..]
        } else {
            &buf[..]
        };
        if window.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = window.iter().map(|s| s * s).sum();
        let rms = (sum_sq / window.len() as f32).sqrt();
        // Empirical scale + perceptual (square-root) curve: normal speech
        // RMS sits well under 1.0 (full scale is clipping), and a linear
        // mapping compresses that whole range into visually tiny meter
        // movement. The sqrt curve is the standard level-meter trick for
        // this — it stretches the quiet-to-moderate range (most speech)
        // and compresses the already-loud range, so talking normally
        // visibly swings the meter instead of just nudging it.
        (rms * 6.0).clamp(0.0, 1.0).sqrt()
    }

    /// Stops capture and returns mono 16 kHz f32 samples, ready for Phase 2.
    pub fn stop(self) -> Result<Vec<f32>, AudioError> {
        // Dropping the stream stops it; do that before reading the final
        // buffer so nothing is still being appended underneath us.
        drop(self.stream);

        if self.shared.device_error.load(Ordering::SeqCst) {
            return Err(AudioError::DeviceDisconnected);
        }

        let captured = self
            .shared
            .samples
            .lock()
            .expect("audio buffer mutex poisoned")
            .clone();

        let mono = downmix_to_mono(&captured, self.input_channels);
        Ok(resample(&mono, self.input_sample_rate, WHISPER_SAMPLE_RATE))
    }
}

fn downmix_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let channels = channels as usize;
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Band-limited resampler (windowed-sinc interpolation). The mic usually
/// records at 44.1/48 kHz; Whisper wants 16 kHz. Plain linear interpolation
/// (what Phase 1 used) folds everything above 8 kHz back into the audible
/// band as noise (aliasing), which costs recognition accuracy. This low-pass
/// filters at the output's Nyquist frequency while interpolating, so only
/// the band Whisper can actually use survives.
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    // Cutoff relative to the input Nyquist frequency, a little below the
    // output Nyquist so the filter's transition band stays out of aliasing
    // range. 1.0 when upsampling (nothing to remove).
    let cutoff = 0.95 * (1.0 / ratio).min(1.0);
    // Kernel half-width in input samples: ~16 sinc zero-crossings per side.
    let half_width = 16.0 / cutoff;
    let out_len = ((samples.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let center = i as f64 * ratio;
        let first = (center - half_width).ceil().max(0.0) as usize;
        let last = ((center + half_width).floor() as usize).min(samples.len() - 1);
        let mut acc = 0.0;
        let mut weight_sum = 0.0;
        for (j, &sample) in samples.iter().enumerate().take(last + 1).skip(first) {
            let t = j as f64 - center;
            let weight = sinc(cutoff * t) * hann(t / half_width);
            acc += sample as f64 * weight;
            weight_sum += weight;
        }
        // Normalizing by the summed weights keeps unity gain everywhere,
        // including near the buffer edges where the kernel is truncated.
        out.push(if weight_sum.abs() > f64::EPSILON {
            (acc / weight_sum) as f32
        } else {
            0.0
        });
    }
    out
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// Hann window over [-1, 1], zero outside.
fn hann(x: f64) -> f64 {
    if x.abs() >= 1.0 {
        0.0
    } else {
        0.5 * (1.0 + (std::f64::consts::PI * x).cos())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_stereo_averages_channels() {
        // L, R, L, R
        let stereo = vec![1.0, 0.0, 0.5, 0.5];
        assert_eq!(downmix_to_mono(&stereo, 2), vec![0.5, 0.5]);
    }

    #[test]
    fn downmix_mono_is_passthrough() {
        let mono = vec![0.1, 0.2, 0.3];
        assert_eq!(downmix_to_mono(&mono, 1), mono);
    }

    #[test]
    fn resample_same_rate_is_passthrough() {
        let samples = vec![0.1, 0.2, 0.3];
        assert_eq!(resample(&samples, 16_000, 16_000), samples);
    }

    #[test]
    fn resample_downsamples_to_expected_length() {
        // 48kHz -> 16kHz is a 3:1 ratio.
        let samples = vec![0.0; 4800];
        let out = resample(&samples, 48_000, 16_000);
        assert_eq!(out.len(), 1600);
    }

    fn sine(freq: f64, rate: u32, seconds: f64) -> Vec<f32> {
        let n = (rate as f64 * seconds) as usize;
        (0..n)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin() as f32)
            .collect()
    }

    /// RMS over the middle of the buffer, away from edge effects.
    fn middle_rms(samples: &[f32]) -> f32 {
        let mid = &samples[samples.len() / 4..samples.len() * 3 / 4];
        (mid.iter().map(|s| s * s).sum::<f32>() / mid.len() as f32).sqrt()
    }

    #[test]
    fn resample_keeps_dc_level() {
        let out = resample(&vec![0.5; 4800], 48_000, 16_000);
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-3));
    }

    #[test]
    fn resample_keeps_speech_band_tone() {
        // 1 kHz is well inside the 0-8 kHz band 16 kHz audio can carry.
        let out = resample(&sine(1_000.0, 48_000, 0.5), 48_000, 16_000);
        let rms = middle_rms(&out);
        assert!(
            (rms - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.02,
            "rms {rms}"
        );
    }

    #[test]
    fn resample_removes_tone_above_output_nyquist() {
        // 12 kHz can't exist at 16 kHz. Linear interpolation would fold it
        // down to an audible 4 kHz alias; a proper resampler removes it.
        let out = resample(&sine(12_000.0, 48_000, 0.5), 48_000, 16_000);
        let rms = middle_rms(&out);
        assert!(rms < 0.01, "aliased energy left: rms {rms}");
    }
}
