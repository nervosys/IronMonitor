//! Example: Using Ollama for local AI inference
//!
//! This example demonstrates how to use the Ollama client for local LLM inference
//! to analyze system monitoring data.
//!
//! # Prerequisites
//!
//! 1. Install Ollama: https://ollama.ai
//! 2. Pull a model: `ollama pull llama3`
//! 3. Start Ollama (usually starts automatically)
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --example ollama_example --features "cli,remote-backends,nvidia"
//! ```

use ironmonlib::agent::local::{InferenceRequest, OllamaClient};
use ironmonlib::agent::LocalInferenceClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Ollama Local AI Inference Example ===\n");

    // Create Ollama client
    let client = match OllamaClient::default() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create Ollama client: {}", e);
            eprintln!("\nMake sure Ollama is installed and running:");
            eprintln!("  1. Install: https://ollama.ai");
            eprintln!("  2. Run: ollama pull llama3");
            eprintln!("  3. Ollama should start automatically\n");
            return Ok(());
        }
    };

    // Check if Ollama is available
    println!("Checking Ollama availability...");
    if !client.is_available().await {
        eprintln!("[!] Ollama server is not running");
        eprintln!("    Start Ollama and try again\n");
        return Ok(());
    }
    println!("[OK] Ollama is running\n");

    // List available models
    println!("Available models:");
    match client.list_models().await {
        Ok(models) => {
            if models.is_empty() {
                println!("  No models found. Pull a model first:");
                println!("    ollama pull llama3\n");
                return Ok(());
            }

            for model in &models {
                println!(
                    "  * {} ({} MB)",
                    model.name,
                    model.size.unwrap_or(0) / 1_000_000
                );
                if let Some(ref family) = model.family {
                    println!("    Family: {}", family);
                }
                if let Some(ref params) = model.parameter_count {
                    println!("    Parameters: {}", params);
                }
                if let Some(ref quant) = model.quantization {
                    println!("    Quantization: {}", quant);
                }
            }
            println!();

            // Use the first available model for demonstration
            let model_name = &models[0].name;
            println!("Using model: {}\n", model_name);

            // Example 1: Simple system query
            println!("=== Example 1: Simple Query ===");
            let request = InferenceRequest {
                model: model_name.clone(),
                prompt: "Explain what GPU temperature monitoring is in one sentence.".to_string(),
                max_tokens: Some(100),
                temperature: Some(0.3),
                ..Default::default()
            };

            match client.generate(request).await {
                Ok(response) => {
                    println!("Query: Explain what GPU temperature monitoring is");
                    println!("Response: {}", response.text.trim());
                    println!("Tokens: {}", response.tokens_generated.unwrap_or(0));
                    println!("Duration: {}ms\n", response.duration_ms);
                }
                Err(e) => eprintln!("Generation error: {}\n", e),
            }

            // Example 2: Hardware analysis query
            println!("=== Example 2: Hardware Analysis ===");
            let request = InferenceRequest {
                model: model_name.clone(),
                prompt: "What are safe operating temperatures for a GPU? Answer in 2 sentences."
                    .to_string(),
                max_tokens: Some(150),
                temperature: Some(0.3),
                ..Default::default()
            };

            match client.generate(request).await {
                Ok(response) => {
                    println!("Query: What are safe GPU temperatures?");
                    println!("Response: {}", response.text.trim());
                    println!("Duration: {}ms\n", response.duration_ms);
                }
                Err(e) => eprintln!("Generation error: {}\n", e),
            }

            // Example 3: System state analysis (simulated)
            println!("=== Example 3: System State Analysis ===");
            let system_context = r#"
Current System State:
- GPU 0: NVIDIA RTX 4090, 65C, 85% utilization, 18GB/24GB memory
- GPU 1: NVIDIA RTX 4090, 72C, 92% utilization, 22GB/24GB memory
- CPU: 45C, 60% utilization
- RAM: 48GB/64GB used
"#;

            let request = InferenceRequest {
                model: model_name.clone(),
                prompt: format!(
                    "{}\n\nBased on this system state, should I be concerned about GPU 1's temperature? \
                    Answer in 2 sentences.",
                    system_context
                ),
                max_tokens: Some(150),
                temperature: Some(0.3),
                ..Default::default()
            };

            match client.generate(request).await {
                Ok(response) => {
                    println!("System Context:");
                    println!("{}", system_context.trim());
                    println!("\nQuery: Should I be concerned about GPU 1's temperature?");
                    println!("Response: {}", response.text.trim());
                    println!("Duration: {}ms\n", response.duration_ms);
                }
                Err(e) => eprintln!("Generation error: {}\n", e),
            }
        }
        Err(e) => {
            eprintln!("Failed to list models: {}", e);
        }
    }

    println!("=== Example Complete ===");
    Ok(())
}
