//! Remote Backend Client - HTTP API Integration
//!
//! This module provides HTTP client implementation for remote AI backends
//! (OpenAI, Anthropic, Ollama, etc.)

use crate::agent::backend::BackendConfig;
use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::time::Instant;

/// OpenAI-compatible chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Ollama-specific chat request
#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct OllamaOptions {
    temperature: f32,
    num_predict: i32,
}

/// Ollama chat response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaChatResponse {
    message: ChatMessage,
    #[allow(dead_code)]
    done: bool,
}

/// OpenAI-compatible chat request
#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
}

/// OpenAI-compatible chat response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    #[allow(dead_code)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ChatChoice {
    message: ChatMessage,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Usage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

/// Remote backend client
pub struct RemoteClient {
    #[allow(dead_code)]
    config: BackendConfig,
    #[cfg(feature = "remote-backends")]
    http_client: reqwest::blocking::Client,
}

impl RemoteClient {
    /// Create new remote client
    pub fn new(config: BackendConfig) -> Result<Self> {
        config.validate()?;

        #[cfg(feature = "remote-backends")]
        {
            let mut builder = reqwest::blocking::Client::builder()
                .timeout(config.timeout)
                .pool_max_idle_per_host(0); // Disable connection pooling to avoid stale connections

            // Increase timeout for locally hosted servers: they load models on demand
            // and run on whatever hardware the user has, so first-token latency is far
            // less predictable than a hosted API's.
            if matches!(
                config.backend_type,
                crate::agent::backend::BackendType::IronWorks
                    | crate::agent::backend::BackendType::RemoteOllama
                    | crate::agent::backend::BackendType::RemoteLMStudio
                    | crate::agent::backend::BackendType::RemoteVllm
                    | crate::agent::backend::BackendType::RemoteTensorRT
            ) {
                builder = builder.timeout(std::time::Duration::from_secs(120)); // 2 minute timeout for local inference
            }

            let http_client = builder
                .build()
                .map_err(|e| IronError::Network(format!("Failed to create HTTP client: {}", e)))?;

            Ok(Self {
                config,
                http_client,
            })
        }

        #[cfg(not(feature = "remote-backends"))]
        {
            Err(IronError::NotImplemented(
                "Remote backends require 'remote-backends' feature".into(),
            ))
        }
    }

    /// Send query to remote backend
    pub fn query(&self, system_prompt: &str, user_query: &str) -> Result<(String, u64)> {
        #[cfg(feature = "remote-backends")]
        {
            let start = Instant::now();

            // CLI providers are driven as subprocesses rather than over HTTP, so they
            // are dispatched before any endpoint or message array is built.
            if let crate::agent::backend::BackendType::Cli(provider) = self.config.backend_type {
                let client = crate::agent::local::CliClient::detect(provider).ok_or_else(|| {
                    IronError::Configuration(format!(
                        "{} is configured but its executable is no longer on PATH",
                        provider.display_name()
                    ))
                })?;
                return client.with_timeout(self.config.timeout).query_blocking(
                    system_prompt,
                    user_query,
                    Some(&self.config.model_id),
                );
            }

            let messages = vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_query.to_string(),
                },
            ];

            let endpoint = self
                .config
                .endpoint
                .as_ref()
                .ok_or_else(|| IronError::Configuration("No endpoint configured".into()))?;

            // Handle Ollama differently from OpenAI-compatible APIs
            if matches!(
                self.config.backend_type,
                crate::agent::backend::BackendType::RemoteOllama
            ) {
                let request = OllamaChatRequest {
                    model: self.config.model_id.clone(),
                    messages,
                    stream: false,
                    options: Some(OllamaOptions {
                        temperature: self.config.temperature,
                        num_predict: self.config.max_tokens as i32,
                    }),
                };

                let url = format!("{}/api/chat", endpoint);
                let response = self
                    .http_client
                    .post(&url)
                    .json(&request)
                    .send()
                    .map_err(|e| IronError::Network(format!("Request failed: {}", e)))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let error_text = response
                        .text()
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    return Err(IronError::Network(format!(
                        "API error {}: {}",
                        status, error_text
                    )));
                }

                let ollama_response: OllamaChatResponse = response
                    .json()
                    .map_err(|e| IronError::Parse(format!("Failed to parse response: {}", e)))?;

                let elapsed = start.elapsed().as_millis() as u64;
                return Ok((ollama_response.message.content, elapsed));
            }

            // OpenAI-compatible APIs
            let request = ChatCompletionRequest {
                model: self.config.model_id.clone(),
                messages,
                temperature: Some(self.config.temperature),
                max_tokens: Some(self.config.max_tokens),
            };

            let url = format!("{}/chat/completions", endpoint);

            let mut req = self.http_client.post(&url).json(&request);

            // Add authentication if needed
            if let Some(ref api_key) = self.config.api_key {
                req = match self.config.backend_type {
                    crate::agent::backend::BackendType::RemoteAnthropic => req
                        .header("x-api-key", api_key)
                        .header("anthropic-version", "2023-06-01"),
                    _ => req.header("Authorization", format!("Bearer {}", api_key)),
                };
            }

            let response = req
                .send()
                .map_err(|e| IronError::Network(format!("Request failed: {}", e)))?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response
                    .text()
                    .unwrap_or_else(|_| "Unknown error".to_string());
                return Err(IronError::Network(format!(
                    "API error {}: {}",
                    status, error_text
                )));
            }

            let completion: ChatCompletionResponse = response
                .json()
                .map_err(|e| IronError::Parse(format!("Failed to parse response: {}", e)))?;

            let response_text = completion
                .choices
                .first()
                .ok_or_else(|| IronError::Parse("No choices in response".into()))?
                .message
                .content
                .clone();

            let elapsed = start.elapsed().as_millis() as u64;

            Ok((response_text, elapsed))
        }

        #[cfg(not(feature = "remote-backends"))]
        {
            let _ = (system_prompt, user_query);
            Err(IronError::NotImplemented(
                "Remote backends require 'remote-backends' feature".into(),
            ))
        }
    }

    /// Check if backend is available (health check)
    pub fn is_available(&self) -> bool {
        #[cfg(feature = "remote-backends")]
        {
            if let Some(ref endpoint) = self.config.endpoint {
                // Simple GET request to check availability
                let url = format!("{}/models", endpoint);
                self.http_client
                    .get(&url)
                    .timeout(std::time::Duration::from_secs(2))
                    .send()
                    .is_ok()
            } else {
                false
            }
        }

        #[cfg(not(feature = "remote-backends"))]
        {
            false
        }
    }

    /// List available models (for Ollama, LM Studio, etc.)
    pub fn list_models(&self) -> Result<Vec<String>> {
        #[cfg(feature = "remote-backends")]
        {
            let endpoint = self
                .config
                .endpoint
                .as_ref()
                .ok_or_else(|| IronError::Configuration("No endpoint configured".into()))?;

            let url = format!("{}/models", endpoint);

            let response = self
                .http_client
                .get(&url)
                .send()
                .map_err(|e| IronError::Network(format!("Failed to list models: {}", e)))?;

            if !response.status().is_success() {
                return Err(IronError::Network(format!(
                    "Failed to list models: {}",
                    response.status()
                )));
            }

            // Parse response (format varies by backend)
            let json: serde_json::Value = response
                .json()
                .map_err(|e| IronError::Parse(format!("Failed to parse models list: {}", e)))?;

            // Extract model names (OpenAI format)
            if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                let models = data
                    .iter()
                    .filter_map(|m| m.get("id").and_then(|id| id.as_str()))
                    .map(|s| s.to_string())
                    .collect();
                return Ok(models);
            }

            // Extract model names (Ollama format)
            if let Some(models) = json.get("models").and_then(|m| m.as_array()) {
                let model_names = models
                    .iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
                    .map(|s| s.to_string())
                    .collect();
                return Ok(model_names);
            }

            Ok(vec![])
        }

        #[cfg(not(feature = "remote-backends"))]
        {
            Err(IronError::NotImplemented(
                "Remote backends require 'remote-backends' feature".into(),
            ))
        }
    }
}

/// Remote backend builder for easy configuration
pub struct RemoteClientBuilder {
    config: BackendConfig,
}

impl RemoteClientBuilder {
    /// Start building an OpenAI client
    pub fn openai(model: &str) -> Self {
        Self {
            config: BackendConfig::openai(model, None),
        }
    }

    /// Start building an Anthropic client
    pub fn anthropic(model: &str) -> Self {
        Self {
            config: BackendConfig::anthropic(model, None),
        }
    }

    /// Start building an Ollama client
    pub fn ollama(model: &str) -> Self {
        Self {
            config: BackendConfig::ollama(model),
        }
    }

    /// Start building an LM Studio client
    pub fn lm_studio(model: &str) -> Self {
        Self {
            config: BackendConfig::lm_studio(model),
        }
    }

    /// Set API key
    pub fn api_key(mut self, key: String) -> Self {
        self.config.api_key = Some(key);
        self
    }

    /// Set custom endpoint
    pub fn endpoint(mut self, endpoint: String) -> Self {
        self.config.endpoint = Some(endpoint);
        self
    }

    /// Set temperature
    pub fn temperature(mut self, temp: f32) -> Self {
        self.config.temperature = temp;
        self
    }

    /// Set max tokens
    pub fn max_tokens(mut self, tokens: usize) -> Self {
        self.config.max_tokens = tokens;
        self
    }

    /// Build the client
    pub fn build(self) -> Result<RemoteClient> {
        RemoteClient::new(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_client_builder() {
        let builder = RemoteClientBuilder::ollama("llama3")
            .temperature(0.5)
            .max_tokens(512);

        assert_eq!(builder.config.model_id, "llama3");
        assert_eq!(builder.config.temperature, 0.5);
        assert_eq!(builder.config.max_tokens, 512);
    }
}
