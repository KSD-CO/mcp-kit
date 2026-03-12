# mcp-kit-macros

Procedural macros for the [`mcp-kit`](https://crates.io/crates/mcp-kit) crate.

This crate provides attribute macros that simplify building MCP (Model Context Protocol) servers by automatically generating boilerplate code and JSON schemas.

## Macros

### `#[tool]`

Generate tools from async functions with automatic schema generation:

```rust
use mcp_kit::prelude::*;

#[tool(description = "Add two numbers")]
async fn add(a: f64, b: f64) -> String {
    format!("{}", a + b)
}

// Use the generated function
McpServer::builder()
    .tool_def(add_tool_def())
    .build()
```

### `#[resource]`

Generate resource handlers for static or template URIs:

```rust
use mcp_kit::{resource, ReadResourceRequest, McpResult, ReadResourceResult};

#[resource(uri = "config://app", name = "App Config")]
async fn config(_req: ReadResourceRequest) -> McpResult<ReadResourceResult> {
    Ok(ReadResourceResult::text("config://app", "{}"))
}

// Template resource with {variables}
#[resource(uri = "file://{path}", name = "File System")]
async fn read_file(req: ReadResourceRequest) -> McpResult<ReadResourceResult> {
    // Implementation
}
```

### `#[prompt]`

Generate prompt handlers with optional arguments:

```rust
use mcp_kit::{prompt, GetPromptRequest, McpResult, GetPromptResult, PromptMessage};

#[prompt(
    name = "greeting",
    description = "A friendly greeting",
    arguments = ["name:optional"]
)]
async fn hello(req: GetPromptRequest) -> McpResult<GetPromptResult> {
    let name = req.arguments.get("name").cloned().unwrap_or("there".into());
    Ok(GetPromptResult::new(vec![
        PromptMessage::user_text(format!("Hello, {}!", name))
    ]))
}
```

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
mcp-kit = "0.1"
```

The macros are automatically re-exported by `mcp-kit`, so you don't need to add this crate as a direct dependency.

## Documentation

See the main [`mcp-kit`](https://docs.rs/mcp-kit) documentation for complete usage guides and examples.

## License

MIT
