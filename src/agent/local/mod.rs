//! Local AI Inference Backends
//!
//! Backends that do not require a hosted API account.
//!
//! # The built-in engine
//!
//! **IronWorks is ironmon's only built-in inference engine** — the engine ironmon ships
//! against, and the default everywhere. See [`ironworks`].
//!
//! Everything else here is an **external provider**: software you install, start, or
//! sign in to separately. They are fully supported, but IronMonitor does not embed them.
//! [`crate::agent::backend::BackendType::is_builtin_engine`] draws the line in code.
//!
//! # External providers
//!
//! Self-hosted servers:
//!
//! - **Ollama**: popular local LLM server with easy model management
//! - **vLLM**: high-performance inference server with OpenAI-compatible API
//! - **TensorRT-LLM**: NVIDIA's optimized inference engine
//! - **LM Studio**: user-friendly local model server
//! - **llama.cpp**: GGUF models, driven through the `llama-cli` executable
//!
//! Command-line tools ([`cli`]), driven as subprocesses:
//!
//! - **`ollama`**, **`claude`**, **`codex`**, **`gemini`**
//!
//! # Local process is not local inference
//!
//! A CLI tool runs on your machine, but that says nothing about where inference
//! happens. Only `ollama` runs the model locally; `claude`, `codex` and `gemini`
//! relay the prompt — including any hardware telemetry in it — to their vendor's API.
//!
//! [`crate::agent::backend::BackendType::runs_on_host`] is the predicate to check
//! when the question is "does this leave the machine", and it is what orders
//! [`crate::agent::backend::BackendDiscovery::recommended`].
//!
//! # Feature Flags
//!
//! IronWorks, Ollama and the CLI providers are always compiled; the rest are opt-in.
//!
//! - `local-llamacpp`: Enable the llama.cpp subprocess client
//! - `local-vllm`: Enable vLLM client (HTTP-based)
//! - `local-tensorrt`: Enable TensorRT-LLM support
//!
//! # Example - Ollama
//!
//! ```no_run
//! use ironmonlib::agent::local::{OllamaClient, LocalInferenceClient, InferenceRequest};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = OllamaClient::new("http://localhost:11434")?;
//!
//! // Check available models
//! let models = client.list_models().await?;
//! println!("Available models: {:?}", models);
//!
//! // Run inference
//! let request = InferenceRequest {
//!     model: "llama3".to_string(),
//!     prompt: "What is the GPU temperature?".to_string(),
//!     system: None,
//!     max_tokens: Some(256),
//!     temperature: Some(0.7),
//!     ..Default::default()
//! };
//! let response = client.generate(request).await?;
//! println!("Response: {}", response.text);
//! # Ok(())
//! # }
//! ```

pub mod cli;
pub mod ironworks;
pub mod ollama;

#[cfg(feature = "local-llamacpp")]
pub mod llamacpp;

#[cfg(feature = "local-vllm")]
pub mod vllm;

#[cfg(feature = "local-tensorrt")]
pub mod tensorrt;

pub use cli::{CliClient, CliProvider};
pub use ironworks::IronWorksClient;
pub use ollama::OllamaClient;

#[cfg(feature = "local-llamacpp")]
pub use llamacpp::LlamaCppClient;

#[cfg(feature = "local-vllm")]
pub use vllm::VllmClient;

#[cfg(feature = "local-tensorrt")]
pub use tensorrt::TensorRtClient;

use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Common inference request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    /// Model identifier
    pub model: String,

    /// Prompt/query text
    pub prompt: String,

    /// System prompt (optional)
    pub system: Option<String>,

    /// Maximum tokens to generate
    pub max_tokens: Option<usize>,

    /// Temperature (0.0-1.0)
    pub temperature: Option<f32>,

    /// Top-p sampling
    pub top_p: Option<f32>,

    /// Stop sequences
    pub stop: Option<Vec<String>>,

    /// Enable streaming
    pub stream: bool,
}

impl Default for InferenceRequest {
    fn default() -> Self {
        Self {
            model: String::new(),
            prompt: String::new(),
            system: None,
            max_tokens: Some(256),
            temperature: Some(0.3),
            top_p: Some(0.9),
            stop: None,
            stream: false,
        }
    }
}

/// Inference response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    /// Generated text
    pub text: String,

    /// Model used
    pub model: String,

    /// Tokens generated
    pub tokens_generated: Option<usize>,

    /// Inference duration in milliseconds
    pub duration_ms: u64,

    /// Whether response was truncated
    pub truncated: bool,
}

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier
    pub name: String,

    /// Model size in bytes
    pub size: Option<u64>,

    /// Model family (e.g., "llama", "phi", "mistral")
    pub family: Option<String>,

    /// Parameter count
    pub parameter_count: Option<String>,

    /// Quantization level (e.g., "Q4_K_M", "Q8_0")
    pub quantization: Option<String>,
}

/// Common trait for local inference clients
#[async_trait]
pub trait LocalInferenceClient: Send + Sync {
    /// Get client name
    fn name(&self) -> &str;

    /// Check if server is available
    async fn is_available(&self) -> bool;

    /// List available models
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;

    /// Generate text from prompt
    async fn generate(&self, request: InferenceRequest) -> Result<InferenceResponse>;

    /// Get model info
    async fn model_info(&self, model_name: &str) -> Result<ModelInfo>;
}
