//! MCP Server on Cloudflare Workers — Complete Example
//!
//! This is a production-ready template demonstrating all MCP capabilities:
//! - **Tools**: Calculator operations, text utilities
//! - **Resources**: Static config, dynamic data
//! - **Resource Templates**: User profiles, documents by ID
//! - **Prompts**: Code review, summarization, translation
//!
//! ## Transport
//!
//! - `POST /mcp` — JSON-RPC endpoint for all MCP requests
//! - `GET  /mcp` — Server metadata and discovery
//!
//! ## Build & Deploy
//!
//! ```bash
//! cd deploy/cloudflare
//! npx wrangler deploy
//! ```
//!
//! ## Test Locally
//!
//! ```bash
//! npx wrangler dev
//!
//! # Initialize
//! curl -X POST http://localhost:8787/mcp \
//!   -H "Content-Type: application/json" \
//!   -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}'
//!
//! # List tools
//! curl -X POST http://localhost:8787/mcp \
//!   -H "Content-Type: application/json" \
//!   -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
//!
//! # Call a tool
//! curl -X POST http://localhost:8787/mcp \
//!   -H "Content-Type: application/json" \
//!   -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add","arguments":{"a":5,"b":3}}}'
//! ```

use std::{collections::HashMap, pin::Pin, rc::Rc};

use futures::Future;
use mcp_kit::{
    error::{McpError, McpResult},
    protocol::{
        JsonRpcError, JsonRpcMessage, JsonRpcRequest, JsonRpcResponse, MCP_PROTOCOL_VERSION,
    },
    types::{
        content::Content,
        messages::{
            CallToolRequest, GetPromptRequest, InitializeRequest, InitializeResult,
            ReadResourceRequest,
        },
        prompt::{GetPromptResult, Prompt, PromptArgument, PromptMessage},
        resource::{ReadResourceResult, Resource, ResourceContents, ResourceTemplate},
        tool::{CallToolResult, Tool},
        LoggingCapability, PromptsCapability, ResourcesCapability, ServerCapabilities, ServerInfo,
        ToolsCapability,
    },
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use worker::*;

// ─── Type Aliases ─────────────────────────────────────────────────────────────

type LocalFuture<T> = Pin<Box<dyn Future<Output = T>>>;
type ToolFn = Rc<dyn Fn(Value) -> LocalFuture<McpResult<CallToolResult>>>;
type ResourceFn = Rc<dyn Fn(ReadResourceRequest) -> LocalFuture<McpResult<ReadResourceResult>>>;
type PromptFn = Rc<dyn Fn(GetPromptRequest) -> LocalFuture<McpResult<GetPromptResult>>>;

// ─── Registry Entries ─────────────────────────────────────────────────────────

struct ToolEntry {
    tool: Tool,
    handler: ToolFn,
}

struct ResourceEntry {
    resource: Resource,
    handler: ResourceFn,
}

struct ResourceTemplateEntry {
    template: ResourceTemplate,
    handler: ResourceFn,
}

struct PromptEntry {
    prompt: Prompt,
    handler: PromptFn,
}

// ─── CloudflareServer ─────────────────────────────────────────────────────────

/// Stateless MCP server for Cloudflare Workers.
///
/// Each request is self-contained. Session state is not persisted across
/// requests, which works well for the common "initialize → call tools" pattern.
pub struct CloudflareServer {
    info: ServerInfo,
    instructions: Option<String>,
    tools: HashMap<String, ToolEntry>,
    resources: HashMap<String, ResourceEntry>,
    resource_templates: Vec<ResourceTemplateEntry>,
    prompts: HashMap<String, PromptEntry>,
}

impl CloudflareServer {
    pub fn builder() -> CloudflareServerBuilder {
        CloudflareServerBuilder::default()
    }

    async fn handle(&self, msg: JsonRpcMessage) -> Option<JsonRpcMessage> {
        match msg {
            JsonRpcMessage::Request(req) => {
                let id = req.id.clone();
                match self.dispatch(req).await {
                    Ok(result) => Some(JsonRpcMessage::Response(JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id,
                        result,
                    })),
                    Err(e) => Some(JsonRpcMessage::Error(JsonRpcError::new(id, e))),
                }
            }
            JsonRpcMessage::Notification(_) => None,
            _ => None,
        }
    }

    async fn dispatch(&self, req: JsonRpcRequest) -> McpResult<Value> {
        let params = req.params.unwrap_or(Value::Null);

        match req.method.as_str() {
            // ── Lifecycle ─────────────────────────────────────────────────────
            "initialize" => {
                let _init: InitializeRequest = serde_json::from_value(params)
                    .map_err(|e| McpError::InvalidParams(e.to_string()))?;

                let result = InitializeResult {
                    protocol_version: MCP_PROTOCOL_VERSION.to_owned(),
                    capabilities: ServerCapabilities {
                        tools: if self.tools.is_empty() {
                            None
                        } else {
                            Some(ToolsCapability {
                                list_changed: Some(false),
                            })
                        },
                        resources: if self.resources.is_empty()
                            && self.resource_templates.is_empty()
                        {
                            None
                        } else {
                            Some(ResourcesCapability {
                                subscribe: Some(false),
                                list_changed: Some(false),
                            })
                        },
                        prompts: if self.prompts.is_empty() {
                            None
                        } else {
                            Some(PromptsCapability {
                                list_changed: Some(false),
                            })
                        },
                        logging: Some(LoggingCapability {}),
                        experimental: None,
                    },
                    server_info: self.info.clone(),
                    instructions: self.instructions.clone(),
                };
                Ok(serde_json::to_value(result)?)
            }

            "ping" => Ok(serde_json::json!({})),

            // ── Tools ─────────────────────────────────────────────────────────
            "tools/list" => {
                let tools: Vec<&Tool> = self.tools.values().map(|e| &e.tool).collect();
                Ok(serde_json::json!({ "tools": tools }))
            }

            "tools/call" => {
                let req: CallToolRequest = serde_json::from_value(params)
                    .map_err(|e| McpError::InvalidParams(e.to_string()))?;
                let entry = self
                    .tools
                    .get(&req.name)
                    .ok_or_else(|| McpError::ToolNotFound(req.name.clone()))?;
                let result = (entry.handler)(req.arguments).await?;
                Ok(serde_json::to_value(result)?)
            }

            // ── Resources ─────────────────────────────────────────────────────
            "resources/list" => {
                let resources: Vec<&Resource> =
                    self.resources.values().map(|e| &e.resource).collect();
                Ok(serde_json::json!({ "resources": resources }))
            }

            "resources/templates/list" => {
                let templates: Vec<&ResourceTemplate> =
                    self.resource_templates.iter().map(|e| &e.template).collect();
                Ok(serde_json::json!({ "resourceTemplates": templates }))
            }

            "resources/read" => {
                let req: ReadResourceRequest = serde_json::from_value(params)
                    .map_err(|e| McpError::InvalidParams(e.to_string()))?;

                // Exact URI match first
                if let Some(entry) = self.resources.get(&req.uri) {
                    let result = (entry.handler)(req).await?;
                    return Ok(serde_json::to_value(result)?);
                }

                // Template match
                for entry in &self.resource_templates {
                    if uri_matches_template(&req.uri, &entry.template.uri_template) {
                        let result = (entry.handler)(req).await?;
                        return Ok(serde_json::to_value(result)?);
                    }
                }

                Err(McpError::ResourceNotFound(req.uri))
            }

            // ── Prompts ───────────────────────────────────────────────────────
            "prompts/list" => {
                let prompts: Vec<&Prompt> = self.prompts.values().map(|e| &e.prompt).collect();
                Ok(serde_json::json!({ "prompts": prompts }))
            }

            "prompts/get" => {
                let req: GetPromptRequest = serde_json::from_value(params)
                    .map_err(|e| McpError::InvalidParams(e.to_string()))?;
                let entry = self
                    .prompts
                    .get(&req.name)
                    .ok_or_else(|| McpError::PromptNotFound(req.name.clone()))?;
                let result = (entry.handler)(req).await?;
                Ok(serde_json::to_value(result)?)
            }

            method => Err(McpError::MethodNotFound(method.to_owned())),
        }
    }
}

// ─── Builder ─────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct CloudflareServerBuilder {
    name: String,
    version: String,
    instructions: Option<String>,
    tools: HashMap<String, ToolEntry>,
    resources: HashMap<String, ResourceEntry>,
    resource_templates: Vec<ResourceTemplateEntry>,
    prompts: HashMap<String, PromptEntry>,
}

impl CloudflareServerBuilder {
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    // ── Tools ─────────────────────────────────────────────────────────────────

    /// Register a tool with typed parameters (auto-deserialized from JSON).
    pub fn tool<T, F, Fut>(mut self, tool: Tool, handler: F) -> Self
    where
        T: DeserializeOwned + 'static,
        F: Fn(T) -> Fut + 'static,
        Fut: Future<Output = McpResult<CallToolResult>> + 'static,
    {
        let f = Rc::new(move |args: Value| -> LocalFuture<McpResult<CallToolResult>> {
            let params: T = match serde_json::from_value(args) {
                Ok(p) => p,
                Err(e) => {
                    return Box::pin(async move { Err(McpError::InvalidParams(e.to_string())) })
                }
            };
            Box::pin(handler(params))
        });
        self.tools
            .insert(tool.name.clone(), ToolEntry { tool, handler: f });
        self
    }

    /// Register a tool with raw JSON handler.
    pub fn tool_raw<F, Fut>(mut self, tool: Tool, handler: F) -> Self
    where
        F: Fn(Value) -> Fut + 'static,
        Fut: Future<Output = McpResult<CallToolResult>> + 'static,
    {
        let f = Rc::new(move |args: Value| -> LocalFuture<McpResult<CallToolResult>> {
            Box::pin(handler(args))
        });
        self.tools
            .insert(tool.name.clone(), ToolEntry { tool, handler: f });
        self
    }

    // ── Resources ─────────────────────────────────────────────────────────────

    /// Register a static resource.
    pub fn resource<F, Fut>(mut self, resource: Resource, handler: F) -> Self
    where
        F: Fn(ReadResourceRequest) -> Fut + 'static,
        Fut: Future<Output = McpResult<ReadResourceResult>> + 'static,
    {
        let f = Rc::new(
            move |req: ReadResourceRequest| -> LocalFuture<McpResult<ReadResourceResult>> {
                Box::pin(handler(req))
            },
        );
        self.resources.insert(
            resource.uri.clone(),
            ResourceEntry {
                resource,
                handler: f,
            },
        );
        self
    }

    /// Register a resource template (URI with variables like `user://{id}`).
    pub fn resource_template<F, Fut>(mut self, template: ResourceTemplate, handler: F) -> Self
    where
        F: Fn(ReadResourceRequest) -> Fut + 'static,
        Fut: Future<Output = McpResult<ReadResourceResult>> + 'static,
    {
        let f = Rc::new(
            move |req: ReadResourceRequest| -> LocalFuture<McpResult<ReadResourceResult>> {
                Box::pin(handler(req))
            },
        );
        self.resource_templates
            .push(ResourceTemplateEntry { template, handler: f });
        self
    }

    // ── Prompts ───────────────────────────────────────────────────────────────

    /// Register a prompt template.
    pub fn prompt<F, Fut>(mut self, prompt: Prompt, handler: F) -> Self
    where
        F: Fn(GetPromptRequest) -> Fut + 'static,
        Fut: Future<Output = McpResult<GetPromptResult>> + 'static,
    {
        let f = Rc::new(
            move |req: GetPromptRequest| -> LocalFuture<McpResult<GetPromptResult>> {
                Box::pin(handler(req))
            },
        );
        self.prompts
            .insert(prompt.name.clone(), PromptEntry { prompt, handler: f });
        self
    }

    pub fn build(self) -> CloudflareServer {
        CloudflareServer {
            info: ServerInfo::new(
                if self.name.is_empty() {
                    "mcp-cloudflare"
                } else {
                    &self.name
                },
                if self.version.is_empty() {
                    "1.0.0"
                } else {
                    &self.version
                },
            ),
            instructions: self.instructions,
            tools: self.tools,
            resources: self.resources,
            resource_templates: self.resource_templates,
            prompts: self.prompts,
        }
    }
}

// ─── Thread-local server instance ─────────────────────────────────────────────

thread_local! {
    static SERVER: Rc<CloudflareServer> = Rc::new(build_server());
}

fn get_server() -> Rc<CloudflareServer> {
    SERVER.with(Rc::clone)
}

// ─── Server Definition ────────────────────────────────────────────────────────

fn build_server() -> CloudflareServer {
    CloudflareServerBuilder::default()
        .name("mcp-cloudflare-demo")
        .version("1.0.0")
        .instructions(
            "A comprehensive MCP server running on Cloudflare Workers.\n\
             Available capabilities:\n\
             - Calculator tools (add, subtract, multiply, divide, sqrt)\n\
             - Text utilities (uppercase, lowercase, reverse, word_count)\n\
             - Resources (config, server info)\n\
             - Resource templates (user profiles, documents)\n\
             - Prompts (code review, summarize, translate)",
        )
        // ── Calculator Tools ──────────────────────────────────────────────────
        .tool(
            Tool::new("add", "Add two numbers", binary_schema()),
            |p: BinaryInput| async move { Ok(CallToolResult::text(format!("{}", p.a + p.b))) },
        )
        .tool(
            Tool::new("subtract", "Subtract b from a", binary_schema()),
            |p: BinaryInput| async move { Ok(CallToolResult::text(format!("{}", p.a - p.b))) },
        )
        .tool(
            Tool::new("multiply", "Multiply two numbers", binary_schema()),
            |p: BinaryInput| async move { Ok(CallToolResult::text(format!("{}", p.a * p.b))) },
        )
        .tool(
            Tool::new("divide", "Divide a by b", binary_schema()),
            |p: BinaryInput| async move {
                if p.b == 0.0 {
                    return Ok(CallToolResult::error("Division by zero"));
                }
                Ok(CallToolResult::text(format!("{}", p.a / p.b)))
            },
        )
        .tool(
            Tool::new(
                "sqrt",
                "Square root of n",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "n": { "type": "number", "description": "Non-negative number" }
                    },
                    "required": ["n"]
                }),
            ),
            |p: SqrtInput| async move {
                if p.n < 0.0 {
                    return Ok(CallToolResult::error("Cannot compute sqrt of negative number"));
                }
                Ok(CallToolResult::text(format!("{}", p.n.sqrt())))
            },
        )
        // ── Text Utility Tools ────────────────────────────────────────────────
        .tool(
            Tool::new("uppercase", "Convert text to uppercase", text_schema()),
            |p: TextInput| async move { Ok(CallToolResult::text(p.text.to_uppercase())) },
        )
        .tool(
            Tool::new("lowercase", "Convert text to lowercase", text_schema()),
            |p: TextInput| async move { Ok(CallToolResult::text(p.text.to_lowercase())) },
        )
        .tool(
            Tool::new(
                "reverse",
                "Reverse the characters in text",
                text_schema(),
            ),
            |p: TextInput| async move {
                Ok(CallToolResult::text(p.text.chars().rev().collect::<String>()))
            },
        )
        .tool(
            Tool::new("word_count", "Count words in text", text_schema()),
            |p: TextInput| async move {
                let count = p.text.split_whitespace().count();
                Ok(CallToolResult::text(format!("{count}")))
            },
        )
        .tool(
            Tool::new(
                "echo",
                "Echo back the input with metadata",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string", "description": "Message to echo" }
                    },
                    "required": ["message"]
                }),
            ),
            |p: EchoInput| async move {
                Ok(CallToolResult::new(vec![
                    Content::text(format!("You said: {}", p.message)),
                    Content::text(format!("Length: {} chars", p.message.len())),
                ]))
            },
        )
        // ── Static Resources ──────────────────────────────────────────────────
        .resource(
            Resource::new("config://app", "App Configuration")
                .with_description("Application configuration settings")
                .with_mime_type("application/json"),
            |req| async move {
                let config = serde_json::json!({
                    "name": "mcp-cloudflare-demo",
                    "version": "1.0.0",
                    "environment": "production",
                    "features": {
                        "tools": true,
                        "resources": true,
                        "prompts": true
                    },
                    "limits": {
                        "max_tokens": 4096,
                        "timeout_ms": 30000
                    }
                });
                Ok(ReadResourceResult::new(vec![ResourceContents::text(
                    req.uri,
                    serde_json::to_string_pretty(&config).unwrap(),
                )]))
            },
        )
        .resource(
            Resource::new("info://server", "Server Information")
                .with_description("Runtime server information")
                .with_mime_type("application/json"),
            |req| async move {
                let info = serde_json::json!({
                    "runtime": "Cloudflare Workers",
                    "transport": "Streamable HTTP",
                    "endpoint": "/mcp",
                    "capabilities": ["tools", "resources", "prompts"],
                    "timestamp": "2024-01-01T00:00:00Z"
                });
                Ok(ReadResourceResult::new(vec![ResourceContents::text(
                    req.uri,
                    serde_json::to_string_pretty(&info).unwrap(),
                )]))
            },
        )
        .resource(
            Resource::new("docs://readme", "README Documentation")
                .with_description("How to use this MCP server")
                .with_mime_type("text/markdown"),
            |req| async move {
                let readme = r#"# MCP Cloudflare Demo Server

## Overview
This is a demonstration MCP server running on Cloudflare Workers.

## Available Tools
- **Calculator**: add, subtract, multiply, divide, sqrt
- **Text Utils**: uppercase, lowercase, reverse, word_count, echo

## Available Resources
- `config://app` - Application configuration
- `info://server` - Server runtime information
- `user://{id}` - User profile by ID
- `doc://{id}` - Document by ID

## Available Prompts
- `code-review` - Review code for issues
- `summarize` - Summarize text content
- `translate` - Translate text to another language
"#;
                Ok(ReadResourceResult::new(vec![ResourceContents::text(
                    req.uri, readme,
                )]))
            },
        )
        // ── Resource Templates ────────────────────────────────────────────────
        .resource_template(
            ResourceTemplate::new("user://{id}", "User Profile")
                .with_description("Get user profile by ID")
                .with_mime_type("application/json"),
            |req| async move {
                // Extract ID from URI (e.g., "user://123" -> "123")
                let id = req.uri.strip_prefix("user://").unwrap_or("unknown");

                // Simulated user data
                let user = serde_json::json!({
                    "id": id,
                    "name": format!("User {}", id),
                    "email": format!("user{}@example.com", id),
                    "role": if id == "1" { "admin" } else { "user" },
                    "created_at": "2024-01-01T00:00:00Z"
                });

                Ok(ReadResourceResult::new(vec![ResourceContents::text(
                    req.uri,
                    serde_json::to_string_pretty(&user).unwrap(),
                )]))
            },
        )
        .resource_template(
            ResourceTemplate::new("doc://{id}", "Document")
                .with_description("Get document by ID")
                .with_mime_type("application/json"),
            |req| async move {
                let id = req.uri.strip_prefix("doc://").unwrap_or("unknown");

                let doc = serde_json::json!({
                    "id": id,
                    "title": format!("Document {}", id),
                    "content": format!("This is the content of document {}.", id),
                    "author": "System",
                    "updated_at": "2024-01-01T00:00:00Z"
                });

                Ok(ReadResourceResult::new(vec![ResourceContents::text(
                    req.uri,
                    serde_json::to_string_pretty(&doc).unwrap(),
                )]))
            },
        )
        // ── Prompts ───────────────────────────────────────────────────────────
        .prompt(
            Prompt::new("code-review", "Review code for bugs and improvements")
                .with_argument(PromptArgument::new("code", "The code to review").required())
                .with_argument(PromptArgument::new("language", "Programming language")),
            |req| async move {
                let args = req.arguments.unwrap_or_default();
                let code = args.get("code").map(|s| s.as_str()).unwrap_or("");
                let lang = args
                    .get("language")
                    .map(|s| s.as_str())
                    .unwrap_or("unknown");

                Ok(GetPromptResult {
                    description: Some(format!("Code review for {} code", lang)),
                    messages: vec![PromptMessage::user(format!(
                        "Please review the following {} code for:\n\
                         1. Potential bugs and errors\n\
                         2. Performance improvements\n\
                         3. Code style and best practices\n\
                         4. Security vulnerabilities\n\n\
                         ```{}\n{}\n```",
                        lang, lang, code
                    ))],
                })
            },
        )
        .prompt(
            Prompt::new("summarize", "Summarize text content")
                .with_argument(PromptArgument::new("text", "The text to summarize").required())
                .with_argument(PromptArgument::new(
                    "max_sentences",
                    "Maximum sentences in summary",
                )),
            |req| async move {
                let args = req.arguments.unwrap_or_default();
                let text = args.get("text").map(|s| s.as_str()).unwrap_or("");
                let max = args
                    .get("max_sentences")
                    .map(|s| s.as_str())
                    .unwrap_or("3");

                Ok(GetPromptResult {
                    description: Some("Text summarization".into()),
                    messages: vec![PromptMessage::user(format!(
                        "Please summarize the following text in {} sentences or fewer:\n\n{}",
                        max, text
                    ))],
                })
            },
        )
        .prompt(
            Prompt::new("translate", "Translate text to another language")
                .with_argument(PromptArgument::new("text", "The text to translate").required())
                .with_argument(
                    PromptArgument::new("target_language", "Target language (e.g., Spanish)")
                        .required(),
                ),
            |req| async move {
                let args = req.arguments.unwrap_or_default();
                let text = args.get("text").map(|s| s.as_str()).unwrap_or("");
                let target = args
                    .get("target_language")
                    .map(|s| s.as_str())
                    .unwrap_or("English");

                Ok(GetPromptResult {
                    description: Some(format!("Translation to {}", target)),
                    messages: vec![PromptMessage::user(format!(
                        "Please translate the following text to {}:\n\n{}",
                        target, text
                    ))],
                })
            },
        )
        .build()
}

// ─── Input Types ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct BinaryInput {
    a: f64,
    b: f64,
}

#[derive(Deserialize)]
struct SqrtInput {
    n: f64,
}

#[derive(Deserialize)]
struct TextInput {
    text: String,
}

#[derive(Deserialize)]
struct EchoInput {
    message: String,
}

// ─── Schema Helpers ───────────────────────────────────────────────────────────

fn binary_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "a": { "type": "number", "description": "First operand" },
            "b": { "type": "number", "description": "Second operand" }
        },
        "required": ["a", "b"]
    })
}

fn text_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "text": { "type": "string", "description": "Input text" }
        },
        "required": ["text"]
    })
}

// ─── Cloudflare Workers Entry Point ───────────────────────────────────────────

#[event(fetch)]
pub async fn main(mut req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    let method = req.method();
    let path = req.path();

    // CORS preflight
    if method == Method::Options {
        return Ok(cors_response(Response::empty()?));
    }

    match (method, path.as_str()) {
        // POST /mcp — JSON-RPC endpoint
        (Method::Post, "/mcp") => {
            let msg: JsonRpcMessage = req
                .json()
                .await
                .map_err(|e| Error::RustError(format!("Invalid JSON: {e}")))?;

            let server = get_server();
            let response = server.handle(msg).await;

            match response {
                Some(resp) => {
                    let body =
                        serde_json::to_string(&resp).map_err(|e| Error::RustError(e.to_string()))?;
                    Ok(cors_response(
                        Response::ok(body)?.with_headers(json_headers()),
                    ))
                }
                None => Ok(cors_response(Response::empty()?.with_status(202))),
            }
        }

        // GET /mcp — Server discovery
        (Method::Get, "/mcp") => {
            let server = get_server();
            let body = serde_json::json!({
                "name": server.info.name,
                "version": server.info.version,
                "protocol_version": MCP_PROTOCOL_VERSION,
                "transport": "streamable-http",
                "endpoint": "/mcp",
                "capabilities": {
                    "tools": !server.tools.is_empty(),
                    "resources": !server.resources.is_empty() || !server.resource_templates.is_empty(),
                    "prompts": !server.prompts.is_empty()
                }
            });
            Ok(cors_response(
                Response::from_json(&body)?.with_headers(json_headers()),
            ))
        }

        // Health check
        (Method::Get, "/health") => Ok(cors_response(Response::ok("OK")?)),

        _ => Response::error("Not Found", 404),
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn json_headers() -> Headers {
    let mut h = Headers::new();
    let _ = h.set("content-type", "application/json");
    h
}

fn cors_response(resp: Response) -> Response {
    let mut h = Headers::new();
    let _ = h.set("access-control-allow-origin", "*");
    let _ = h.set("access-control-allow-methods", "GET, POST, OPTIONS");
    let _ = h.set(
        "access-control-allow-headers",
        "content-type, authorization",
    );
    let _ = h.set("access-control-max-age", "86400");
    resp.with_headers(h)
}

/// Simple URI template matcher — supports `{variable}` placeholders.
fn uri_matches_template(uri: &str, template: &str) -> bool {
    let mut uri_chars = uri.chars().peekable();
    let mut tpl_chars = template.chars().peekable();

    while let Some(&tc) = tpl_chars.peek() {
        if tc == '{' {
            // Skip variable name until '}'
            while tpl_chars.next().map(|c| c != '}').unwrap_or(false) {}
            // Consume non-'/' characters in uri
            if uri_chars.peek().is_none() {
                return false;
            }
            while uri_chars.peek().map(|&c| c != '/').unwrap_or(false) {
                uri_chars.next();
            }
        } else {
            tpl_chars.next();
            if uri_chars.next() != Some(tc) {
                return false;
            }
        }
    }

    uri_chars.peek().is_none()
}
