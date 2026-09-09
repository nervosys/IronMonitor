//! AI Monitor (amon) - Syntactic sugar for `ironmon ai`
//!
//! This binary provides a simpler interface for AI-related commands.
//!
//! Usage:
//!   amon                    - Enter interactive query mode
//!   amon query \[question\]   - Ask a question
//!   amon manifest \[opts\]    - Export tool manifests for AI agents
//!   amon server             - Start MCP server for Claude Desktop

#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;

#[cfg(feature = "cli")]
#[derive(Parser)]
#[command(name = "amon")]
#[command(about = "AI Monitor: Natural language interface to system hardware", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<AmonCommand>,

    /// List available AI backends
    #[arg(long)]
    list_backends: bool,
}

#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum AmonCommand {
    /// Ask AI agent about system state (default if no subcommand)
    Query {
        /// Question to ask the AI agent
        question: Vec<String>,
    },
    /// Export tool manifests for AI agents (OpenAI, Claude, Gemini, etc.)
    Manifest {
        /// Output format: openai, anthropic, gemini, grok, llama, mistral, deepseek, jsonld, mcp, or json
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Start MCP (Model Context Protocol) server for Claude Desktop integration
    Server,
}

#[cfg(feature = "cli")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::agent::AgentConfig;

    env_logger::init();

    let cli = Cli::parse();

    // List backends if requested
    if cli.list_backends {
        println!("[*] Available AI Backends:\n");
        let backends = AgentConfig::list_available_backends();
        for (i, backend) in backends.iter().enumerate() {
            println!("{}. {}", i + 1, backend.display_name());
            if let Some(env_var) = backend.api_key_env_var() {
                let status = if std::env::var(env_var).is_ok() {
                    "[+] configured"
                } else {
                    "[-] not configured"
                };
                println!("   API Key: {} {}", env_var, status);
            }
            if let Some(endpoint) = backend.default_endpoint() {
                println!("   Endpoint: {}", endpoint);
            }
            println!();
        }
        return Ok(());
    }

    match cli.command {
        // Manifest export
        Some(AmonCommand::Manifest { format, output }) => {
            use ironmonlib::ai_api::{AgentManifest, ExportFormat};

            let manifest = AgentManifest::new();

            let export_format = match format.to_lowercase().as_str() {
                "openai" | "chatgpt" | "gpt" | "gpt4" | "o1" | "o3" => ExportFormat::OpenAI,
                "anthropic" | "claude" => ExportFormat::Anthropic,
                "gemini" | "google" => ExportFormat::Gemini,
                "grok" | "xai" => ExportFormat::Grok,
                "llama" | "meta" => ExportFormat::Llama,
                "mistral" | "mixtral" => ExportFormat::Mistral,
                "deepseek" | "r1" => ExportFormat::DeepSeek,
                "jsonld" | "json-ld" | "schema" => ExportFormat::JsonLd,
                "mcp" => ExportFormat::Mcp,
                "json" | "simple" => ExportFormat::SimpleJson,
                _ => {
                    eprintln!("Unknown format '{}'. Supported: openai, anthropic, gemini, grok, llama, mistral, deepseek, jsonld, mcp, json", format);
                    return Err("Invalid format".into());
                }
            };

            let exported = manifest.export(export_format);
            let json_output = serde_json::to_string_pretty(&exported)?;

            if let Some(path) = output {
                std::fs::write(&path, &json_output)?;
                eprintln!("[+] Manifest written to: {}", path.display());
            } else {
                println!("{}", json_output);
            }
        }

        // MCP Server
        Some(AmonCommand::Server) => {
            use ironmonlib::ai_api::McpServer;

            eprintln!("[*] Starting MCP (Model Context Protocol) server...");
            eprintln!("[*] Communicating via stdio (JSON-RPC 2.0)");
            eprintln!("[*] Ready for connections from Claude Desktop or other MCP clients");

            let mut server = McpServer::new()?;
            server.run_stdio()?;
        }

        // Query mode (explicit or default)
        Some(AmonCommand::Query { question }) => {
            let query = if question.is_empty() {
                None
            } else {
                Some(question.join(" "))
            };
            run_query_mode(query)?;
        }

        // No subcommand = interactive mode
        None => {
            run_query_mode(None)?;
        }
    }

    Ok(())
}

#[cfg(feature = "cli")]
fn run_query_mode(query: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    use ironmonlib::agent::{Agent, AgentConfig};
    use ironmonlib::UnifiedMonitor;
    use std::io::{self, Write};

    // Create monitor for system state
    let monitor = UnifiedMonitor::new()?;

    // Auto-detect and configure best available backend
    let config = match AgentConfig::auto_detect() {
        Ok(cfg) => {
            if let Some(ref backend) = cfg.backend {
                eprintln!("[*] Using backend: {}", backend.backend_type.display_name());
            }
            cfg
        }
        Err(e) => {
            eprintln!("[!] No AI backends available: {}", e);
            eprintln!("[!] To use AI features, install Ollama (https://ollama.com) or set an API key (OPENAI_API_KEY, GITHUB_TOKEN, etc.)");
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No AI backend configured",
            )));
        }
    }
    .with_caching(true)
    .with_cache_size(50)
    .with_timeout(Duration::from_secs(30));

    let mut agent = Agent::new(config)?;

    if let Some(question) = query {
        // Single query mode
        println!("[AI Monitor]");
        println!("Question: {}\n", question);

        let response = agent.ask(&question, &monitor)?;
        println!("{}", response.response);

        if response.from_cache {
            println!("\n[CACHE] (from cache, <1ms)");
        } else {
            println!("\n[TIME] ({}ms)", response.inference_time_ms);
        }
    } else {
        // Interactive mode
        println!("[AI Monitor - Interactive Mode]");
        println!("Ask questions about your system state. Type 'quit' or 'exit' to leave.\n");
        println!("Examples:");
        println!("  * What's my GPU temperature?");
        println!("  * Show me memory usage");
        println!("  * Is my CPU usage normal?");
        println!("  * How much power am I using?\n");

        loop {
            print!("You: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();

            if input.is_empty() {
                continue;
            }

            if input.eq_ignore_ascii_case("quit") || input.eq_ignore_ascii_case("exit") {
                println!("Goodbye!");
                break;
            }

            match agent.ask(input, &monitor) {
                Ok(response) => {
                    println!("\n[Agent]: {}\n", response.response);
                    if response.from_cache {
                        println!("[CACHE] (from cache, <1ms)\n");
                    } else {
                        println!("[TIME] ({}ms)\n", response.inference_time_ms);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}\n", e);
                }
            }
        }
    }

    Ok(())
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("CLI features not enabled. Please compile with --features cli");
    std::process::exit(1);
}
