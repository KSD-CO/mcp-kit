//! # mcp-kit-client — MCP Client SDK
//!
//! A Rust client library for connecting to MCP (Model Context Protocol) servers.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use mcp_kit_client::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Connect to an MCP server via stdio (subprocess)
//!     let client = McpClient::stdio("path/to/mcp-server")
//!         .await?;
//!
//!     // Initialize the connection
//!     let server_info = client.initialize("my-app", "1.0.0").await?;
//!     println!("Connected to: {}", server_info.name);
//!
//!     // List available tools
//!     let tools = client.list_tools().await?;
//!     for tool in tools {
//!         println!("Tool: {} - {}", tool.name, tool.description.unwrap_or_default());
//!     }
//!
//!     // Call a tool
//!     let result = client.call_tool("greet", serde_json::json!({
//!         "name": "World"
//!     })).await?;
//!     println!("Result: {:?}", result);
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Transports
//!
//! The client supports multiple transport mechanisms:
//!
//! - **Stdio**: Connect to a subprocess via stdin/stdout
//! - **SSE**: Connect via HTTP Server-Sent Events
//! - **WebSocket**: Connect via WebSocket
//!
//! ## Features
//!
//! - `stdio` - Enable stdio transport (default)
//! - `sse` - Enable SSE transport (default)
//! - `websocket` - Enable WebSocket transport (default)
//! - `full` - Enable all transports (default)

mod client;
mod error;
mod transport;

pub use client::{McpClient, McpClientBuilder};
pub use error::{ClientError, ClientResult};

// Re-export types from mcp-kit
pub use mcp_kit::{
    CallToolResult, Content, ImageContent, InitializeResult, ListPromptsResult,
    ListResourcesResult, ListToolsResult, Prompt, ReadResourceResult, Resource, TextContent, Tool,
};

/// Prelude for convenient imports
pub mod prelude {
    pub use crate::{ClientError, ClientResult, McpClient, McpClientBuilder};
    pub use mcp_kit::{
        CallToolResult, Content, ImageContent, InitializeResult, ListPromptsResult,
        ListResourcesResult, ListToolsResult, Prompt, ReadResourceResult, Resource, TextContent,
        Tool,
    };
    pub use serde_json;
}
