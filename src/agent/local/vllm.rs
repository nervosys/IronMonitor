//! vLLM Client - High-Performance Inference Server
//!
//! vLLM is a high-throughput serving engine with OpenAI-compatible API.
//! This module provides a client for vLLM servers.
//!
//! # Requirements
//!
//! - vLLM server running with OpenAI-compatible endpoint
//! - Default endpoint: http://localhost:8000
//!
//! # Features
//!
//! - PagedAttention for efficient memory management
//! - Continuous batching for high throughput
//! - OpenAI-compatible API
//! - Support for various model architectures
//!
//! # Example
//!
//! ```no_run
//! use ironmonlib::agent::local::{VllmClient, InferenceRequest, LocalInferenceClient};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = VllmClient::new("http://localhost:8000")?;
//!
//! let request = InferenceRequest {
//!     model: "meta-llama/Llama-3-8B".to_string(),
//!     prompt: "Analyze system performance".to_string(),
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
use std::time::Instant;

/// vLLM API client
#[derive(Debug, Clone)]
pub struct VllmClient {
    endpoint: String,
    #[cfg(feature = "remote-backends")]
    client: reqwest::Client,
}

impl VllmClient {
    /// Create new vLLM client
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
                "vLLM client requires 'remote-backends' feature".to_string(),
            ))
        }
    }

    /// Create default vLLM client (localhost:8000)
    ///
    /// Not `Default::default` — constructing this validates the endpoint and so
    /// can fail, which `Default` has no way to report. Renaming it would break
    /// callers on the published API for a purely cosmetic gain.
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Result<Self> {
        Self::new("http://localhost:8000")
    }
}

#[async_trait]
impl LocalInferenceClient for VllmClient {
    fn name(&self) -> &str {
        "vLLM"
    }

    async fn is_available(&self) -> bool {
        #[cfg(feature = "remote-backends")]
        {
            let url = format!("{}/v1/models", self.endpoint);
            self.client.get(&url).send().await.is_ok()
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
                return Err(IronError::Agent(
                    "Failed to list models from vLLM".to_string(),
                ));
            }

            let models: VllmModelsResponse = response
                .json()
                .await
                .map_err(|e| IronError::Agent(format!("Failed to parse models: {}", e)))?;

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
            "vLLM client requires 'remote-backends' feature".to_string(),
        ))
    }

    async fn generate(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        #[cfg(feature = "remote-backends")]
        {
            let start = Instant::now();

            // vLLM uses OpenAI-compatible API
            let request_body = VllmCompletionRequest {
                model: request.model.clone(),
                prompt: request.prompt,
                max_tokens: request.max_tokens,
                temperature: request.temperature,
                top_p: request.top_p,
                stop: request.stop,
            };

            let url = format!("{}/v1/completions", self.endpoint);
            let response = self
                .client
                .post(&url)
                .json(&request_body)
                .send()
                .await
                .map_err(|e| IronError::Network(e.to_string()))?;

            if !response.status().is_success() {
                return Err(IronError::Agent(format!(
                    "vLLM API error: {}",
                    response.status()
                )));
            }

            let vllm_response: VllmCompletionResponse = response
                .json()
                .await
                .map_err(|e| IronError::Agent(format!("Failed to parse response: {}", e)))?;

            let choice = vllm_response
                .choices
                .first()
                .ok_or_else(|| IronError::Agent("No response from vLLM".to_string()))?;

            Ok(InferenceResponse {
                text: choice.text.clone(),
                model: vllm_response.model,
                tokens_generated: Some(vllm_response.usage.completion_tokens),
                duration_ms: start.elapsed().as_millis() as u64,
                truncated: choice.finish_reason == "length",
            })
        }

        #[cfg(not(feature = "remote-backends"))]
        Err(IronError::NotImplemented(
            "vLLM client requires 'remote-backends' feature".to_string(),
        ))
    }

    async fn model_info(&self, model_name: &str) -> Result<ModelInfo> {
        Ok(ModelInfo {
            name: model_name.to_string(),
            size: None,
            family: None,
            parameter_count: None,
            quantization: None,
        })
    }
}

// vLLM API Types (OpenAI-compatible)

#[derive(Debug, Serialize)]
struct VllmCompletionRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct VllmCompletionResponse {
    model: String,
    choices: Vec<VllmChoice>,
    usage: VllmUsage,
}

#[derive(Debug, Deserialize)]
struct VllmChoice {
    text: String,
    finish_reason: String,
}

#[derive(Debug, Deserialize)]
struct VllmUsage {
    completion_tokens: usize,
}

#[derive(Debug, Deserialize)]
struct VllmModelsResponse {
    data: Vec<VllmModel>,
}

#[derive(Debug, Deserialize)]
struct VllmModel {
    id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vllm_client_creation() {
        let result = VllmClient::new("http://localhost:8000");
        #[cfg(feature = "remote-backends")]
        assert!(result.is_ok());
        #[cfg(not(feature = "remote-backends"))]
        assert!(result.is_err());
    }
}
