//! Agent Backend Discovery and Configuration Example
//!
//! This example demonstrates:
//! - Automatic backend discovery
//! - Backend configuration
//! - Switching between local and remote backends
//! - Using different AI models (OpenAI, Anthropic, Ollama, IronWorks)

use ironmonlib::agent::{Agent, AgentConfig, BackendConfig, BackendDiscovery, BackendType};
use ironmonlib::UnifiedMonitor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("IronMonitor - AI Agent Backend Discovery\n");
    println!("{}", "=".repeat(60));

    // 1. Discover available backends
    println!("\n1. Discovering Available Backends...\n");
    let discovery = BackendDiscovery::discover();

    println!("Available backends:");
    for backend in discovery.available() {
        println!("  ✓ {}", backend.display_name());
        if backend.requires_api_key() {
            if let Some(env_var) = backend.api_key_env_var() {
                let status = if std::env::var(env_var).is_ok() {
                    "configured ✓"
                } else {
                    "not configured ✗"
                };
                println!("    API Key: {} ({})", env_var, status);
            }
        }
        if let Some(endpoint) = backend.default_endpoint() {
            println!("    Endpoint: {}", endpoint);
        }
    }

    let recommended = discovery.recommended();
    println!("\n  Recommended: {}", recommended.display_name());

    // 2. Whatever discovery found
    //
    // This step used to build an `AgentConfig::new(ModelSize::Medium)` and call it
    // the "rule-based built-in backend, always available". There is no such
    // backend: `Agent::new` requires one to be configured and returns
    // `Configuration` otherwise, so the example aborted here — five of its six
    // sections were unreachable. Use the discovered backend instead, and treat
    // "none available" as a thing to report rather than an error to die on.
    println!("\n{}", "=".repeat(60));
    println!("\n2. Using the Recommended Backend\n");

    let monitor = UnifiedMonitor::new()?;

    match AgentConfig::auto_detect().and_then(Agent::new) {
        // Don't `?` the query itself: this reaches a real backend, which can time
        // out or refuse. A demo that aborts on the first slow model is less useful
        // than one that says what happened and moves on to the rest of its output.
        Ok(mut agent) => match agent.ask("What's my GPU temperature?", &monitor) {
            Ok(response) => {
                println!("Query:    What's my GPU temperature?");
                println!("Response: {}", response.response);
                println!(
                    "Time:     {}ms ({})",
                    response.inference_time_ms,
                    recommended.display_name()
                );
            }
            Err(e) => println!("Query failed against {}: {e}", recommended.display_name()),
        },
        Err(e) => {
            println!("No backend could be configured: {e}");
            println!("Start a local server (`ollama serve`) or set a provider API key.");
        }
    }

    // 3. Ollama Backend (if available)
    println!("\n{}", "=".repeat(60));
    println!("\n3. Checking Ollama Backend (Local Server)\n");

    if discovery.is_available(&BackendType::RemoteOllama) {
        println!("✓ Ollama is running locally");

        #[cfg(feature = "remote-backends")]
        {
            // List available models
            let ollama_config = BackendConfig::ollama("llama3");
            let client = ironmonlib::agent::RemoteClient::new(ollama_config)?;

            match client.list_models() {
                Ok(models) => {
                    println!("\nAvailable models:");
                    for model in models.iter().take(5) {
                        println!("  • {}", model);
                    }
                }
                Err(e) => println!("Could not list models: {}", e),
            }

            // Create agent with Ollama
            println!("\nCreating agent with Ollama (llama3)...");
            let ollama_backend = BackendConfig::ollama("llama3");
            let config_ollama = AgentConfig::with_backend(ollama_backend);

            match Agent::new(config_ollama) {
                Ok(mut agent_ollama) => {
                    println!("✓ Agent created successfully");

                    let response = agent_ollama.ask("What's my GPU temperature?", &monitor)?;
                    println!("\nQuery:    What's my GPU temperature?");
                    println!("Response: {}", response.response);
                    println!("Time:     {}ms (Ollama)", response.inference_time_ms);
                }
                Err(e) => println!("✗ Failed to create agent: {}", e),
            }
        }

        #[cfg(not(feature = "remote-backends"))]
        println!(
            "  ℹ️  Remote backends require 'remote-backends' feature\n\
            Build with: cargo run --features remote-backends --example agent_backends"
        );
    } else {
        println!("✗ Ollama is not running");
        println!("  To use Ollama:");
        println!("  1. Install: https://ollama.com/download");
        println!("  2. Run: ollama serve");
        println!("  3. Pull model: ollama pull llama3");
    }

    // 4. OpenAI Backend (if API key available)
    println!("\n{}", "=".repeat(60));
    println!("\n4. Checking OpenAI Backend\n");

    if discovery.is_available(&BackendType::RemoteOpenAI) {
        println!("✓ OpenAI API key configured (OPENAI_API_KEY)");

        #[cfg(feature = "remote-backends")]
        {
            println!("\nCreating agent with OpenAI (gpt-5.6-terra)...");
            let openai_backend = BackendConfig::openai("gpt-5.6-terra", None);
            let config_openai = AgentConfig::with_backend(openai_backend);

            match Agent::new(config_openai) {
                Ok(mut agent_openai) => {
                    println!("✓ Agent created successfully");

                    let response = agent_openai.ask("What's my GPU temperature?", &monitor)?;
                    println!("\nQuery:    What's my GPU temperature?");
                    println!("Response: {}", response.response);
                    println!("Time:     {}ms (OpenAI)", response.inference_time_ms);
                }
                Err(e) => println!("✗ Failed to create agent: {}", e),
            }
        }
    } else {
        println!("✗ OpenAI API key not found");
        println!("  To use OpenAI:");
        println!("  1. Get API key: https://platform.openai.com/api-keys");
        println!("  2. Set: export OPENAI_API_KEY='your-key-here'");
    }

    // 5. Backend Comparison
    println!("\n{}", "=".repeat(60));
    println!("\n5. Backend Comparison\n");

    println!(
        "{:<25} {:<15} {:<15} {:<10}",
        "Backend", "Cost", "Speed", "Privacy"
    );
    println!("{}", "-".repeat(65));

    let backends = vec![
        ("IronWorks (built-in)", "Free", "~200ms", "100% local"),
        ("Ollama (local)", "Free", "~500ms", "100% local"),
        ("LM Studio (local)", "Free", "~400ms", "100% local"),
        // Cloud rates are per model and split into input and output prices, so a
        // single figure here would be wrong whichever model the reader picks.
        // GitHub Models is gone entirely — GitHub retired it on 2026-07-30.
        ("OpenAI", "see pricing", "~300ms", "Cloud"),
        ("Anthropic Claude", "see pricing", "~400ms", "Cloud"),
    ];

    for (name, cost, speed, privacy) in backends {
        println!("{:<25} {:<15} {:<15} {:<10}", name, cost, speed, privacy);
    }

    // 6. Configuration Examples
    println!("\n{}", "=".repeat(60));
    println!("\n6. Configuration Examples\n");

    // `AgentConfig::new` alone is not a runnable configuration — it sets no
    // backend, and `Agent::new` rejects that. Show the discovery path instead.
    println!("Auto-detected (Default):");
    println!(
        "  let config = AgentConfig::auto_detect()?;\n\
        let agent = Agent::new(config)?;\n"
    );

    println!("Ollama (Local):");
    println!(
        "  let backend = BackendConfig::ollama(\"llama3\");\n\
        let config = AgentConfig::with_backend(backend);\n\
        let agent = Agent::new(config)?;\n"
    );

    println!("OpenAI:");
    println!(
        "  let backend = BackendConfig::openai(\"gpt-5.6-terra\", None);\n\
        let config = AgentConfig::with_backend(backend);\n\
        let agent = Agent::new(config)?;\n"
    );

    println!("Anthropic Claude:");
    println!(
        "  let backend = BackendConfig::anthropic(\"claude-sonnet-5\", None);\n\
        let config = AgentConfig::with_backend(backend);\n\
        let agent = Agent::new(config)?;\n"
    );

    // 7. Recommendations
    println!("\n{}", "=".repeat(60));
    println!("\n7. Recommendations\n");

    println!("For quick testing:");
    println!("  → IronWorks, ironmon's built-in engine: local, no account needed\n");

    println!("For better reasoning (local):");
    println!("  → Ollama with llama3 or mistral");
    println!("  → LM Studio with any local model\n");

    println!("For best reasoning (cloud):");
    // Naming specific models here would date this example the way it already had.
    println!("  → OpenAI or Anthropic; ask each provider's /v1/models");
    println!("    endpoint for what your account can actually reach\n");

    println!("For cost-effective:");
    println!("  → A local server (Ollama, vLLM): no per-token cost at all");
    println!("  → Or a smaller hosted model; compare current provider pricing\n");

    Ok(())
}
