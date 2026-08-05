#ifndef VOICEDROP_CORE_H
#define VOICEDROP_CORE_H

#include <stdint.h>

/*
 * Hand-written to match core/src/lib.rs and core/src/engine.rs exactly. If
 * cbindgen is introduced later to generate this automatically, keep the
 * function signatures below as the source of truth for what Swift currently
 * depends on.
 */

/* Returns an owned, NUL-terminated string. Caller must pass the result to
 * voicedrop_core_free_string exactly once. */
char *voicedrop_core_ping(void);

/* Frees a string previously returned by voicedrop_core_ping. Safe to call
 * with NULL. */
void voicedrop_core_free_string(char *s);

/* Opaque handle for the Dictation Session engine (state machine + audio
 * capture). Only ever accessed through the voicedrop_engine_* functions
 * below, from a single thread (the Swift main thread). */
typedef struct VoiceDropEngine VoiceDropEngine;

/* Status codes returned by voicedrop_engine_* calls below. Keep in sync
 * with StatusCode in core/src/engine.rs. */
enum {
    VOICEDROP_OK = 0,
    VOICEDROP_ERR_ILLEGAL_STATE = 1,
    VOICEDROP_ERR_NO_INPUT_DEVICE = 2,
    VOICEDROP_ERR_AUDIO_OTHER = 3,
    VOICEDROP_ERR_NOT_RECORDING = 4,
    VOICEDROP_ERR_WAV_WRITE_FAILED = 5,
    VOICEDROP_ERR_DEVICE_DISCONNECTED = 6,
    VOICEDROP_ERR_INVALID_PATH = 7,
    /* Recorded audio was silence-only or too brief to transcribe. */
    VOICEDROP_NO_SPEECH = 8,
    /* Model failed to load, or whisper.cpp failed mid-transcription. */
    VOICEDROP_ERR_TRANSCRIPTION_FAILED = 9,
};

/* Dictation Session states returned by voicedrop_engine_state. Keep in sync
 * with SessionState in core/src/session.rs. */
enum {
    VOICEDROP_STATE_IDLE = 0,
    VOICEDROP_STATE_RECORDING = 1,
    VOICEDROP_STATE_PROCESSING = 2,
    VOICEDROP_STATE_DONE = 3,
    VOICEDROP_STATE_DISCARDED = 4,
    VOICEDROP_STATE_ERROR = 5,
    VOICEDROP_STATE_NO_SPEECH = 6,
};

/* Allocates a new engine, starting in the Idle state. Must be freed exactly
 * once via voicedrop_engine_free. */
VoiceDropEngine *voicedrop_engine_new(void);

/* Frees an engine previously returned by voicedrop_engine_new. Safe to call
 * with NULL. Any in-progress recording is torn down. */
void voicedrop_engine_free(VoiceDropEngine *engine);

/* Transitions Idle -> Recording and starts capturing from the default input
 * device. Returns VOICEDROP_OK, or an error code (e.g.
 * VOICEDROP_ERR_NO_INPUT_DEVICE) if recording could not start — in which
 * case the engine remains Idle. */
int32_t voicedrop_engine_start_recording(VoiceDropEngine *engine);

/* Transitions Recording -> Processing, stops capture, resamples to 16kHz
 * mono, then runs it through whisper.cpp to produce a Raw Transcript
 * (retrieve via voicedrop_engine_last_transcript), reaching Done. Buffers
 * under transcribe::MIN_SPEECH_MS (or that transcribe to empty text)
 * short-circuit to VOICEDROP_NO_SPEECH / VOICEDROP_STATE_NO_SPEECH without
 * running Whisper. Returns VOICEDROP_ERR_NOT_RECORDING if no recording was
 * in progress, VOICEDROP_ERR_DEVICE_DISCONNECTED if the input device failed
 * mid-recording, or VOICEDROP_ERR_TRANSCRIPTION_FAILED if the model
 * couldn't be loaded or whisper.cpp itself failed (session reaches Error in
 * both cases). */
int32_t voicedrop_engine_stop_recording(VoiceDropEngine *engine);

/* Overrides the whisper.cpp model file path (defaults to a path under
 * Application Support — see docs/adr/0002-whisper-model-bundling.md).
 * `path` must be a NUL-terminated UTF-8 string. */
int32_t voicedrop_engine_set_model_path(VoiceDropEngine *engine, const char *path);

/* Sets the language Whisper should transcribe in. Pass NULL for
 * auto-detect; otherwise an ISO 639-1 code such as "en", "fr", "de". */
int32_t voicedrop_engine_set_language(VoiceDropEngine *engine, const char *language);

/* Sets the language used when auto-detect is selected but the clip is
 * shorter than transcribe::AUTO_DETECT_MIN_MS (auto-detect is unreliable on
 * short clips). Pass NULL to clear it. */
int32_t voicedrop_engine_set_fallback_language(VoiceDropEngine *engine, const char *language);

/* Sets the Custom Vocabulary bias list as a comma-separated string of
 * words/phrases, fed to whisper.cpp as an initial prompt. Pass NULL or ""
 * to clear it. Foundation-only: Phase 5 owns the editable list UI. */
int32_t voicedrop_engine_set_vocabulary(VoiceDropEngine *engine, const char *words);

/* Returns the most recent Raw Transcript as an owned, NUL-terminated
 * string, or NULL if no session has produced one yet. Caller must free the
 * result with voicedrop_core_free_string. */
char *voicedrop_engine_last_transcript(const VoiceDropEngine *engine);

/* Writes the most recently completed session's captured audio to a 16-bit
 * PCM mono WAV file at `path` for manual listening verification. `path`
 * must be a NUL-terminated UTF-8 string. Returns VOICEDROP_ERR_NOT_RECORDING
 * if no session has completed yet. */
int32_t voicedrop_engine_write_verification_wav(VoiceDropEngine *engine, const char *path);

/* Transitions a terminal state (Done/Discarded/Error) back to Idle, ready
 * for the next Dictation Session. */
int32_t voicedrop_engine_reset(VoiceDropEngine *engine);

/* Returns the engine's current VOICEDROP_STATE_* value, or -1 if `engine`
 * is NULL. */
int32_t voicedrop_engine_state(const VoiceDropEngine *engine);

#endif /* VOICEDROP_CORE_H */
