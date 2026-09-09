//! IronWorks — the default inference backend.
//!
//! [IronWorks](https://github.com/nervosys/ironworks) is a pure-Rust LLM inference
//! engine with CPU/CUDA/Metal/Vulkan/ROCm backends, GGUF and SafeTensors loading, and
//! an OpenAI-compatible HTTP server.
//!
//! # Why HTTP rather than an in-process dependency
//!
//! IronWorks is a 30-crate workspace and is not published on crates.io under the name
//! `ironworks` (that name belongs to an unrelated project). Depending on it by path
//! would make `iron-monitor` unpublishable and would pull an entire inference
//! engine, with its GPU toolchains, into every build of a hardware monitor.
//!
//! Talking to `ironworks serve` over its OpenAI-compatible API keeps this crate
//! publishable and dependency-free, lets IronWorks be upgraded independently, and
//! keeps model loading out of the monitoring process — which matters here, because
//! the whole point of the snapshot pipeline is that nothing heavy shares an address
//! space with the collectors.
//!
//! # Privacy
//!
//! IronWorks runs on your machine. Unlike the hosted backends, system telemetry sent
//! to it never leaves the host, so this is the backend to prefer when hardware state
//! is sensitive.
//!
//! # Starting a server
//!
//! ```text
//! ironworks run Qwen/Qwen3-8B-GGUF --mode serve --port 8080
//! ```
//!
//! # Example
//!
//! ```no_run
//! use ironmonlib::agent::local::{IronWorksClient, LocalInferenceClient, InferenceRequest};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = IronWorksClient::default_endpoint()?;
//!
//! if client.is_available().await {
//!     let response = client.generate(InferenceRequest {
//!         model: "qwen3-8b".to_string(),
//!         prompt: "Is this GPU temperature safe?".to_string(),
//!         system: Some("You are a hardware monitoring assistant.".to_string()),
//!         ..Default::default()
//!     }).await?;
//!     println!("{}", response.text);
//! }
//! # Ok(())
//! # }
//! ```

use super::{InferenceRequest, InferenceResponse, LocalInferenceClient, ModelInfo};
use crate::error::{IronError, Result};
use async_trait::async_trait;
#[cfg(feature = "remote-backends")]
use serde::Deserialize;
#[cfg(feature = "remote-backends")]
use serde::Serialize;
#[cfg(feature = "remote-backends")]
use std::time::Instant;

/// Default endpoint for a local `ironworks serve` instance.
///
/// Port 8080 matches the IronWorks server and CLI default.
pub const DEFAULT_ENDPOINT: &str = "http://localhost:8080";

/// Client for an IronWorks inference server.
#[derive(Debug, Clone)]
pub struct IronWorksClient {
    endpoint: String,
    #[cfg(feature = "remote-backends")]
    client: reqwest::Client,
}

impl IronWorksClient {
    /// Connect to an IronWorks server at `endpoint`.
    pub fn new(endpoint: &str) -> Result<Self> {
        #[cfg(feature = "remote-backends")]
        {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .map_err(|e| IronError::Network(e.to_string()))?;

            Ok(Self {
                endpoint: endpoint.trim_end_matches('/').to_string(),
                client,
            })
        }

        #[cfg(not(feature = "remote-backends"))]
        {
            let _ = endpoint;
            Err(IronError::NotImplemented(
                "IronWorks client requires the 'remote-backends' feature".to_string(),
            ))
        }
    }

    /// Connect to a local IronWorks server on the default port.
    ///
    /// Named `default_endpoint` rather than `default` because construction is
    /// fallible, which [`Default`] cannot express.
    pub fn default_endpoint() -> Result<Self> {
        Self::new(DEFAULT_ENDPOINT)
    }

    /// The endpoint this client talks to.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

/// One message in a chat completion request.
#[cfg(feature = "remote-backends")]
#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    stream: bool,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    message: Option<ChatResponseMessage>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    #[serde(default)]
    content: String,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    completion_tokens: Option<usize>,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

#[async_trait]
impl LocalInferenceClient for IronWorksClient {
    fn name(&self) -> &str {
        "IronWorks"
    }

    async fn is_available(&self) -> bool {
        #[cfg(feature = "remote-backends")]
        {
            let url = format!("{}/v1/models", self.endpoint);
            // A reachable-but-erroring server is not usable, so require success
            // rather than merely a completed request.
            match self.client.get(&url).send().await {
                Ok(response) => response.status().is_success(),
                Err(_) => false,
            }
        }

        #[cfg(not(feature = "remote-backends"))]
        false
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        #[cfg(feature = "remote-backends")]
        {
            let url = format!("{}/v1/models", self.endpoint);
            let response = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| IronError::Network(e.to_string()))?;

            if !response.status().is_success() {
                return Err(IronError::Agent(format!(
                    "IronWorks returned {} listing models",
                    response.status()
                )));
            }

            let models: ModelsResponse = response
                .json()
                .await
                .map_err(|e| IronError::Agent(format!("Failed to parse model list: {e}")))?;

            Ok(models
                .data
                .into_iter()
                .map(|m| ModelInfo {
                    name: m.id,
                    size: None,
                    family: None,
                    parameter_count: None,
                    quantization: None,
                })
                .collect())
        }

        #[cfg(not(feature = "remote-backends"))]
        Err(IronError::NotImplemented(
            "IronWorks client requires the 'remote-backends' feature".to_string(),
        ))
    }

    async fn generate(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        #[cfg(feature = "remote-backends")]
        {
            let start = Instant::now();

            // Chat completions rather than the legacy completions route: it carries
            // the system prompt as its own message, so the model reliably
            // distinguishes the hardware context from the user's question.
            let mut messages = Vec::with_capacity(2);
            if let Some(system) = request.system.filter(|s| !s.is_empty()) {
                messages.push(ChatMessage {
                    role: "system",
                    content: system,
                });
            }
            messages.push(ChatMessage {
                role: "user",
                content: request.prompt,
            });

            let body = ChatCompletionRequest {
                model: request.model.clone(),
                messages,
                max_tokens: request.max_tokens,
                temperature: request.temperature,
                top_p: request.top_p,
                stop: request.stop,
                // Streaming would require a different response parser; the agent
                // consumes whole answers.
                stream: false,
            };

            let url = format!("{}/v1/chat/completions", self.endpoint);
            let response = self
                .client
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| IronError::Network(e.to_string()))?;

            let status = response.status();
            if !status.is_success() {
                // Include the body: IronWorks reports the actual cause (unknown
                // model, context overflow) there, and the bare status is rarely
                // enough to act on.
                let detail = response.text().await.unwrap_or_default();
                return Err(IronError::Agent(format!(
                    "IronWorks API error {status}: {}",
                    detail.trim()
                )));
            }

            let completion: ChatCompletionResponse = response
                .json()
                .await
                .map_err(|e| IronError::Agent(format!("Failed to parse completion: {e}")))?;

            let choice = completion.choices.into_iter().next().ok_or_else(|| {
                IronError::Agent("IronWorks returned no completion choices".to_string())
            })?;

            let text = choice.message.map(|m| m.content).unwrap_or_default();

            // OpenAI-compatible servers report "length" when the token cap cut the
            // answer short; anything else means the model stopped on its own.
            let truncated = choice.finish_reason.as_deref() == Some("length");

            Ok(InferenceResponse {
                text: text.trim().to_string(),
                model: if completion.model.is_empty() {
                    request.model
                } else {
                    completion.model
                },
                tokens_generated: completion.usage.and_then(|u| u.completion_tokens),
                duration_ms: start.elapsed().as_millis() as u64,
                truncated,
            })
        }

        #[cfg(not(feature = "remote-backends"))]
        {
            let _ = request;
            Err(IronError::NotImplemented(
                "IronWorks client requires the 'remote-backends' feature".to_string(),
            ))
        }
    }

    async fn model_info(&self, model_name: &str) -> Result<ModelInfo> {
        let models = self.list_models().await?;
        models
            .into_iter()
            .find(|m| m.name == model_name)
            .ok_or_else(|| {
                IronError::Agent(format!(
                    "IronWorks is not serving a model named {model_name}"
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_endpoint_matches_ironworks_server_default() {
        // IronWorks' server and CLI both default to 8080; drifting from that would
        // make auto-detection silently fail against a stock server.
        assert_eq!(DEFAULT_ENDPOINT, "http://localhost:8080");
    }

    /// The three tests below construct a client, which `new()` refuses without
    /// the `remote-backends` feature. They were ungated and so failed in every
    /// build that does not enable it — invisible to CI, which checks
    /// `--all-features` and each feature *alone* and never the documented
    /// cross-check subset. Same shape as `examples/cpu_monitor.rs` failing on
    /// macOS without `apple`: a combination the cross-check can build is one CI
    /// never tries.
    #[cfg(feature = "remote-backends")]
    #[test]
    fn trailing_slash_is_normalized() {
        // Otherwise URLs become "http://host//v1/models", which some routers 404.
        let client = IronWorksClient::new("http://localhost:8080/").expect("client");
        assert_eq!(client.endpoint(), "http://localhost:8080");
    }

    #[cfg(feature = "remote-backends")]
    #[test]
    fn client_reports_its_name() {
        let client = IronWorksClient::default_endpoint().expect("client");
        assert_eq!(client.name(), "IronWorks");
        assert_eq!(client.endpoint(), DEFAULT_ENDPOINT);
    }

    /// An unreachable server must report unavailable rather than erroring, so
    /// discovery can fall through to the next backend.
    #[cfg(feature = "remote-backends")]
    #[tokio::test]
    async fn unreachable_server_is_reported_unavailable() {
        // Port 1 is reserved and never has a listener.
        let client = IronWorksClient::new("http://127.0.0.1:1").expect("client");
        assert!(!client.is_available().await);
    }

    /// The other side of the same contract: a build without the feature must
    /// decline with a reason rather than silently constructing something that
    /// cannot work. Without this, gating the three above would leave the
    /// featureless build asserting nothing at all.
    #[cfg(not(feature = "remote-backends"))]
    #[test]
    fn a_build_without_remote_backends_refuses_with_a_reason() {
        let err = IronWorksClient::new("http://localhost:8080")
            .expect_err("a client cannot be built without the feature");
        let why = err.to_string();
        assert!(
            why.contains("remote-backends"),
            "the refusal must name the feature that is missing: {why}"
        );
    }
}
