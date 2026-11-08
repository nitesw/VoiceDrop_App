#ifndef VOICEDROP_CORE_H
#define VOICEDROP_CORE_H

#include <stdint.h>

/*
 * Hand-written to match core/src/lib.rs, core/src/engine.rs and
 * core/src/models.rs exactly. Keep the function signatures below as the
 * source of truth for what Swift depends on.
 */

/* Returns an owned, NUL-terminated string. Caller must pass the result to
 * voicedrop_core_free_string exactly once. */
char *voicedrop_core_ping(void);

/* Frees a string previously returned by voicedrop_core_ping or
 * voicedrop_engine_last_transcript. Safe to call with NULL. */
void voicedrop_core_free_string(char *s);

/* Opaque handle for the Dictation Session engine (state machine + audio
 * capture + Whisper). Accessed only through the voicedrop_engine_*
 * functions below. */
typedef struct VoiceDropEngine VoiceDropEngine;

/* Status codes returned by voicedrop_engine_* calls below. Keep in sync
 * with StatusCode in core/src/engine.rs. Gaps (5, 10-12) are codes that
 * belonged to removed features (see docs/adr/0009). */
enum {
    VOICEDROP_OK = 0,
    VOICEDROP_ERR_ILLEGAL_STATE = 1,
    VOICEDROP_ERR_NO_INPUT_DEVICE = 2,
    VOICEDROP_ERR_AUDIO_OTHER = 3,
    VOICEDROP_ERR_NOT_RECORDING = 4,
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
 * mono and runs it through whisper.cpp, reaching Done (retrieve the text via
 * voicedrop_engine_last_transcript). Buffers under transcribe::MIN_SPEECH_MS
 * (or that transcribe to empty text) short-circuit to VOICEDROP_NO_SPEECH /
 * VOICEDROP_STATE_NO_SPEECH. Returns VOICEDROP_ERR_NOT_RECORDING if no
 * recording was in progress, VOICEDROP_ERR_DEVICE_DISCONNECTED if the input
 * device failed mid-recording, or VOICEDROP_ERR_TRANSCRIPTION_FAILED if the
 * model couldn't be loaded or whisper.cpp itself failed (the session
 * reaches Error in all failure cases). Blocks while Whisper runs — call it
 * off the main thread. */
int32_t voicedrop_engine_stop_recording(VoiceDropEngine *engine);

/* Overrides the whisper.cpp model file path (defaults to the path the
 * voicedrop_model_* functions download to). `path` must be a
 * NUL-terminated UTF-8 string. */
int32_t voicedrop_engine_set_model_path(VoiceDropEngine *engine, const char *path);

/* Sets the language Whisper should transcribe in. Pass NULL for
 * auto-detect; otherwise an ISO 639-1 code such as "en", "uk", "fr". */
int32_t voicedrop_engine_set_language(VoiceDropEngine *engine, const char *language);

/* Sets the language used when auto-detect is selected but the clip is
 * shorter than transcribe::AUTO_DETECT_MIN_MS (auto-detect is unreliable on
 * short clips). Pass NULL to clear it. */
int32_t voicedrop_engine_set_fallback_language(VoiceDropEngine *engine, const char *language);

/* Returns the last session's transcript as an owned, NUL-terminated string,
 * or NULL if no session has produced one yet. Caller must free the result
 * with voicedrop_core_free_string. */
char *voicedrop_engine_last_transcript(const VoiceDropEngine *engine);

/* Transitions a terminal state (Done/Discarded/Error/NoSpeech) back to
 * Idle, ready for the next Dictation Session. */
int32_t voicedrop_engine_reset(VoiceDropEngine *engine);

/* Drops the cached Whisper model, freeing its memory. Next use lazily
 * reloads it from disk — call when the app is disabled, not during a
 * normal session. */
void voicedrop_engine_unload_models(VoiceDropEngine *engine);

/* Returns the engine's current VOICEDROP_STATE_* value, or -1 if `engine`
 * is NULL. */
int32_t voicedrop_engine_state(const VoiceDropEngine *engine);

/* Current input level (0.0-1.0) for the Dictation HUD's live waveform.
 * Meant to be polled on a UI timer while voicedrop_engine_state reports
 * VOICEDROP_STATE_RECORDING; returns 0.0 at all other times, and if
 * `engine` is NULL. */
float voicedrop_engine_current_input_level(const VoiceDropEngine *engine);

/* --- Whisper model download (core/src/models.rs) ------------------------
 * Global, not tied to a VoiceDropEngine handle. */

enum {
    VOICEDROP_MODEL_OK = 0,
    VOICEDROP_MODEL_ERR_NETWORK_FAILED = 2,
    VOICEDROP_MODEL_ERR_IO_FAILED = 3,
};

/* Returns 1 if the Whisper model is already downloaded, 0 if not. */
int32_t voicedrop_model_is_downloaded(void);

/* Downloads the Whisper model, blocking until it completes or fails — call
 * it off the main thread. `on_progress`, if non-NULL, is called
 * periodically with (bytes_downloaded, total_bytes_or_0_if_unknown,
 * user_data). Returns a VOICEDROP_MODEL_* status. */
int32_t voicedrop_model_download(
    void (*on_progress)(uint64_t, uint64_t, void *),
    void *user_data);

#endif /* VOICEDROP_CORE_H */
