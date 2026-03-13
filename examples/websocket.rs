//! WebSocket transport example.
//!
//! Run with: `cargo run --example websocket`
//! Test with: Connect via WebSocket client to ws://localhost:3001/ws

use mcp_kit::prelude::*;
use schemars::JsonSchema;
use serde::Deserialize;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Deserialize, JsonSchema)]
struct CalculatorInput {
    /// First number
    a: f64,
    /// Second number
    b: f64,
    /// Operation: "add", "sub", "mul", "div"
    op: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("mcp_kit=debug".parse()?))
        .with_writer(std::io::stderr)
        .init();

    let schema = serde_json::to_value(schemars::schema_for!(CalculatorInput))?;

    let server = McpServer::builder()
        .name("websocket-calculator")
        .version("1.0.0")
        .instructions("A simple calculator server over WebSocket")
        .tool(
            Tool::new("calculate", "Perform basic arithmetic operations", schema),
            |input: CalculatorInput| async move {
                let result = match input.op.as_str() {
                    "add" => input.a + input.b,
                    "sub" => input.a - input.b,
                    "mul" => input.a * input.b,
                    "div" => {
                        if input.b == 0.0 {
                            return CallToolResult::text("Error: Division by zero");
                        }
                        input.a / input.b
                    }
                    _ => {
                        return CallToolResult::text(format!(
                            "Error: Unknown operation '{}'",
                            input.op
                        ))
                    }
                };
                CallToolResult::text(format!("{} {} {} = {}", input.a, input.op, input.b, result))
            },
        )
        .build();

    println!("🚀 WebSocket MCP Server running on ws://localhost:3001/ws");
    println!("   Connect with any WebSocket client");

    server
        .serve_websocket("0.0.0.0:3001".parse::<std::net::SocketAddr>()?)
        .await?;

    Ok(())
}
