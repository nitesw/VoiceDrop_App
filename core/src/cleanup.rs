//! The Cleanup Pass: turns a *Raw Transcript* into a *Cleaned Transcript* by
//! stripping disfluencies, adding punctuation, and correcting grammar,
//! without changing meaning. See CONTEXT.md and
//! `docs/todos/0004-phase3-cleanup-pass.md`.
//!
//! Provider selection is a runtime config value, not compile-time — see
//! [ADR-0002](../../docs/adr/0002-local-first-with-byo-key-cloud-fallback.md)
//! and its amendments
//! [ADR-0005](../../docs/adr/0005-cleanup-pass-optional-and-free-form-endpoint.md)
//! (optional Cleanup Pass, free-form cloud endpoint) and
//! [ADR-0008](../../docs/adr/0008-local-cleanup-in-process-again.md) (why
//! `Local` is a self-contained in-process model again, not Ollama-backed —
//! Ollama/other local runners are still reachable, just via `Cloud`'s
//! free-form endpoint instead of a dedicated provider).
//!
//! Apple's on-device Foundation Models framework (macOS-only alternative
//! provider from ADR-0002) is deliberately **not** implemented here: it's a
//! Swift/ObjC-only API with no C ABI, so it can't be called from this Rust
//! core. Per [ADR-0001](../../docs/adr/0001-rust-core-with-native-ui-shells.md)'s
//! precedent of thin per-OS adapters for OS-specific concerns, that provider
//! lives entirely in the Swift shell (Phase 4), which calls Foundation
//! Models directly and never routes through `CleanupProvider`. It reuses
//! `system_prompt` below (exposed over FFI as
//! `voicedrop_cleanup_prompt_for_strength`) so the wording is defined once
//! and shared, not duplicated in Swift.

use std::path::{Path, PathBuf};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use std::num::NonZeroU32;

/// Hard cap on generated tokens so a misbehaving model can't hang a
/// Dictation Session forever. Cleaned transcripts are short (a few
/// sentences of dictated speech), so this is generous, not tight.
const MAX_NEW_TOKENS: usize = 512;

#[derive(Debug)]
pub enum CleanupError {
    Timeout,
    InferenceFailed(String),
    NetworkFailed(String),
    /// Missing/invalid API key, malformed base URL, or similar setup error —
    /// distinguished from `NetworkFailed` (request went out but failed) and
    /// `InferenceFailed` (model/request was fine, generation failed).
    InvalidConfig(String),
    ModelNotFound(PathBuf),
}

/// The three *Cleanup Strength* levels from CONTEXT.md. Global preference,
/// not per-session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupStrength {
    /// Disfluency + grammar only, preserves original wording/structure.
    VerbatimClean,
    /// Also merges fragments, tightens wordy phrasing.
    LightEdit,
    /// Heavier restructuring.
    FormalRewrite,
}

/// The system prompt for a given strength level. Shared across every
/// transforming provider (local, cloud, and — via the FFI export below —
/// Apple Foundation Models in the Swift shell) so they can't silently drift
/// into different cleanup behavior.
pub fn system_prompt(strength: CleanupStrength) -> &'static str {
    match strength {
        CleanupStrength::VerbatimClean => {
            "You clean up dictated speech transcripts. Remove filler words and \
             disfluencies (\"uh\", \"um\", false starts), add punctuation, and fix \
             grammar. Preserve the original wording, structure, and sentence \
             boundaries exactly — do not merge, split, reorder, or rephrase \
             sentences, and do not change the meaning. Output only the cleaned \
             transcript, nothing else."
        }
        CleanupStrength::LightEdit => {
            "You clean up dictated speech transcripts. Remove filler words and \
             disfluencies, add punctuation, and fix grammar. You may merge \
             sentence fragments and tighten wordy phrasing, but do not change \
             the meaning or add information that wasn't said. Output only the \
             cleaned transcript, nothing else."
        }
        CleanupStrength::FormalRewrite => {
            "You clean up dictated speech transcripts. Remove filler words and \
             disfluencies, add punctuation, fix grammar, and restructure the \
             text into clear, well-organized prose suitable for a formal \
             document. Do not change the meaning or add information that \
             wasn't said. Output only the cleaned transcript, nothing else."
        }
    }
}

/// Raw transcript + strength + language in, cleaned transcript out. One
/// method, implemented per provider so switching providers is a runtime
/// config change (see the "Provider interface" todo).
pub trait CleanupProvider {
    fn cleanup(
        &self,
        raw_transcript: &str,
        strength: CleanupStrength,
        language: Option<&str>,
    ) -> Result<String, CleanupError>;
}

/// Passes the *Raw Transcript* through unchanged. Selecting this provider
/// means no cleanup model is ever downloaded and no cloud key is ever
/// required — per ADR-0005, the user can opt out of the Cleanup Pass
/// entirely, and per the same ADR this is the default.
pub struct NoneProvider;

impl CleanupProvider for NoneProvider {
    fn cleanup(
        &self,
        raw_transcript: &str,
        _strength: CleanupStrength,
        _language: Option<&str>,
    ) -> Result<String, CleanupError> {
        Ok(raw_transcript.to_string())
    }
}

fn user_prompt(raw_transcript: &str, language: Option<&str>) -> String {
    match language {
        Some(lang) => format!("Language: {lang}\nTranscript: {raw_transcript}"),
        None => format!("Transcript: {raw_transcript}"),
    }
}

/// Local Cleanup Pass via `llama.cpp` (through `llama-cpp-2`) — a
/// self-contained in-process model, no external app required, downloaded
/// and managed by VoiceDrop the same way the Whisper STT model is (see
/// [ADR-0004](../../docs/adr/0004-whisper-model-download-on-first-run.md)).
/// Loads a GGUF model once and reuses it across sessions — loading is
/// expensive.
///
/// `whisper-rs` and `llama-cpp-2` each vendor their own copy of ggml;
/// statically linking both caused ~600 duplicate-symbol errors at the
/// Swift app link step. Fixed via `llama-cpp-sys-2`'s `dynamic-link`
/// feature — see
/// [ADR-0006](../../docs/adr/0006-shared-ggml-symbol-collision-and-model-catalog.md)
/// for the full story and [ADR-0008](../../docs/adr/0008-local-cleanup-in-process-again.md)
/// for why this approach is back after a brief detour through an
/// Ollama-backed local provider.
///
/// Model selection/benchmarking (`0004-phase3-cleanup-pass.md`, "Local
/// provider" todo) isn't finished: `scripts/download-cleanup-model.sh`
/// defaults to Qwen2.5-0.5B-Instruct as a fast starting point, but other
/// catalog candidates (see `models::CATALOG`) haven't been benchmarked
/// against it yet on real hardware.
pub struct LocalProvider {
    backend: LlamaBackend,
    model: LlamaModel,
}

impl LocalProvider {
    pub fn load(model_path: &Path) -> Result<Self, CleanupError> {
        if !model_path.is_file() {
            return Err(CleanupError::ModelNotFound(model_path.to_path_buf()));
        }
        let backend =
            LlamaBackend::init().map_err(|e| CleanupError::InferenceFailed(e.to_string()))?;
        let model = LlamaModel::load_from_file(&backend, model_path, &LlamaModelParams::default())
            .map_err(|e| CleanupError::InferenceFailed(e.to_string()))?;
        Ok(LocalProvider { backend, model })
    }

    fn build_prompt(
        &self,
        raw_transcript: &str,
        strength: CleanupStrength,
        language: Option<&str>,
    ) -> Result<String, CleanupError> {
        let template = self
            .model
            .chat_template(None)
            .or_else(|_| LlamaChatTemplate::new("chatml").map_err(Into::into))
            .map_err(|e: llama_cpp_2::ChatTemplateError| {
                CleanupError::InferenceFailed(e.to_string())
            })?;

        let messages = [
            LlamaChatMessage::new("system".to_string(), system_prompt(strength).to_string())
                .map_err(|e| CleanupError::InferenceFailed(e.to_string()))?,
            LlamaChatMessage::new("user".to_string(), user_prompt(raw_transcript, language))
                .map_err(|e| CleanupError::InferenceFailed(e.to_string()))?,
        ];

        self.model
            .apply_chat_template(&template, &messages, true)
            .map_err(|e| CleanupError::InferenceFailed(e.to_string()))
    }
}

impl CleanupProvider for LocalProvider {
    fn cleanup(
        &self,
        raw_transcript: &str,
        strength: CleanupStrength,
        language: Option<&str>,
    ) -> Result<String, CleanupError> {
        let prompt = self.build_prompt(raw_transcript, strength, language)?;

        let tokens = self
            .model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| CleanupError::InferenceFailed(e.to_string()))?;

        let n_ctx = NonZeroU32::new((tokens.len() + MAX_NEW_TOKENS) as u32)
            .unwrap_or(NonZeroU32::new(2048).expect("2048 is nonzero"));
        let ctx_params = LlamaContextParams::default().with_n_ctx(Some(n_ctx));
        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| CleanupError::InferenceFailed(e.to_string()))?;

        let mut batch = LlamaBatch::new(tokens.len().max(MAX_NEW_TOKENS), 1);
        batch
            .add_sequence(&tokens, 0, false)
            .map_err(|e| CleanupError::InferenceFailed(e.to_string()))?;
        ctx.decode(&mut batch)
            .map_err(|e| CleanupError::InferenceFailed(e.to_string()))?;

        let mut sampler = LlamaSampler::greedy();
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut pos = tokens.len() as i32;

        // `pos` also feeds `batch.add` below, so it isn't purely a loop
        // counter — clippy's suggested rewrite doesn't fit.
        #[allow(clippy::explicit_counter_loop)]
        for _ in 0..MAX_NEW_TOKENS {
            let next = sampler.sample(&ctx, batch.n_tokens() - 1);
            if self.model.is_eog_token(next) {
                break;
            }

            let piece = self
                .model
                .token_to_piece(next, &mut decoder, false, None)
                .map_err(|e| CleanupError::InferenceFailed(e.to_string()))?;
            output.push_str(&piece);

            sampler.accept(next);
            batch.clear();
            batch
                .add(next, pos, &[0], true)
                .map_err(|e| CleanupError::InferenceFailed(e.to_string()))?;
            ctx.decode(&mut batch)
                .map_err(|e| CleanupError::InferenceFailed(e.to_string()))?;
            pos += 1;
        }

        Ok(output.trim().to_string())
    }
}

#[derive(serde::Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: String,
}

#[derive(serde::Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
}

#[derive(serde::Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(serde::Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(serde::Deserialize)]
struct ChatResponseMessage {
    content: String,
}

/// Cloud Cleanup Pass: a user-supplied base URL + API key, assumed
/// OpenAI-compatible (`POST {base_url}/chat/completions`). Per ADR-0005,
/// this is a free-form endpoint, not a fixed vendor list — it covers hosted
/// providers, OpenRouter, and any self-hosted/local server that speaks the
/// same request/response shape: **Ollama, LM Studio, vLLM, or any other
/// local model runner all work here too** (point `base_url` at
/// `http://localhost:<port>/v1` with any non-empty placeholder API key if
/// the server doesn't check one). No VoiceDrop backend in the loop: the
/// request goes directly from this client to `base_url`.
pub struct CloudProvider {
    base_url: String,
    api_key: String,
    model: String,
}

impl CloudProvider {
    pub fn new(base_url: String, api_key: String, model: String) -> Result<Self, CleanupError> {
        if base_url.trim().is_empty() {
            return Err(CleanupError::InvalidConfig(
                "cloud cleanup base URL is empty".to_string(),
            ));
        }
        if api_key.trim().is_empty() {
            return Err(CleanupError::InvalidConfig(
                "cloud cleanup API key is empty".to_string(),
            ));
        }
        Ok(CloudProvider {
            base_url,
            api_key,
            model,
        })
    }

    /// Builds the OpenAI-compatible `/chat/completions` request body. Split
    /// out from `cleanup` so it's unit-testable without a network call.
    fn build_request(
        &self,
        raw_transcript: &str,
        strength: CleanupStrength,
        language: Option<&str>,
    ) -> ChatRequest<'_> {
        ChatRequest {
            model: &self.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: system_prompt(strength).to_string(),
                },
                ChatMessage {
                    role: "user",
                    content: user_prompt(raw_transcript, language),
                },
            ],
        }
    }

    fn endpoint_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

impl CleanupProvider for CloudProvider {
    fn cleanup(
        &self,
        raw_transcript: &str,
        strength: CleanupStrength,
        language: Option<&str>,
    ) -> Result<String, CleanupError> {
        let request = self.build_request(raw_transcript, strength, language);

        let response: ChatResponse = ureq::post(&self.endpoint_url())
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .send_json(&request)
            .map_err(|e| CleanupError::NetworkFailed(e.to_string()))?
            .body_mut()
            .read_json()
            .map_err(|e| {
                CleanupError::InferenceFailed(format!(
                    "response did not match the expected OpenAI-compatible shape: {e}"
                ))
            })?;

        let content = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| CleanupError::InferenceFailed("response had no choices".to_string()))?
            .message
            .content;

        Ok(content.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_provider_passes_transcript_through_unchanged() {
        let provider = NoneProvider;
        let result = provider
            .cleanup("uh, hello   there", CleanupStrength::LightEdit, None)
            .unwrap();
        assert_eq!(result, "uh, hello   there");
    }

    #[test]
    fn strength_levels_produce_distinct_prompts() {
        let verbatim = system_prompt(CleanupStrength::VerbatimClean);
        let light = system_prompt(CleanupStrength::LightEdit);
        let formal = system_prompt(CleanupStrength::FormalRewrite);
        assert_ne!(verbatim, light);
        assert_ne!(light, formal);
        assert_ne!(verbatim, formal);
    }

    #[test]
    fn verbatim_prompt_forbids_restructuring() {
        assert!(system_prompt(CleanupStrength::VerbatimClean).contains("do not merge"));
    }

    #[test]
    fn cloud_provider_rejects_empty_base_url() {
        let result = CloudProvider::new(
            String::new(),
            "sk-test".to_string(),
            "gpt-4o-mini".to_string(),
        );
        assert!(matches!(result, Err(CleanupError::InvalidConfig(_))));
    }

    #[test]
    fn cloud_provider_rejects_empty_api_key() {
        let result = CloudProvider::new(
            "https://api.openai.com/v1".to_string(),
            String::new(),
            "gpt-4o-mini".to_string(),
        );
        assert!(matches!(result, Err(CleanupError::InvalidConfig(_))));
    }

    #[test]
    fn cloud_provider_accepts_arbitrary_endpoint() {
        // Not just a named vendor — a local server address (Ollama, LM
        // Studio, vLLM, ...) works too, per ADR-0005.
        let result = CloudProvider::new(
            "http://localhost:11434/v1".to_string(),
            "unused".to_string(),
            "qwen2.5:0.5b".to_string(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn endpoint_url_strips_trailing_slash() {
        let provider = CloudProvider::new(
            "https://api.example.com/v1/".to_string(),
            "key".to_string(),
            "model".to_string(),
        )
        .unwrap();
        assert_eq!(
            provider.endpoint_url(),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn build_request_includes_strength_prompt_and_transcript() {
        let provider = CloudProvider::new(
            "https://api.example.com/v1".to_string(),
            "key".to_string(),
            "model".to_string(),
        )
        .unwrap();
        let request = provider.build_request("hello world", CleanupStrength::VerbatimClean, None);
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, "system");
        assert_eq!(
            request.messages[0].content,
            system_prompt(CleanupStrength::VerbatimClean)
        );
        assert_eq!(request.messages[1].role, "user");
        assert!(request.messages[1].content.contains("hello world"));
    }

    #[test]
    fn build_request_includes_language_when_given() {
        let provider = CloudProvider::new(
            "https://api.example.com/v1".to_string(),
            "key".to_string(),
            "model".to_string(),
        )
        .unwrap();
        let request = provider.build_request("bonjour", CleanupStrength::VerbatimClean, Some("fr"));
        assert!(request.messages[1].content.contains("Language: fr"));
    }

    #[test]
    fn missing_local_model_file_is_reported() {
        let result = LocalProvider::load(Path::new("/nonexistent/model.gguf"));
        assert!(matches!(result, Err(CleanupError::ModelNotFound(_))));
    }

    /// Manual verification only — needs a real GGUF model on disk (run
    /// `scripts/download-cleanup-model.sh` first) and takes a few seconds.
    /// Not run in CI: `cargo test --release -- --ignored local_provider_cleans_a_disfluent_transcript`.
    #[test]
    #[ignore]
    fn local_provider_cleans_a_disfluent_transcript() {
        let home = std::env::var("HOME").expect("HOME must be set");
        let model_path = PathBuf::from(home)
            .join("Library/Application Support/VoiceDrop/models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
        let provider = LocalProvider::load(&model_path).expect("model should load");

        let raw = "so uh i think we should, like, go to the store and, um, get some milk";
        let result = provider
            .cleanup(raw, CleanupStrength::VerbatimClean, None)
            .expect("cleanup should succeed");

        println!("Raw:     {raw}");
        println!("Cleaned: {result}");
        assert!(!result.is_empty());
    }

    /// Manual verification only — needs a real OpenAI-compatible server
    /// reachable at `http://localhost:11434/v1` (e.g. `ollama serve` with
    /// `ollama pull qwen2.5:0.5b`). Confirms the "bring your own via
    /// Ollama/any local runner" path documented on `CloudProvider` actually
    /// holds against a real self-hosted server, not just a named cloud
    /// vendor.
    /// Not run in CI: `cargo test -- --ignored cloud_provider_works_against_a_real_openai_compatible_server`.
    #[test]
    #[ignore]
    fn cloud_provider_works_against_a_real_openai_compatible_server() {
        let provider = CloudProvider::new(
            "http://localhost:11434/v1".to_string(),
            "unused".to_string(),
            "qwen2.5:0.5b".to_string(),
        )
        .unwrap();

        let raw = "so uh i think we should, like, go to the store and, um, get some milk";
        let result = provider
            .cleanup(raw, CleanupStrength::VerbatimClean, None)
            .expect("cleanup should succeed against a real local server");

        println!("Raw:     {raw}");
        println!("Cleaned: {result}");
        assert!(!result.is_empty());
    }
}
