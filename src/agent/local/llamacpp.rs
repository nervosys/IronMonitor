//! llama.cpp Integration - Direct GGUF Model Loading
//!
//! This module provides integration with llama.cpp for running GGUF models
//! directly without requiring a separate server.
//!
//! # Requirements
//!
//! - llama.cpp library (via llama-cpp-rs bindings) OR
//! - llama.cpp CLI executable in PATH (llama-cli or main)
//!
//! # Model Format
//!
//! Supports GGUF format models with various quantization levels:
//! - Q4_K_M: 4-bit medium quantization (recommended, good balance)
//! - Q5_K_M: 5-bit medium quantization (better quality)
//! - Q8_0: 8-bit quantization (best quality, larger)
//! - F16: 16-bit float (full precision, very large)
//!
//! # Example
//!
//! ```no_run
//! use ironmonlib::agent::local::{LlamaCppClient, InferenceRequest, LocalInferenceClient};
//! use std::path::PathBuf;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let model_path = PathBuf::from("models/llama-3-8b-Q4_K_M.gguf");
//! let client = LlamaCppClient::new(model_path)?;
//!
//! let request = InferenceRequest {
//!     model: "llama-3-8b".to_string(),
//!     prompt: "Analyze GPU temperature data".to_string(),
//!     max_tokens: Some(256),
//!     temperature: Some(0.3),
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
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

/// llama.cpp client for direct GGUF model inference
#[derive(Debug)]
pub struct LlamaCppClient {
    model_path: PathBuf,
    model_name: String,
    /// Path to llama.cpp executable (llama-cli, main, or llama-server)
    executable: Option<PathBuf>,
}

impl LlamaCppClient {
    /// Create new llama.cpp client with model path
    pub fn new(model_path: PathBuf) -> Result<Self> {
        if !model_path.exists() {
            return Err(IronError::Configuration(format!(
                "Model file not found: {}",
                model_path.display()
            )));
        }

        let model_name = model_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Try to find llama.cpp executable
        let executable = Self::find_llamacpp_executable();

        Ok(Self {
            model_path,
            model_name,
            executable,
        })
    }

    /// Find llama.cpp executable in common locations
    fn find_llamacpp_executable() -> Option<PathBuf> {
        // Common executable names
        let exe_names = ["llama-cli", "main", "llama.cpp"];

        // Check PATH
        for name in &exe_names {
            if let Ok(output) = Command::new("where").arg(name).output() {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout);
                    let first_line = path.lines().next()?;
                    return Some(PathBuf::from(first_line.trim()));
                }
            }

            // Try Unix-style which
            if let Ok(output) = Command::new("which").arg(name).output() {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout);
                    return Some(PathBuf::from(path.trim()));
                }
            }
        }

        // Check common installation paths
        let common_paths = [
            PathBuf::from("llama.cpp/build/bin/llama-cli"),
            PathBuf::from("llama.cpp/main"),
            PathBuf::from("/usr/local/bin/llama-cli"),
            PathBuf::from("C:\\llama.cpp\\build\\bin\\Release\\llama-cli.exe"),
        ];

        for path in &common_paths {
            if path.exists() {
                return Some(path.clone());
            }
        }

        None
    }

    /// Load model from standard locations
    pub fn auto_discover() -> Result<Self> {
        // Common model locations
        let search_paths = vec![
            PathBuf::from("models"),
            PathBuf::from("~/.cache/lm-studio/models"),
            PathBuf::from("~/.ollama/models"),
        ];

        for base_path in search_paths {
            if base_path.exists() {
                // Look for .gguf files
                if let Ok(entries) = std::fs::read_dir(&base_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("gguf") {
                            return Self::new(path);
                        }
                    }
                }
            }
        }

        Err(IronError::Configuration(
            "No GGUF models found in standard locations".to_string(),
        ))
    }
}

#[async_trait]
impl LocalInferenceClient for LlamaCppClient {
    fn name(&self) -> &str {
        "llama.cpp"
    }

    async fn is_available(&self) -> bool {
        self.model_path.exists() && self.executable.is_some()
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Only one model loaded at a time
        Ok(vec![ModelInfo {
            name: self.model_name.clone(),
            size: std::fs::metadata(&self.model_path).ok().map(|m| m.len()),
            family: None, // Could parse from filename
            parameter_count: None,
            quantization: None,
        }])
    }

    async fn generate(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        let exe = self.executable.as_ref().ok_or_else(|| {
            IronError::NotImplemented(
                "llama.cpp executable not found. Install llama.cpp and ensure llama-cli is in PATH.".to_string(),
            )
        })?;

        let start = Instant::now();

        // Build command arguments
        let mut cmd = Command::new(exe);
        cmd.arg("-m")
            .arg(&self.model_path)
            .arg("-p")
            .arg(&request.prompt)
            .arg("-n")
            .arg(request.max_tokens.unwrap_or(256).to_string())
            .arg("--no-display-prompt"); // Don't echo prompt in output

        if let Some(temp) = request.temperature {
            cmd.arg("--temp").arg(temp.to_string());
        }

        if let Some(top_p) = request.top_p {
            cmd.arg("--top-p").arg(top_p.to_string());
        }

        // Execute synchronously (async subprocess would require tokio::process)
        let output = cmd
            .output()
            .map_err(|e| IronError::CommandFailed(format!("Failed to run llama.cpp: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(IronError::CommandFailed(format!(
                "llama.cpp failed: {}",
                stderr
            )));
        }

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok(InferenceResponse {
            text,
            model: self.model_name.clone(),
            tokens_generated: None, // Would need to parse llama.cpp output
            duration_ms: start.elapsed().as_millis() as u64,
            truncated: false,
        })
    }

    async fn model_info(&self, _model_name: &str) -> Result<ModelInfo> {
        Ok(ModelInfo {
            name: self.model_name.clone(),
            size: std::fs::metadata(&self.model_path).ok().map(|m| m.len()),
            family: None,
            parameter_count: None,
            quantization: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llamacpp_client_invalid_path() {
        let result = LlamaCppClient::new(PathBuf::from("nonexistent.gguf"));
        assert!(result.is_err());
    }

    #[test]
    fn test_find_executable() {
        // This will likely return None unless llama.cpp is installed
        let exe = LlamaCppClient::find_llamacpp_executable();
        println!("llama.cpp executable: {:?}", exe);
    }
}
