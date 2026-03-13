//! Client error types.

use thiserror::Error;

/// Result type for client operations.
pub type ClientResult<T> = Result<T, ClientError>;

/// Client errors.
#[derive(Debug, Error)]
pub enum ClientError {
    /// Transport error (connection failed, IO error, etc.)
    #[error("Transport error: {0}")]
    Transport(String),

    /// JSON-RPC error returned by the server
    #[error("Server error ({code}): {message}")]
    ServerError { code: i32, message: String },

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Request timeout
    #[error("Request timeout")]
    Timeout,

    /// Connection closed
    #[error("Connection closed")]
    Closed,

    /// Invalid response
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Protocol error
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Client not initialized
    #[error("Client not initialized - call initialize() first")]
    NotInitialized,

    /// Request cancelled
    #[error("Request cancelled")]
    Cancelled,
}
