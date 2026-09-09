//! AI Chatbot Integration Test
//!
//! This example demonstrates how the AI Data API automatically
//! detects relevant tools based on user queries and provides
//! context for the AI agent.
//!
//! Run with:
//! ```bash
//! cargo run --release --features nvidia --example ai_chatbot_test
//! ```

use ironmonlib::ai_api::AiDataApi;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== AI Data API Chatbot Integration Test ===\n");

    // Create the AI Data API
    let mut api = AiDataApi::new()?;

    // Test queries that a user might ask in the chatbot
    let test_queries = vec![
        "What GPUs do I have?",
        "How much RAM is being used?",
        "What processes are using the most CPU?",
        "Show me my disk information",
        "What's my network bandwidth?",
        "What are my system temperatures?",
        "Give me a system overview",
        "Which applications are using my GPU?",
    ];

    for query in test_queries {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("User Query: \"{}\"", query);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        let context = api.auto_query(query);

        // Show a truncated version of the context
        let lines: Vec<&str> = context.lines().take(30).collect();
        println!("{}", lines.join("\n"));

        if context.lines().count() > 30 {
            println!(
                "\n... [truncated for display - full context has {} lines]",
                context.lines().count()
            );
        }

        println!("\n");
    }

    println!("=== Integration Test Complete ===");
    println!("\nThe auto_query() function automatically:");
    println!("  1. Analyzes the user's question");
    println!("  2. Determines which monitoring tools are relevant");
    println!("  3. Calls those tools and collects the results");
    println!("  4. Formats everything as context for the AI agent");
    println!("\nThis context is then injected into the agent's prompt,");
    println!("giving it real-time system data to answer questions accurately.");

    Ok(())
}
