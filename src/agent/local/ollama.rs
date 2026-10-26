//! Ollama Client - Local LLM Server Integration
//!
//! Ollama is a popular tool for running large language models locally.
//! This module provides a client for the Ollama HTTP API.
//!
//! # Requirements
//!
//! - Ollama must be installed and running: <https://ollama.ai>
//! - Default endpoint: http://localhost:11434
//!
//! # Supported API Endpoints
//!
//! - `/api/generate` - Generate completion
//! - `/api/chat` - Chat completion (conversational)
//! - `/api/tags` - List available models
//! - `/api/show` - Get model information
//!
//! # Example
//!
//! ```no_run
//! use ironmonitor::agent::local::{OllamaClient, InferenceRequest, LocalInferenceClient};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = OllamaClient::new("http://localhost:11434")?;
//!
//! let request = InferenceRequest {
//!     model: "llama3".to_string(),
//!     prompt: "What is the current GPU temperature?".to_string(),
//!     ..Default::default()
//! };
//!
//! let response = client.generate(request).await?;
//! println!("{}", response.text);
//! # Ok(())
//! # }
//! ```

use super::{InferenceRequest, InferenceResponse, LocalInferenceClient, ModelInfo};
use crate::error::{IronError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::time::Instant;

/// Ollama API client
#[derive(Debug, Clone)]
pub struct OllamaClient {
    #[allow(dead_code)]
    endpoint: String,
    #[cfg(feature = "remote-backends")]
    client: reqwest::Client,
}

impl OllamaClient {
    /// Create new Ollama client
    #[allow(unused_variables)]
    pub fn new(endpoint: &str) -> Result<Self> {
        #[cfg(feature = "remote-backends")]
        {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .map_err(|e| IronError::Network(e.to_string()))?;

            Ok(Self {
                endpoint: endpoint.trim_end_matches('/').to_string(),
                client,
            })
        }

        #[cfg(not(feature = "remote-backends"))]
        {
            Err(IronError::NotImplemented(
                "Ollama client requires 'remote-backends' feature".to_string(),
            ))
        }
    }

    /// Create default Ollama client (localhost:11434)
    ///
    /// Not `Default::default`: constructing a client can fail, and that trait has no
    /// way to report it. Renaming would break callers for no gain.
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Result<Self> {
        Self::new("http://localhost:11434")
    }

    /// Generate text (non-streaming)
    #[cfg(feature = "remote-backends")]
    pub async fn generate_simple(&self, model: &str, prompt: &str) -> Result<String> {
        let request = InferenceRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            ..Default::default()
        };

        let response = self.generate(request).await?;
        Ok(response.text)
    }

    /// Chat completion (conversational format)
    #[cfg(feature = "remote-backends")]
    pub async fn chat(&self, model: &str, messages: Vec<ChatMessage>) -> Result<InferenceResponse> {
        let start = Instant::now();

        let request_body = OllamaChatRequest {
            model: model.to_string(),
            messages,
            stream: false,
            options: None,
        };

        let url = format!("{}/api/chat", self.endpoint);
        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| IronError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IronError::Agent(format!(
                "Ollama API error: {}",
                response.status()
            )));
        }

        let ollama_response: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| IronError::Agent(format!("Failed to parse response: {}", e)))?;

        Ok(InferenceResponse {
            text: ollama_response.message.content,
            model: ollama_response.model,
            tokens_generated: ollama_response.eval_count.map(|n| n as usize),
            duration_ms: start.elapsed().as_millis() as u64,
            truncated: ollama_response
                .done_reason
                .as_deref()
                .map(|reason| reason == "length"),
        })
    }

    /// Pull/download a model
    #[cfg(feature = "remote-backends")]
    pub async fn pull_model(&self, model: &str) -> Result<()> {
        let url = format!("{}/api/pull", self.endpoint);

        let request_body = serde_json::json!({
            "name": model,
            "stream": false,
        });

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| IronError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(IronError::Agent(format!(
                "Failed to pull model: {}",
                response.status()
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl LocalInferenceClient for OllamaClient {
    fn name(&self) -> &str {
        "Ollama"
    }

    async fn is_available(&self) -> bool {
        #[cfg(feature = "remote-backends")]
        {
            // Any HTTP answer used to count. An unrelated server on the port was then
            // reported as Ollama, so the listing's shape is required too.
            let url = format!("{}/api/tags", self.endpoint);
            match self.client.get(&url).send().await {
                Ok(response) if response.status().is_success() => response
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .is_some_and(|body| body.get("models").is_some_and(|m| m.is_array())),
                _ => false,
            }
        }

        #[cfg(not(feature = "remote-backends"))]
        false
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        #[cfg(feature = "remote-backends")]
        {
            let url = format!("{}/api/tags", self.endpoint);
            let response = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| IronError::Network(e.to_string()))?;

            if !response.status().is_success() {
                return Err(IronError::Agent(
                    "Failed to list models from Ollama".to_string(),
                ));
            }

            let tags: OllamaTagsResponse = response
                .json()
                .await
                .map_err(|e| IronError::Agent(format!("Failed to parse models: {}", e)))?;

            Ok(tags
                .models
                .into_iter()
                .map(|m| ModelInfo {
                    name: m.name,
                    size: Some(m.size),
                    family: m.details.family,
                    parameter_count: m.details.parameter_size,
                    quantization: m.details.quantization_level,
                })
                .collect())
        }

        #[cfg(not(feature = "remote-backends"))]
        Err(IronError::NotImplemented(
            "Ollama client requires 'remote-backends' feature".to_string(),
        ))
    }

    #[allow(unused_variables)]
    async fn generate(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        #[cfg(feature = "remote-backends")]
        {
            let start = Instant::now();

            let mut options = OllamaOptions::default();
            if let Some(temp) = request.temperature {
                options.temperature = Some(temp);
            }
            if let Some(tokens) = request.max_tokens {
                options.num_predict = Some(tokens as i32);
            }
            if let Some(top_p) = request.top_p {
                options.top_p = Some(top_p);
            }

            let request_body = OllamaGenerateRequest {
                model: request.model.clone(),
                prompt: request.prompt.clone(),
                system: request.system,
                stream: false,
                options: Some(options),
            };

            let url = format!("{}/api/generate", self.endpoint);
            let response = self
                .client
                .post(&url)
                .json(&request_body)
                .send()
                .await
                .map_err(|e| IronError::Network(e.to_string()))?;

            if !response.status().is_success() {
                return Err(IronError::Agent(format!(
                    "Ollama API error: {}",
                    response.status()
                )));
            }

            let ollama_response: OllamaGenerateResponse = response
                .json()
                .await
                .map_err(|e| IronError::Agent(format!("Failed to parse response: {}", e)))?;

            // `done` is true for every completed generation, including a normal stop,
            // so `truncated` used to be inverted: every complete answer read as cut
            // off. The stop reason is `done_reason`, "length" when the token limit hit.
            // A missing eval_count used to become a measured zero tokens; it is now
            // unknown.
            Ok(InferenceResponse {
                text: ollama_response.response,
                model: ollama_response.model,
                tokens_generated: ollama_response.eval_count.map(|n| n as usize),
                duration_ms: start.elapsed().as_millis() as u64,
                truncated: ollama_response
                    .done_reason
                    .as_deref()
                    .map(|reason| reason == "length"),
            })
        }

        #[cfg(not(feature = "remote-backends"))]
        Err(IronError::NotImplemented(
            "Ollama client requires 'remote-backends' feature".to_string(),
        ))
    }

    #[allow(unused_variables)]
    async fn model_info(&self, model_name: &str) -> Result<ModelInfo> {
        #[cfg(feature = "remote-backends")]
        {
            let url = format!("{}/api/show", self.endpoint);

            let request_body = serde_json::json!({
                "name": model_name,
            });

            let response = self
                .client
                .post(&url)
                .json(&request_body)
                .send()
                .await
                .map_err(|e| IronError::Network(e.to_string()))?;

            if !response.status().is_success() {
                return Err(IronError::Agent(format!("Model {} not found", model_name)));
            }

            let show_response: OllamaShowResponse = response
                .json()
                .await
                .map_err(|e| IronError::Agent(format!("Failed to parse model info: {}", e)))?;

            Ok(ModelInfo {
                name: model_name.to_string(),
                size: Some(show_response.size),
                family: show_response.details.family,
                parameter_count: show_response.details.parameter_size,
                quantization: show_response.details.quantization_level,
            })
        }

        #[cfg(not(feature = "remote-backends"))]
        Err(IronError::NotImplemented(
            "Ollama client requires 'remote-backends' feature".to_string(),
        ))
    }
}

// Ollama API Types
// These are used when the remote-backends feature is enabled
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
struct OllamaGenerateResponse {
    model: String,
    response: String,
    done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    done_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    eval_count: Option<i32>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "system", "user", "assistant"
    pub content: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
struct OllamaChatResponse {
    model: String,
    message: ChatMessage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    done_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    eval_count: Option<i32>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    size: u64,
    details: OllamaModelDetails,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OllamaModelDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameter_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantization_level: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OllamaShowResponse {
    size: u64,
    details: OllamaModelDetails,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_client_creation() {
        let result = OllamaClient::new("http://localhost:11434");
        #[cfg(feature = "remote-backends")]
        assert!(result.is_ok());
        #[cfg(not(feature = "remote-backends"))]
        assert!(result.is_err());
    }

    /// A completed generation reports `done: true` whether it stopped naturally or
    /// hit the token limit, so `done` says nothing about truncation. The stop reason
    /// is `done_reason`, and a response without it must not be read as a stop.
    #[test]
    fn stop_reason_comes_from_done_reason_not_done() {
        let natural: OllamaGenerateResponse = serde_json::from_str(
            r#"{"model":"m","response":"ok","done":true,"done_reason":"stop","eval_count":12}"#,
        )
        .expect("captured-shape response parses");
        assert!(natural.done);
        assert_eq!(natural.done_reason.as_deref(), Some("stop"));
        assert_eq!(natural.eval_count, Some(12));

        let cut_off: OllamaGenerateResponse = serde_json::from_str(
            r#"{"model":"m","response":"partial","done":true,"done_reason":"length"}"#,
        )
        .expect("length-stopped response parses");
        assert_eq!(cut_off.done_reason.as_deref(), Some("length"));
        // No eval_count in the body: the token count is unknown, not zero.
        assert_eq!(cut_off.eval_count, None);

        let unreported: OllamaGenerateResponse =
            serde_json::from_str(r#"{"model":"m","response":"x","done":true}"#)
                .expect("response without a stop reason parses");
        assert_eq!(unreported.done_reason, None);
    }

    #[test]
    fn test_default_client() {
        let result = OllamaClient::default();
        #[cfg(feature = "remote-backends")]
        assert!(result.is_ok());
        #[cfg(not(feature = "remote-backends"))]
        assert!(result.is_err());
    }
}
