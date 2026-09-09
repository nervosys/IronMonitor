//! Simple AI Agent Example
//!
//! Quick demonstration of the AI agent without interactive input.
//!
//! Run with:
//! ```bash
//! cargo run --release --features full --example agent_simple
//! ```

use ironmonlib::agent::{Agent, AgentConfig, ModelSize};
use ironmonlib::UnifiedMonitor;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("IronMonitor AI Agent - Simple Demo\n");

    // Initialize
    println!("Initializing...");
    let monitor = UnifiedMonitor::new()?;
    let config = AgentConfig::new(ModelSize::Medium);
    let mut agent = Agent::new(config)?;
    println!("✓ Ready ({} GPUs detected)\n", monitor.gpu_count());

    // Example queries
    let queries = vec![
        "What's my GPU temperature?",
        "Show GPU utilization",
        "How much power am I using?",
    ];

    for query in queries {
        println!("Q: {}", query);
        let response = agent.ask(query, &monitor)?;
        println!("A: {}", response.response);
        println!(
            "   [{}ms, type: {}]\n",
            response.inference_time_ms, response.query_type
        );
    }

    // Show cache stats
    let (used, cap) = agent.cache_stats();
    println!("Cache: {}/{} entries", used, cap);

    Ok(())
}
