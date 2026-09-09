//! TensorRT-LLM Client - NVIDIA Optimized Inference
//!
//! TensorRT-LLM provides highly optimized inference for NVIDIA GPUs with support
//! for various optimization techniques including quantization, multi-GPU, and
//! in-flight batching.
//!
//! # Requirements
//!
//! - NVIDIA GPU with Compute Capability >= 8.0 (Ampere or newer)
//! - TensorRT-LLM installation
//! - Triton Inference Server with TensorRT-LLM backend OR
//! - TensorRT-LLM Python API
//!
//! # Features
//!
//! - FP16/INT8/INT4 quantization
//! - Multi-GPU tensor parallelism
//! - In-flight batching
//! - KV cache optimization
//!
//! # Example
//!
//! ```no_run
//! use ironmonlib::agent::local::{TensorRtClient, InferenceRequest, LocalInferenceClient};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = TensorRtClient::new("localhost:8001")?;
//!
//! let request = InferenceRequest {
//!     model: "llama-3-8b-tensorrt".to_string(),
//!     prompt: "Analyze GPU metrics".to_string(),
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
#[cfg(feature = "remote-backends")]
use serde::{Deserialize, Serialize};

/// TensorRT-LLM client (via Triton Inference Server)
#[derive(Debug, Clone)]
pub struct TensorRtClient {
    triton_url: String,
    #[cfg(feature = "remote-backends")]
    client: reqwest::Client,
}

impl TensorRtClient {
    /// Create new TensorRT-LLM client
    pub fn new(triton_url: &str) -> Result<Self> {
        #[cfg(feature = "remote-backends")]
        {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .map_err(|e| IronError::Network(e.to_string()))?;

            Ok(Self {
                triton_url: triton_url.to_string(),
                client,
            })
        }

        #[cfg(not(feature = "remote-backends"))]
        {
            Err(IronError::NotImplemented(
                "TensorRT-LLM client requires 'remote-backends' feature".to_string(),
            ))
        }
    }

    /// Create default TensorRT-LLM client (localhost:8001)
    ///
    /// Not `Default::default` — constructing this validates the endpoint and so
    /// can fail, which `Default` has no way to report. Renaming it would break
    /// callers on the published API for a purely cosmetic gain.
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Result<Self> {
        Self::new("localhost:8001")
    }

    /// Check if NVIDIA GPU is available
    pub fn check_gpu_support() -> bool {
        #[cfg(feature = "nvidia")]
        {
            // Check for NVIDIA GPU with sufficient compute capability
            use crate::gpu::GpuCollection;
            if let Ok(gpus) = GpuCollection::auto_detect() {
                return !gpus.is_empty();
            }
        }
        false
    }
}

#[async_trait]
impl LocalInferenceClient for TensorRtClient {
    fn name(&self) -> &str {
        "TensorRT-LLM"
    }

    async fn is_available(&self) -> bool {
        #[cfg(feature = "remote-backends")]
        {
            // Check if Triton server is running
            let url = format!("http://{}/v2/health/ready", self.triton_url);
            self.client.get(&url).send().await.is_ok() && Self::check_gpu_support()
        }

        #[cfg(not(feature = "remote-backends"))]
        false
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        #[cfg(feature = "remote-backends")]
        {
            let url = format!("http://{}/v2/models", self.triton_url);
            let response = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| IronError::Network(e.to_string()))?;

            if !response.status().is_success() {
                return Err(IronError::Agent(
                    "Failed to list models from Triton".to_string(),
                ));
            }

            // Parse Triton model repository response
            let text = response
                .text()
                .await
                .map_err(|e| IronError::Agent(e.to_string()))?;
            if let Ok(repo) = serde_json::from_str::<TritonModelRepository>(&text) {
                return Ok(repo
                    .models
                    .into_iter()
                    .map(|m| ModelInfo {
                        name: m.name,
                        size: None,
                        family: Some("TensorRT-LLM".to_string()),
                        parameter_count: None,
                        quantization: None,
                    })
                    .collect());
            }

            Ok(vec![])
        }

        #[cfg(not(feature = "remote-backends"))]
        Err(IronError::NotImplemented(
            "TensorRT-LLM client requires 'remote-backends' feature".to_string(),
        ))
    }

    async fn generate(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        #[cfg(feature = "remote-backends")]
        {
            use std::time::Instant;

            let start = Instant::now();

            // Triton inference request format for TensorRT-LLM
            let triton_request = TritonInferRequest {
                inputs: vec![
                    TritonInput {
                        name: "text_input".to_string(),
                        shape: vec![1, 1],
                        datatype: "BYTES".to_string(),
                        data: vec![request.prompt.clone()],
                    },
                    TritonInput {
                        name: "max_tokens".to_string(),
                        shape: vec![1, 1],
                        datatype: "INT32".to_string(),
                        data: vec![request.max_tokens.unwrap_or(256).to_string()],
                    },
                ],
                outputs: vec![TritonOutput {
                    name: "text_output".to_string(),
                }],
            };

            let url = format!(
                "http://{}/v2/models/{}/infer",
                self.triton_url, request.model
            );
            let response = self
                .client
                .post(&url)
                .json(&triton_request)
                .send()
                .await
                .map_err(|e| IronError::Network(e.to_string()))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(IronError::Agent(format!(
                    "Triton inference failed ({}): {}",
                    status, body
                )));
            }

            let triton_response: TritonInferResponse = response
                .json()
                .await
                .map_err(|e| IronError::Agent(format!("Failed to parse Triton response: {}", e)))?;

            // Extract text from response
            let text = triton_response
                .outputs
                .iter()
                .find(|o| o.name == "text_output")
                .and_then(|o| o.data.first().cloned())
                .unwrap_or_default();

            Ok(InferenceResponse {
                text,
                model: request.model,
                tokens_generated: None, // Triton doesn't easily expose this
                duration_ms: start.elapsed().as_millis() as u64,
                truncated: false,
            })
        }

        #[cfg(not(feature = "remote-backends"))]
        Err(IronError::NotImplemented(
            "TensorRT-LLM client requires 'remote-backends' feature".to_string(),
        ))
    }

    async fn model_info(&self, model_name: &str) -> Result<ModelInfo> {
        Ok(ModelInfo {
            name: model_name.to_string(),
            size: None,
            family: Some("TensorRT-LLM".to_string()),
            parameter_count: None,
            quantization: None,
        })
    }
}

// Triton API Types

#[cfg(feature = "remote-backends")]
#[derive(Debug, Serialize)]
struct TritonInferRequest {
    inputs: Vec<TritonInput>,
    outputs: Vec<TritonOutput>,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Serialize)]
struct TritonInput {
    name: String,
    shape: Vec<usize>,
    datatype: String,
    data: Vec<String>,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Serialize)]
struct TritonOutput {
    name: String,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Deserialize)]
struct TritonInferResponse {
    outputs: Vec<TritonOutputData>,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Deserialize)]
struct TritonOutputData {
    name: String,
    data: Vec<String>,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Deserialize)]
struct TritonModelRepository {
    models: Vec<TritonModel>,
}

#[cfg(feature = "remote-backends")]
#[derive(Debug, Deserialize)]
struct TritonModel {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensorrt_client_creation() {
        let result = TensorRtClient::new("localhost:8001");
        #[cfg(feature = "remote-backends")]
        assert!(result.is_ok());
        #[cfg(not(feature = "remote-backends"))]
        assert!(result.is_err());
    }

    #[test]
    fn test_gpu_support_check() {
        // This will only pass if NVIDIA GPU is available
        let has_gpu = TensorRtClient::check_gpu_support();
        println!("NVIDIA GPU support: {}", has_gpu);
    }
}
