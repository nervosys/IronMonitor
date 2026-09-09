//! Lightweight inference engine for AI models
//!
//! This module provides ML-powered inference engines for generating responses
//! using local models and remote APIs.

use crate::agent::{AgentConfig, Query, RemoteClient, SystemState};
use crate::error::{IronError, Result};
use std::time::Instant;

/// Inference engine (ML-powered only)
pub struct InferenceEngine {
    config: AgentConfig,
    #[allow(dead_code)]
    initialized: bool,
    remote_client: RemoteClient,
}

impl InferenceEngine {
    /// Create new inference engine with configuration
    pub fn new(config: &AgentConfig) -> Result<Self> {
        let remote_client = if let Some(ref backend_config) = config.backend {
            RemoteClient::new(backend_config.clone())?
        } else {
            return Err(IronError::Configuration(
                "No backend configured. Agent requires an AI backend (Ollama, OpenAI, etc.)"
                    .to_string(),
            ));
        };

        Ok(Self {
            config: config.clone(),
            initialized: true,
            remote_client,
        })
    }

    /// Generate response based on query and system state
    pub fn generate_response(&mut self, query: &Query, state: &SystemState) -> Result<String> {
        let start = Instant::now();

        // Use ML backend for all responses
        let response = self.generate_ml_response(&self.remote_client, query, state)?;

        // Check timeout
        let elapsed = start.elapsed();
        if elapsed.as_secs() > self.config.timeout_seconds {
            return Err(IronError::Other(format!(
                "Inference timeout after {} seconds",
                elapsed.as_secs()
            )));
        }

        Ok(response)
    }

    /// Generate response using ML backend (local or remote)
    fn generate_ml_response(
        &self,
        client: &RemoteClient,
        query: &Query,
        state: &SystemState,
    ) -> Result<String> {
        // Check if the query contains embedded tool context (from AI Data API)
        let has_tool_context = query.text.contains("# Real-time System Data");

        // Build system prompt with context
        let system_prompt = if has_tool_context {
            // When tool context is embedded, instruct the AI to use it
            "You are a hardware monitoring assistant for IronMonitor. \
            The user's question includes REAL-TIME SYSTEM DATA in JSON format that was \
            automatically gathered from monitoring tools. Use this data to provide specific, \
            accurate answers. Reference actual values from the JSON (temperatures, memory usage, \
            GPU names, process names, etc.). Be concise and factual. \
            Do NOT describe how the tools work - just answer the question using the data provided."
                .to_string()
        } else {
            // Standard prompt with SystemState context
            // `to_context_string` already opens with "Current System State:", so this
            // used to print that header twice.
            format!(
                "You are a hardware monitoring assistant. Provide concise, factual answers \
                about system state. Keep responses under 200 words.\n\n{}",
                state.to_context_string()
            )
        };

        // Send query to ML backend
        let (response, _elapsed) = client.query(&system_prompt, &query.text)?;

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentConfig, BackendConfig};

    #[test]
    fn test_engine_requires_backend() {
        let config = AgentConfig::default();
        let engine = InferenceEngine::new(&config);
        assert!(engine.is_err()); // Should fail without backend
    }

    #[test]
    fn test_engine_with_backend() {
        let config = AgentConfig {
            backend: Some(BackendConfig::ollama("test-model")),
            ..Default::default()
        };
        // Note: This will still fail without Ollama running, but validates structure
        let _result = InferenceEngine::new(&config);
    }
}
