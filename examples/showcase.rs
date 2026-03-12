//! Comprehensive showcase of all rust-mcp features
//!
//! This example demonstrates:
//! - Multiple tools with different input/output types
//! - Static and template resources
//! - Prompts with and without arguments
//! - Error handling
//! - Async operations
//! - State management
//! - JSON content types
//!
//! Run with:
//!   cargo run --example showcase
//!   cargo run --example showcase -- --sse  # SSE transport on port 3000

use rust_mcp::prelude::*;
use rust_mcp::{prompt, resource, GetPromptRequest, ReadResourceRequest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

// ─── Shared State ────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    counter: Arc<Mutex<i32>>,
    notes: Arc<Mutex<Vec<String>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            counter: Arc::new(Mutex::new(0)),
            notes: Arc::new(Mutex::new(vec![
                "Welcome to rust-mcp showcase!".to_string(),
                "Try calling the tools and resources.".to_string(),
            ])),
        }
    }
}

// ─── Tool Examples with #[tool] Macro ────────────────────────────────────────

/// Simple math operation
#[tool(description = "Add two numbers together")]
async fn add(a: f64, b: f64) -> String {
    format!("Result: {}", a + b)
}

/// Tool with structured input
#[derive(Deserialize, JsonSchema)]
struct CalculateInput {
    /// The operation to perform
    operation: String,
    /// First operand
    x: f64,
    /// Second operand
    y: f64,
}

#[tool(description = "Perform various math operations (add, subtract, multiply, divide)")]
async fn calculate(params: CalculateInput) -> Result<CallToolResult, String> {
    let result = match params.operation.as_str() {
        "add" => params.x + params.y,
        "subtract" => params.x - params.y,
        "multiply" => params.x * params.y,
        "divide" => {
            if params.y == 0.0 {
                return Err("Cannot divide by zero".to_string());
            }
            params.x / params.y
        }
        op => return Err(format!("Unknown operation: {}", op)),
    };

    Ok(CallToolResult::text(format!(
        "{} {} {} = {}",
        params.x, params.operation, params.y, result
    )))
}

/// Tool that returns JSON content
#[derive(Serialize)]
struct SystemInfo {
    name: String,
    version: String,
    features: Vec<String>,
    uptime_seconds: u64,
}

#[tool(description = "Get system information as JSON")]
async fn system_info() -> CallToolResult {
    let info = SystemInfo {
        name: "rust-mcp-showcase".to_string(),
        version: "0.1.0".to_string(),
        features: vec![
            "tools".to_string(),
            "resources".to_string(),
            "prompts".to_string(),
            "macros".to_string(),
            "state".to_string(),
        ],
        uptime_seconds: 0, // Would track real uptime in production
    };

    CallToolResult::text(serde_json::to_string_pretty(&info).unwrap())
}

/// Async tool with simulated delay
#[tool(description = "Simulate a long-running async operation")]
async fn async_work(duration_ms: u64) -> String {
    tokio::time::sleep(tokio::time::Duration::from_millis(duration_ms)).await;
    format!("Completed after {}ms", duration_ms)
}

/// Tool with list input
#[derive(Deserialize, JsonSchema)]
struct SumListInput {
    /// List of numbers to sum
    numbers: Vec<f64>,
}

#[tool(description = "Sum a list of numbers")]
async fn sum_list(params: SumListInput) -> String {
    let sum: f64 = params.numbers.iter().sum();
    format!("Sum of {:?} = {}", params.numbers, sum)
}

// ─── Tool Examples with Manual API (without macros) ─────────────────────────

// Counter tool with shared state
async fn increment_counter(state: Arc<Mutex<i32>>) -> CallToolResult {
    let mut counter = state.lock().await;
    *counter += 1;
    CallToolResult::text(format!("Counter: {}", *counter))
}

async fn get_counter(state: Arc<Mutex<i32>>) -> CallToolResult {
    let counter = state.lock().await;
    CallToolResult::text(format!("Counter: {}", *counter))
}

async fn reset_counter(state: Arc<Mutex<i32>>) -> CallToolResult {
    let mut counter = state.lock().await;
    *counter = 0;
    CallToolResult::text("Counter reset to 0")
}

// Notes management
#[derive(Deserialize, JsonSchema)]
struct AddNoteInput {
    /// The note content to add
    note: String,
}

async fn add_note(notes: Arc<Mutex<Vec<String>>>, params: AddNoteInput) -> CallToolResult {
    let mut notes = notes.lock().await;
    notes.push(params.note.clone());
    CallToolResult::text(format!(
        "Added note: '{}'. Total notes: {}",
        params.note,
        notes.len()
    ))
}

async fn list_notes(notes: Arc<Mutex<Vec<String>>>) -> CallToolResult {
    let notes = notes.lock().await;
    let notes_text = notes
        .iter()
        .enumerate()
        .map(|(i, note)| format!("{}. {}", i + 1, note))
        .collect::<Vec<_>>()
        .join("\n");
    CallToolResult::text(format!("Notes ({}):\n{}", notes.len(), notes_text))
}

// ─── Resource Examples with #[resource] Macro ────────────────────────────────

/// Static resource - server configuration
#[resource(
    uri = "config://server",
    name = "Server Configuration",
    description = "Current server configuration and settings",
    mime_type = "application/json"
)]
async fn server_config(_req: ReadResourceRequest) -> McpResult<ReadResourceResult> {
    let config = serde_json::json!({
        "server": {
            "name": "rust-mcp-showcase",
            "version": "0.1.0",
            "protocol_version": "2024-11-05"
        },
        "features": {
            "tools": true,
            "resources": true,
            "prompts": true,
            "logging": false
        },
        "limits": {
            "max_connections": 100,
            "request_timeout_ms": 30000
        }
    });

    Ok(ReadResourceResult::text(
        "config://server",
        serde_json::to_string_pretty(&config).unwrap(),
    ))
}

/// Static resource - API documentation
#[resource(
    uri = "docs://api",
    name = "API Documentation",
    description = "Complete API reference for all tools and resources"
)]
async fn api_docs(_req: ReadResourceRequest) -> McpResult<ReadResourceResult> {
    let docs = r#"# rust-mcp Showcase API

## Tools

### Math Operations
- `add(a, b)` - Add two numbers
- `calculate(operation, x, y)` - Perform math operations (add, subtract, multiply, divide)
- `sum_list(numbers)` - Sum a list of numbers

### System
- `system_info()` - Get system information as JSON
- `async_work(duration_ms)` - Simulate async operation

### State Management
- `increment_counter()` - Increment the shared counter
- `get_counter()` - Get current counter value
- `reset_counter()` - Reset counter to 0
- `add_note(note)` - Add a note to the list
- `list_notes()` - List all notes

## Resources

- `config://server` - Server configuration
- `docs://api` - This API documentation
- `file://{path}` - Read any text file from filesystem
- `data://json/{key}` - Access JSON data by key

## Prompts

- `code-review` - Generate code review prompt
- `explain-concept` - Generate explanation prompt
- `debug-help` - Generate debugging assistance prompt
"#;

    Ok(ReadResourceResult::text("docs://api", docs))
}

/// Template resource - file system access
#[resource(
    uri = "file://{path}",
    name = "File System",
    description = "Read text files from the local filesystem"
)]
async fn read_file(req: ReadResourceRequest) -> McpResult<ReadResourceResult> {
    let path = req.uri.trim_start_matches("file://");

    match tokio::fs::read_to_string(path).await {
        Ok(content) => Ok(ReadResourceResult::text(req.uri.clone(), content)),
        Err(e) => Err(McpError::ResourceNotFound(format!(
            "Could not read file '{}': {}",
            path, e
        ))),
    }
}

/// Template resource - dynamic JSON data
#[resource(
    uri = "data://json/{key}",
    name = "JSON Data Store",
    description = "Access predefined JSON data by key"
)]
async fn json_data(req: ReadResourceRequest) -> McpResult<ReadResourceResult> {
    let key = req.uri.trim_start_matches("data://json/");

    let data = match key {
        "example" => serde_json::json!({
            "id": 1,
            "name": "Example Data",
            "items": ["item1", "item2", "item3"]
        }),
        "config" => serde_json::json!({
            "debug": false,
            "port": 3000,
            "host": "localhost"
        }),
        "stats" => serde_json::json!({
            "requests": 42,
            "errors": 0,
            "uptime": "1h 23m"
        }),
        _ => {
            return Err(McpError::ResourceNotFound(format!(
                "Unknown data key: {}",
                key
            )))
        }
    };

    Ok(ReadResourceResult::text(
        req.uri.clone(),
        serde_json::to_string_pretty(&data).unwrap(),
    ))
}

// ─── Prompt Examples with #[prompt] Macro ────────────────────────────────────

/// Code review prompt
#[prompt(
    name = "code-review",
    description = "Generate a comprehensive code review prompt",
    arguments = ["code:required", "language:optional", "focus:optional"]
)]
async fn code_review_prompt(req: GetPromptRequest) -> McpResult<GetPromptResult> {
    let code = req.arguments.get("code").cloned().unwrap_or_default();
    let language = req
        .arguments
        .get("language")
        .cloned()
        .unwrap_or_else(|| "unknown".into());
    let focus = req
        .arguments
        .get("focus")
        .cloned()
        .unwrap_or_else(|| "general quality".into());

    let prompt_text = format!(
        r#"Please review the following {language} code with a focus on {focus}:

```{language}
{code}
```

Provide detailed feedback on:
1. Code quality and best practices
2. Potential bugs or issues
3. Performance considerations
4. Security concerns
5. Suggestions for improvement
6. Testing recommendations

Be specific and provide examples where applicable."#
    );

    Ok(GetPromptResult::new(vec![PromptMessage::user_text(
        prompt_text,
    )]))
}

/// Concept explanation prompt
#[prompt(
    name = "explain-concept",
    description = "Generate a prompt to explain a technical concept",
    arguments = ["concept:required", "level:optional"]
)]
async fn explain_concept_prompt(req: GetPromptRequest) -> McpResult<GetPromptResult> {
    let concept = req.arguments.get("concept").cloned().unwrap_or_default();
    let level = req
        .arguments
        .get("level")
        .cloned()
        .unwrap_or_else(|| "intermediate".into());

    let prompt_text = format!(
        r#"Please explain the concept of "{concept}" at a {level} level.

Include:
1. A clear definition
2. Why it matters
3. Common use cases
4. Simple examples
5. Related concepts
6. Common misconceptions

Use analogies where helpful and keep the explanation engaging."#
    );

    Ok(GetPromptResult::new(vec![PromptMessage::user_text(
        prompt_text,
    )]))
}

/// Debug assistance prompt
#[prompt(
    name = "debug-help",
    description = "Generate a debugging assistance prompt",
    arguments = ["error:required", "context:optional"]
)]
async fn debug_help_prompt(req: GetPromptRequest) -> McpResult<GetPromptResult> {
    let error = req.arguments.get("error").cloned().unwrap_or_default();
    let context = req
        .arguments
        .get("context")
        .cloned()
        .unwrap_or_else(|| "No additional context provided".into());

    let prompt_text = format!(
        r#"I'm encountering the following error:

```
{error}
```

Context:
{context}

Please help me debug this issue by:
1. Explaining what this error means
2. Identifying potential root causes
3. Suggesting step-by-step debugging strategies
4. Providing code examples of fixes
5. Recommending preventive measures

Be thorough and consider edge cases."#
    );

    Ok(GetPromptResult::new(vec![PromptMessage::user_text(
        prompt_text,
    )]))
}

// ─── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("showcase=info,rust_mcp=debug")
        .init();

    tracing::info!("Starting rust-mcp comprehensive showcase");

    // Create shared state
    let state = AppState::new();

    // Build server with all features
    let server = McpServer::builder()
        .name("rust-mcp-showcase")
        .version("0.1.0")
        .instructions(
            "Comprehensive showcase of rust-mcp features. \
             Includes tools, resources, prompts, state management, and error handling. \
             Try all the tools and resources to explore the capabilities!",
        )
        // Macro-based tools
        .tool_def(add_tool_def())
        .tool_def(calculate_tool_def())
        .tool_def(system_info_tool_def())
        .tool_def(async_work_tool_def())
        .tool_def(sum_list_tool_def())
        // Manual API tools with state
        .tool(
            Tool::new(
                "increment_counter",
                "Increment the shared counter",
                serde_json::json!({"type": "object"}),
            ),
            {
                let state = state.counter.clone();
                move |_: serde_json::Value| {
                    let state = state.clone();
                    async move { increment_counter(state).await }
                }
            },
        )
        .tool(
            Tool::new(
                "get_counter",
                "Get the current counter value",
                serde_json::json!({"type": "object"}),
            ),
            {
                let state = state.counter.clone();
                move |_: serde_json::Value| {
                    let state = state.clone();
                    async move { get_counter(state).await }
                }
            },
        )
        .tool(
            Tool::new(
                "reset_counter",
                "Reset the counter to 0",
                serde_json::json!({"type": "object"}),
            ),
            {
                let state = state.counter.clone();
                move |_: serde_json::Value| {
                    let state = state.clone();
                    async move { reset_counter(state).await }
                }
            },
        )
        .tool(
            Tool::new(
                "add_note",
                "Add a note to the shared list",
                serde_json::to_value(schemars::schema_for!(AddNoteInput))?,
            ),
            {
                let notes = state.notes.clone();
                move |params: AddNoteInput| {
                    let notes = notes.clone();
                    async move { add_note(notes, params).await }
                }
            },
        )
        .tool(
            Tool::new(
                "list_notes",
                "List all notes",
                serde_json::json!({"type": "object"}),
            ),
            {
                let notes = state.notes.clone();
                move |_: serde_json::Value| {
                    let notes = notes.clone();
                    async move { list_notes(notes).await }
                }
            },
        )
        // Resources
        .resource_def(server_config_resource_def())
        .resource_def(api_docs_resource_def())
        .resource_def(read_file_resource_def())
        .resource_def(json_data_resource_def())
        // Prompts
        .prompt_def(code_review_prompt_prompt_def())
        .prompt_def(explain_concept_prompt_prompt_def())
        .prompt_def(debug_help_prompt_prompt_def())
        .build();

    // Check for --sse flag
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--sse".to_string()) {
        tracing::info!("Starting SSE transport on http://localhost:3000");
        server.serve_sse(([0, 0, 0, 0], 3000)).await?;
    } else {
        tracing::info!("Starting stdio transport");
        server.serve_stdio().await?;
    }

    Ok(())
}
