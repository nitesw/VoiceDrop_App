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
 * mono, and (until Phase 2/3 exist) immediately marks processing as
 * succeeded, reaching Done. Returns VOICEDROP_ERR_NOT_RECORDING if no
 * recording was in progress, or VOICEDROP_ERR_DEVICE_DISCONNECTED if the
 * input device failed mid-recording (session reaches Error in that case). */
int32_t voicedrop_engine_stop_recording(VoiceDropEngine *engine);

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
