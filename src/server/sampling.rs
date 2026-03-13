//! Sampling API for server-initiated LLM requests.
//!
//! This module allows MCP servers to request LLM completions from clients
//! that support the sampling capability. This enables agentic workflows
//! where the server can leverage the client's LLM for complex tasks.
//!
//! # Example
//! ```rust,ignore
//! use mcp_kit::server::sampling::{SamplingRequest, SamplingClient};
//!
//! async fn agentic_tool(client: impl SamplingClient) -> Result<String, Error> {
//!     let request = SamplingRequest::new()
//!         .add_user_message("Analyze this data and suggest improvements")
//!         .max_tokens(1000);
//!     
//!     let response = client.create_message(request).await?;
//!     Ok(response.content.text().unwrap_or_default())
//! }
//! ```

use crate::error::McpResult;
use crate::protocol::{JsonRpcRequest, RequestId};
use crate::types::sampling::{
    CreateMessageRequest, CreateMessageResult, IncludeContext, ModelPreferences, SamplingMessage,
};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// A client for making sampling requests to the MCP client.
///
/// Implementations handle the actual communication with the client.
pub trait SamplingClient: Send + Sync {
    /// Send a sampling/createMessage request to the client.
    fn create_message(
        &self,
        request: CreateMessageRequest,
    ) -> Pin<Box<dyn Future<Output = McpResult<CreateMessageResult>> + Send + '_>>;
}

/// Builder for creating sampling requests.
#[derive(Debug, Clone, Default)]
pub struct SamplingRequestBuilder {
    messages: Vec<SamplingMessage>,
    model_preferences: Option<ModelPreferences>,
    system_prompt: Option<String>,
    max_tokens: u32,
    stop_sequences: Option<Vec<String>>,
    temperature: Option<f64>,
    metadata: Option<Value>,
    include_context: Option<IncludeContext>,
}

impl SamplingRequestBuilder {
    /// Create a new sampling request builder.
    pub fn new() -> Self {
        Self {
            max_tokens: 1000,
            ..Default::default()
        }
    }

    /// Add a user message.
    pub fn user_message(mut self, content: impl Into<String>) -> Self {
        self.messages.push(SamplingMessage::user_text(content));
        self
    }

    /// Add an assistant message (for multi-turn conversations).
    pub fn assistant_message(mut self, content: impl Into<String>) -> Self {
        self.messages.push(SamplingMessage::assistant_text(content));
        self
    }

    /// Add a raw message with role and content.
    pub fn message(mut self, message: SamplingMessage) -> Self {
        self.messages.push(message);
        self
    }

    /// Set the system prompt.
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set maximum tokens for the response.
    pub fn max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = tokens;
        self
    }

    /// Set stop sequences.
    pub fn stop_sequences(mut self, sequences: Vec<String>) -> Self {
        self.stop_sequences = Some(sequences);
        self
    }

    /// Set temperature for sampling.
    pub fn temperature(mut self, temp: f64) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set model preferences.
    pub fn model_preferences(mut self, prefs: ModelPreferences) -> Self {
        self.model_preferences = Some(prefs);
        self
    }

    /// Set whether to include MCP context.
    pub fn include_context(mut self, include: IncludeContext) -> Self {
        self.include_context = Some(include);
        self
    }

    /// Set custom metadata.
    pub fn metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Build the request.
    pub fn build(self) -> CreateMessageRequest {
        CreateMessageRequest {
            messages: self.messages,
            model_preferences: self.model_preferences,
            system_prompt: self.system_prompt,
            include_context: self.include_context,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            stop_sequences: self.stop_sequences,
            metadata: self.metadata,
        }
    }
}

/// Channel-based sampling client implementation.
///
/// Sends requests through a channel to be forwarded to the MCP client.
#[derive(Clone)]
pub struct ChannelSamplingClient {
    request_tx: mpsc::Sender<(JsonRpcRequest, oneshot::Sender<McpResult<Value>>)>,
    next_id: Arc<AtomicU64>,
}

impl ChannelSamplingClient {
    /// Create a new channel-based sampling client.
    pub fn new(
        request_tx: mpsc::Sender<(JsonRpcRequest, oneshot::Sender<McpResult<Value>>)>,
    ) -> Self {
        Self {
            request_tx,
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    fn next_request_id(&self) -> RequestId {
        RequestId::Number(self.next_id.fetch_add(1, Ordering::SeqCst) as i64)
    }
}

impl SamplingClient for ChannelSamplingClient {
    fn create_message(
        &self,
        request: CreateMessageRequest,
    ) -> Pin<Box<dyn Future<Output = McpResult<CreateMessageResult>> + Send + '_>> {
        Box::pin(async move {
            let (response_tx, response_rx) = oneshot::channel();

            let rpc_request = JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: self.next_request_id(),
                method: "sampling/createMessage".to_string(),
                params: Some(serde_json::to_value(&request)?),
            };

            self.request_tx
                .send((rpc_request, response_tx))
                .await
                .map_err(|_| {
                    crate::error::McpError::InternalError("Sampling channel closed".to_string())
                })?;

            let result = response_rx.await.map_err(|_| {
                crate::error::McpError::InternalError("Response channel closed".to_string())
            })??;

            serde_json::from_value(result).map_err(|e| {
                crate::error::McpError::InternalError(format!("Invalid sampling response: {}", e))
            })
        })
    }
}

/// No-op sampling client for when client doesn't support sampling.
#[derive(Clone, Default)]
pub struct NoOpSamplingClient;

impl SamplingClient for NoOpSamplingClient {
    fn create_message(
        &self,
        _request: CreateMessageRequest,
    ) -> Pin<Box<dyn Future<Output = McpResult<CreateMessageResult>> + Send + '_>> {
        Box::pin(async move {
            Err(crate::error::McpError::InvalidRequest(
                "Client does not support sampling".to_string(),
            ))
        })
    }
}
