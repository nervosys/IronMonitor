//! Example: AI Data API Demo
//!
//! This example demonstrates how to use the AI Data API to provide
//! full system visibility to AI systems.
//!
//! Run with:
//! ```sh
//! cargo run --release --features nvidia --example ai_api_demo
//! ```

use ironmonlib::ai_api::{AiDataApi, ToolCategory, ToolResult};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== AI Data API Demo ===\n");

    // Create the API
    let mut api = AiDataApi::new()?;

    // List all available tools
    println!("Available Tools by Category:");
    println!("============================\n");

    for category in [
        ToolCategory::System,
        ToolCategory::Gpu,
        ToolCategory::Cpu,
        ToolCategory::Memory,
        ToolCategory::Disk,
        ToolCategory::Network,
        ToolCategory::Process,
        ToolCategory::Hardware,
    ] {
        let tools = api.list_tools_by_category(category);
        println!("{} Tools ({}):", category, tools.len());
        for tool in tools {
            println!("  • {} - {}", tool.name, tool.description);
        }
        println!();
    }

    // Get system summary
    println!("\n=== System Summary ===\n");
    let result = api.call_tool("get_system_summary", serde_json::json!({}))?;
    print_result(&result);

    // Get GPU status
    println!("\n=== GPU Status ===\n");
    let result = api.call_tool("get_gpu_status", serde_json::json!({}))?;
    print_result(&result);

    // Get top CPU processes
    println!("\n=== Top 5 CPU Processes ===\n");
    let result = api.call_tool("get_top_cpu_processes", serde_json::json!({"count": 5}))?;
    print_result(&result);

    // Get top Memory processes
    println!("\n=== Top 5 Memory Processes ===\n");
    let result = api.call_tool("get_top_memory_processes", serde_json::json!({"count": 5}))?;
    print_result(&result);

    // Get disk list
    println!("\n=== Disk List ===\n");
    let result = api.call_tool("get_disk_list", serde_json::json!({}))?;
    print_result(&result);

    // Get network interfaces
    println!("\n=== Active Network Interfaces ===\n");
    let result = api.call_tool("get_network_interfaces", serde_json::json!({}))?;
    print_result(&result);

    // Show how to get tools in OpenAI format
    println!("\n=== OpenAI Function Calling Format (first 2) ===\n");
    let openai_tools = api.tools_as_openai_functions();
    for tool in openai_tools.iter().take(2) {
        println!("{}", serde_json::to_string_pretty(tool)?);
        println!();
    }

    // Show how to get tools in Anthropic format
    println!("\n=== Anthropic Tools Format (first 2) ===\n");
    let anthropic_tools = api.tools_as_anthropic_tools();
    for tool in anthropic_tools.iter().take(2) {
        println!("{}", serde_json::to_string_pretty(tool)?);
        println!();
    }

    // Show prompt format
    println!("\n=== Prompt Format (truncated) ===\n");
    let prompt = api.tools_as_prompt();
    let lines: Vec<&str> = prompt.lines().take(40).collect();
    println!("{}", lines.join("\n"));
    println!("... (truncated)");

    Ok(())
}

fn print_result(result: &ToolResult) {
    println!("Tool: {}", result.tool_name);
    println!("Success: {}", result.success);
    println!("Execution time: {}ms", result.execution_time_ms);

    if let Some(ref data) = result.data {
        println!("Data:");
        // Pretty print JSON, truncated
        let json_str = serde_json::to_string_pretty(data).unwrap_or_default();
        let lines: Vec<&str> = json_str.lines().take(30).collect();
        println!("{}", lines.join("\n"));
        if json_str.lines().count() > 30 {
            println!("  ... (truncated)");
        }
    }

    if let Some(ref error) = result.error {
        println!("Error: {}", error);
    }
}
