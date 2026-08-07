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
    /* The Cleanup Pass model failed to load, or inference itself failed. */
    VOICEDROP_ERR_CLEANUP_FAILED = 10,
    /* The cloud Cleanup Pass provider's request failed (network/timeout). */
    VOICEDROP_ERR_CLEANUP_NETWORK_FAILED = 11,
    /* The selected Cleanup Pass provider is missing required config (e.g.
     * no cloud base URL/API key set). */
    VOICEDROP_ERR_CLEANUP_INVALID_CONFIG = 12,
};

/* Cleanup Pass provider selection — keep in sync with CleanupProviderKind
 * in core/src/engine.rs. VOICEDROP_CLEANUP_LOCAL is a self-contained
 * in-process model (see docs/adr/0008-local-cleanup-in-process-again.md) —
 * configure with voicedrop_engine_set_cleanup_local_model_path. To bring
 * your own model via Ollama or another local runner instead, use
 * VOICEDROP_CLEANUP_CLOUD pointed at that server's address.
 * VOICEDROP_CLEANUP_APPLE is a marker only: Rust can't call Apple's
 * Swift/ObjC-only Foundation Models framework, so the Swift shell must run
 * cleanup itself and report the result back via
 * voicedrop_engine_set_cleaned_transcript. */
enum {
    VOICEDROP_CLEANUP_NONE = 0,
    VOICEDROP_CLEANUP_LOCAL = 1,
    VOICEDROP_CLEANUP_APPLE = 2,
    VOICEDROP_CLEANUP_CLOUD = 3,
};

/* Cleanup Strength levels — keep in sync with CleanupStrength in
 * core/src/cleanup.rs. */
enum {
    VOICEDROP_STRENGTH_VERBATIM_CLEAN = 0,
    VOICEDROP_STRENGTH_LIGHT_EDIT = 1,
    VOICEDROP_STRENGTH_FORMAL_REWRITE = 2,
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

/* Returns the shared Cleanup Pass system prompt (a VOICEDROP_STRENGTH_*
 * value) as an owned NUL-terminated string, or NULL for an invalid
 * strength. Lets a Swift-side Apple Foundation Models provider reuse the
 * exact same wording as the local/cloud providers. Caller must free the
 * result with voicedrop_core_free_string. */
char *voicedrop_cleanup_prompt_for_strength(int32_t strength);

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
 * mono, runs it through whisper.cpp to produce a Raw Transcript (retrieve
 * via voicedrop_engine_last_raw_transcript), then runs the selected Cleanup
 * Pass provider (voicedrop_engine_set_cleanup_provider) to produce the
 * transcript ready for injection (voicedrop_engine_last_transcript),
 * reaching Done. Buffers under transcribe::MIN_SPEECH_MS (or that
 * transcribe to empty text) short-circuit to VOICEDROP_NO_SPEECH /
 * VOICEDROP_STATE_NO_SPEECH without running Whisper or the Cleanup Pass.
 * If the provider is VOICEDROP_CLEANUP_APPLE, this stops after STT —
 * voicedrop_engine_last_transcript returns NULL until the Swift shell calls
 * voicedrop_engine_set_cleaned_transcript. Returns
 * VOICEDROP_ERR_NOT_RECORDING if no recording was in progress,
 * VOICEDROP_ERR_DEVICE_DISCONNECTED if the input device failed
 * mid-recording, VOICEDROP_ERR_TRANSCRIPTION_FAILED if the STT model
 * couldn't be loaded or whisper.cpp itself failed, or
 * VOICEDROP_ERR_CLEANUP_FAILED / VOICEDROP_ERR_CLEANUP_NETWORK_FAILED /
 * VOICEDROP_ERR_CLEANUP_INVALID_CONFIG if the Cleanup Pass failed (session
 * reaches Error in all failure cases; failures never silently fall back to
 * the Raw Transcript). */
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

/* Returns the transcript ready for injection as an owned, NUL-terminated
 * string, or NULL if no session has produced one yet. This is the Cleaned
 * Transcript once a transforming Cleanup Pass provider has run, the Raw
 * Transcript unchanged for VOICEDROP_CLEANUP_NONE, or (for
 * VOICEDROP_CLEANUP_APPLE) whatever was last set via
 * voicedrop_engine_set_cleaned_transcript. Caller must free the result with
 * voicedrop_core_free_string. */
char *voicedrop_engine_last_transcript(const VoiceDropEngine *engine);

/* Returns the most recent Raw Transcript (pre-Cleanup-Pass) as an owned,
 * NUL-terminated string, or NULL if no session has produced one yet. Caller
 * must free the result with voicedrop_core_free_string. */
char *voicedrop_engine_last_raw_transcript(const VoiceDropEngine *engine);

/* Selects the Cleanup Pass provider at runtime (a VOICEDROP_CLEANUP_*
 * value) — a config change, not a code change. */
int32_t voicedrop_engine_set_cleanup_provider(VoiceDropEngine *engine, int32_t kind);

/* Sets the Cleanup Strength level (a VOICEDROP_STRENGTH_* value). */
int32_t voicedrop_engine_set_cleanup_strength(VoiceDropEngine *engine, int32_t strength);

/* Overrides the local Cleanup Pass GGUF model path (defaults to a path
 * under Application Support, downloaded on first use — see
 * docs/adr/0008-local-cleanup-in-process-again.md). `path` must be a
 * NUL-terminated UTF-8 string. */
int32_t voicedrop_engine_set_cleanup_local_model_path(VoiceDropEngine *engine, const char *path);

/* Configures the cloud Cleanup Pass provider: a free-form base URL (e.g.
 * "https://api.openai.com/v1", or "http://localhost:11434/v1" to bring
 * your own model via Ollama, or any other local runner's OpenAI-compatible
 * address), API key, and model name, assumed OpenAI-compatible. All three
 * are required NUL-terminated UTF-8 strings — for a local server that
 * doesn't check the key, pass any non-empty placeholder. */
int32_t voicedrop_engine_set_cleanup_cloud_config(
    VoiceDropEngine *engine, const char *base_url, const char *api_key, const char *model);

/* Sets custom words to always strip from every Raw Transcript, as a
 * comma-separated list — merged with a small built-in default list,
 * applied unconditionally regardless of Cleanup Pass provider (including
 * VOICEDROP_CLEANUP_NONE). Pass NULL or "" to clear custom words (built-in
 * defaults still apply). */
int32_t voicedrop_engine_set_blocklist(VoiceDropEngine *engine, const char *words);

/* Lets the Swift shell hand back a Cleaned Transcript it produced itself via
 * Apple's Foundation Models framework, when the Cleanup Pass provider is
 * VOICEDROP_CLEANUP_APPLE (Rust can't call that framework directly).
 * Returns VOICEDROP_ERR_ILLEGAL_STATE if the provider isn't Apple. `text`
 * must be a NUL-terminated UTF-8 string. */
int32_t voicedrop_engine_set_cleaned_transcript(VoiceDropEngine *engine, const char *text);

/* --- Model catalog (core/src/models.rs) ---------------------------------
 * Global, not tied to a VoiceDropEngine handle. VoiceDrop-managed
 * downloads: the Whisper STT model, and GGUF candidates for the
 * self-contained VOICEDROP_CLEANUP_LOCAL provider. Backing plumbing for a
 * Phase 5 Settings Window model picker. voicedrop_model_download blocks
 * for the whole transfer — call it off any UI-critical thread. */

enum {
    VOICEDROP_MODEL_OK = 0,
    VOICEDROP_MODEL_ERR_UNKNOWN_ID = 1,
    VOICEDROP_MODEL_ERR_NETWORK_FAILED = 2,
    VOICEDROP_MODEL_ERR_IO_FAILED = 3,
};

/* Model kind — keep in sync with ModelKind in core/src/models.rs. */
enum {
    VOICEDROP_MODEL_KIND_STT = 0,
    VOICEDROP_MODEL_KIND_CLEANUP = 1,
};

/* Number of entries in the model catalog. */
int32_t voicedrop_model_catalog_count(void);

/* Returns catalog entry `index`'s stable id, or NULL if out of range.
 * Caller must free with voicedrop_core_free_string. */
char *voicedrop_model_catalog_id(int32_t index);

/* Returns catalog entry `index`'s human-readable name, or NULL if out of
 * range. Caller must free with voicedrop_core_free_string. */
char *voicedrop_model_catalog_display_name(int32_t index);

/* Returns catalog entry `index`'s kind (a VOICEDROP_MODEL_KIND_* value), or
 * -1 if out of range. */
int32_t voicedrop_model_catalog_kind(int32_t index);

/* Returns catalog entry `index`'s approximate download size in bytes (a UI
 * label, not a guarantee), or 0 if out of range. */
uint64_t voicedrop_model_catalog_approx_size_bytes(int32_t index);

/* Returns 1 if `id`'s model file is already downloaded, 0 if not, -1 if
 * `id` is unknown/invalid. `id` must be a NUL-terminated UTF-8 string. */
int32_t voicedrop_model_is_downloaded(const char *id);

/* Returns the on-disk path `id`'s model would live at (downloaded or not),
 * or NULL if `id` is unknown. Feed the result into
 * voicedrop_engine_set_model_path / voicedrop_engine_set_cleanup_local_model_path
 * depending on the entry's kind. Caller must free with
 * voicedrop_core_free_string. */
char *voicedrop_model_path_for(const char *id);

/* Downloads `id`'s model file, blocking until it completes or fails.
 * `on_progress`, if non-NULL, is called periodically with
 * (bytes_downloaded, total_bytes_or_0_if_unknown, user_data). Returns a
 * VOICEDROP_MODEL_* status. */
int32_t voicedrop_model_download(
    const char *id,
    void (*on_progress)(uint64_t, uint64_t, void *),
    void *user_data);

/* Deletes `id`'s downloaded model file, if present (not an error if
 * already absent). Returns a VOICEDROP_MODEL_* status. */
int32_t voicedrop_model_delete(const char *id);

/* --- Ollama "bring your own" suggestions (core/src/models.rs) ----------
 * Names to suggest when the user picks "bring your own via Ollama" —
 * `ollama pull <name>`, then pass the name as `model` to
 * voicedrop_engine_set_cleanup_cloud_config with base_url
 * "http://localhost:11434/v1". Ollama itself owns pulling/storing these;
 * VoiceDrop only suggests names. See
 * docs/adr/0008-local-cleanup-in-process-again.md. */

/* Number of suggested Ollama Cleanup Pass models. */
int32_t voicedrop_ollama_model_count(void);

/* Returns suggestion `index`'s Ollama model name, or NULL if out of range.
 * Caller must free with voicedrop_core_free_string. */
char *voicedrop_ollama_model_name(int32_t index);

/* Returns suggestion `index`'s human-readable display name, or NULL if out
 * of range. Caller must free with voicedrop_core_free_string. */
char *voicedrop_ollama_model_display_name(int32_t index);

/* Returns suggestion `index`'s approximate download size in bytes (a UI
 * label — Ollama manages the actual download), or 0 if out of range. */
uint64_t voicedrop_ollama_model_approx_size_bytes(int32_t index);

/* Writes the most recently completed session's captured audio to a 16-bit
 * PCM mono WAV file at `path` for manual listening verification. `path`
 * must be a NUL-terminated UTF-8 string. Returns VOICEDROP_ERR_NOT_RECORDING
 * if no session has completed yet. */
int32_t voicedrop_engine_write_verification_wav(VoiceDropEngine *engine, const char *path);

/* Transitions a terminal state (Done/Discarded/Error) back to Idle, ready
 * for the next Dictation Session. */
int32_t voicedrop_engine_reset(VoiceDropEngine *engine);

/* Drops the cached Whisper transcriber and Cleanup Pass providers, freeing
 * whatever memory they hold (the Whisper model, and/or the local llama.cpp
 * GGUF model). Next use after this lazily reloads from disk, same as a
 * fresh launch — call when the app is disabled and should stop holding
 * onto loaded models, not during a normal session. */
void voicedrop_engine_unload_models(VoiceDropEngine *engine);

/* Returns the engine's current VOICEDROP_STATE_* value, or -1 if `engine`
 * is NULL. */
int32_t voicedrop_engine_state(const VoiceDropEngine *engine);

/* Current input level (0.0-1.0) for the Dictation HUD's live waveform.
 * Meant to be polled on a UI timer while voicedrop_engine_state reports
 * VOICEDROP_STATE_RECORDING; returns 0.0 at all other times, and if
 * `engine` is NULL. */
float voicedrop_engine_current_input_level(const VoiceDropEngine *engine);

#endif /* VOICEDROP_CORE_H */
