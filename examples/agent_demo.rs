//! AI Agent Example
//!
//! This example demonstrates the lightweight AI agent that can answer
//! questions about system state, make predictions, and perform calculations.
//!
//! Run with:
//! ```bash
//! cargo run --release --features full --example agent_demo
//! ```

use ironmonlib::agent::{Agent, AgentConfig, ModelSize};
use ironmonlib::UnifiedMonitor;
use std::error::Error;
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn Error>> {
    println!("═══════════════════════════════════════════════════════════════");
    println!("     IronMonitor AI Agent Demo");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Initialize system monitor
    println!("Initializing hardware monitor...");
    let monitor = UnifiedMonitor::new()?;
    println!("✓ Detected {} GPU(s)\n", monitor.gpu_count());

    // Create agent with different model sizes
    println!("Available Model Sizes:");
    println!(
        "  1. Small (100M)  - {}ms latency, {}MB memory",
        ModelSize::Small.latency_estimate_ms(),
        ModelSize::Small.memory_mb()
    );
    println!(
        "  2. Medium (500M) - {}ms latency, {}MB memory (default)",
        ModelSize::Medium.latency_estimate_ms(),
        ModelSize::Medium.memory_mb()
    );
    println!(
        "  3. Large (1B)    - {}ms latency, {}MB memory",
        ModelSize::Large.latency_estimate_ms(),
        ModelSize::Large.memory_mb()
    );

    print!("\nSelect model size (1-3, default=2): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let model_size = match input.trim() {
        "1" => ModelSize::Small,
        "3" => ModelSize::Large,
        _ => ModelSize::Medium,
    };

    println!("\nInitializing agent with {} model...", model_size);
    let config = AgentConfig::new(model_size);
    let mut agent = Agent::new(config)?;
    println!("✓ Agent ready\n");

    // Demo queries
    println!("═══════════════════════════════════════════════════════════════");
    println!("Demo Queries (non-interactive)");
    println!("═══════════════════════════════════════════════════════════════\n");

    let demo_queries = vec![
        "What's my GPU temperature?",
        "Show GPU utilization",
        "How much power am I using?",
        "Calculate average GPU temperature",
        "Is my GPU temperature safe?",
        "Show memory usage",
        "Compare all GPUs",
        "What's the status of GPU 0?",
        "Total power consumption?",
        "Should I improve cooling?",
    ];

    for (i, query) in demo_queries.iter().enumerate() {
        println!("─────────────────────────────────────────────────────────────");
        println!("Query {}: \"{}\"", i + 1, query);
        println!("─────────────────────────────────────────────────────────────");

        let response = agent.ask(query, &monitor)?;

        println!("Type: {}", response.query_type);
        println!("Inference Time: {}ms", response.inference_time_ms);
        println!("Cached: {}", if response.from_cache { "Yes" } else { "No" });
        println!("\nAgent Response:");
        println!("{}\n", response.response);

        // Small delay for readability
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Cache statistics
    let (cache_used, cache_capacity) = agent.cache_stats();
    println!("═══════════════════════════════════════════════════════════════");
    println!(
        "Cache Statistics: {}/{} entries",
        cache_used, cache_capacity
    );
    println!("═══════════════════════════════════════════════════════════════\n");

    // Interactive mode
    println!("═══════════════════════════════════════════════════════════════");
    println!("Interactive Mode (type 'quit' or 'exit' to end)");
    println!("═══════════════════════════════════════════════════════════════\n");

    loop {
        print!("You: ");
        io::stdout().flush()?;

        let mut query = String::new();
        io::stdin().read_line(&mut query)?;

        let query = query.trim();

        if query.is_empty() {
            continue;
        }

        if query.eq_ignore_ascii_case("quit")
            || query.eq_ignore_ascii_case("exit")
            || query.eq_ignore_ascii_case("q")
        {
            println!("\nGoodbye!");
            break;
        }

        if query.eq_ignore_ascii_case("help") || query == "?" {
            println!("\nAgent: I can help you with:");
            println!("  • System State: 'What's my GPU temperature?', 'Show memory usage'");
            println!("  • Energy: 'How much power?', 'Cost per hour?'");
            println!("  • Comparisons: 'Compare GPU 0 vs GPU 1'");
            println!("  • Recommendations: 'Should I improve cooling?'");
            println!("  • Calculations: 'Calculate average utilization'\n");
            continue;
        }

        if query.eq_ignore_ascii_case("clear cache") {
            agent.clear_cache();
            println!("\nAgent: Cache cleared.\n");
            continue;
        }

        if query.eq_ignore_ascii_case("stats") {
            let (used, cap) = agent.cache_stats();
            println!("\nAgent Statistics:");
            println!("  Model: {}", agent.config().model_size);
            println!("  Cache: {}/{} entries", used, cap);
            println!("  Temperature: {}", agent.config().temperature);
            println!("  Max Tokens: {}\n", agent.config().max_response_tokens);
            continue;
        }

        // Get fresh monitor state
        let current_monitor = UnifiedMonitor::new()?;

        let response = agent.ask(query, &current_monitor)?;

        println!(
            "\nAgent [{}ms{}]: {}",
            response.inference_time_ms,
            if response.from_cache { ", cached" } else { "" },
            response.response
        );
        println!();
    }

    Ok(())
}
