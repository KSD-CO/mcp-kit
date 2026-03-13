//! Example: Connect to an MCP server using the client SDK.
//!
//! This example demonstrates how to use the MCP client to:
//! 1. Connect to a server via WebSocket
//! 2. Initialize the connection
//! 3. List available tools
//! 4. Call a tool
//!
//! Run the websocket example first:
//! ```bash
//! cargo run --example websocket
//! ```
//!
//! Then run this client:
//! ```bash
//! cargo run --example client_demo
//! ```

use mcp_kit_client::prelude::*;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("mcp_kit_client=debug".parse()?),
        )
        .with_writer(std::io::stderr)
        .init();

    println!("🔌 Connecting to MCP server via WebSocket...");

    // Connect to the WebSocket server
    let client = McpClient::websocket("ws://localhost:3001/ws").await?;

    println!("📡 Initializing connection...");

    // Initialize the connection
    let server_info = client.initialize("client-demo", "1.0.0").await?;
    println!(
        "✅ Connected to: {} v{}",
        server_info.name, server_info.version
    );

    // List available tools
    println!("\n📋 Available tools:");
    let tools = client.list_tools().await?;
    for tool in &tools {
        println!(
            "  - {} : {}",
            tool.name,
            tool.description.as_deref().unwrap_or("(no description)")
        );
    }

    // Call the calculate tool
    if tools.iter().any(|t| t.name == "calculate") {
        println!("\n🧮 Calling 'calculate' tool...");

        let result = client
            .call_tool(
                "calculate",
                serde_json::json!({
                    "a": 42.0,
                    "b": 8.0,
                    "op": "mul"
                }),
            )
            .await?;

        println!("Result: {:?}", result);

        // Try division
        let result = client
            .call_tool(
                "calculate",
                serde_json::json!({
                    "a": 100.0,
                    "b": 4.0,
                    "op": "div"
                }),
            )
            .await?;

        println!("Result: {:?}", result);
    }

    // Close the connection
    println!("\n👋 Closing connection...");
    client.close().await?;

    println!("✅ Done!");

    Ok(())
}
