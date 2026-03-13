//! WebSocket transport for MCP servers.
//!
//! Provides bidirectional communication over WebSocket connections,
//! suitable for browser-based clients and real-time applications.
//!
//! # Example
//! ```rust,ignore
//! use mcp_kit::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let server = McpServer::builder()
//!         .name("ws-server")
//!         .version("1.0.0")
//!         .build();
//!
//!     server.serve_websocket("0.0.0.0:3000").await?;
//!     Ok(())
//! }
//! ```

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::error::McpError;
use crate::protocol::JsonRpcMessage;
use crate::server::{session::Session, McpServer, NotificationSender};

/// Extension trait for serving MCP over WebSocket.
pub trait ServeWebSocketExt {
    /// Start serving MCP over WebSocket on the given address.
    fn serve_websocket(
        self,
        addr: impl Into<SocketAddr> + Send,
    ) -> impl std::future::Future<Output = Result<(), McpError>> + Send;

    /// Start serving MCP over WebSocket with a custom notification channel buffer size.
    fn serve_websocket_with_buffer(
        self,
        addr: impl Into<SocketAddr> + Send,
        buffer_size: usize,
    ) -> impl std::future::Future<Output = Result<(), McpError>> + Send;
}

impl ServeWebSocketExt for McpServer {
    async fn serve_websocket(self, addr: impl Into<SocketAddr> + Send) -> Result<(), McpError> {
        self.serve_websocket_with_buffer(addr, 32).await
    }

    async fn serve_websocket_with_buffer(
        self,
        addr: impl Into<SocketAddr> + Send,
        buffer_size: usize,
    ) -> Result<(), McpError> {
        let addr = addr.into();
        let state = WebSocketState {
            server: Arc::new(self),
            buffer_size,
        };

        let app = Router::new()
            .route("/ws", get(ws_handler))
            .route("/mcp", get(ws_handler))
            .route("/health", get(|| async { "OK" }))
            .with_state(state);

        info!("Starting WebSocket MCP server on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

#[derive(Clone)]
struct WebSocketState {
    server: Arc<McpServer>,
    buffer_size: usize,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<WebSocketState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: WebSocketState) {
    let mut session = Session::new();
    let session_id = session.id.clone();
    info!(session_id = %session_id, "WebSocket client connected");

    // Create notification channel for this session
    let (_notifier, mut notification_rx) = NotificationSender::channel(state.buffer_size);

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Channel for sending messages back to the client
    let (tx, mut rx) = mpsc::channel::<String>(state.buffer_size);
    let tx_for_notifications = tx.clone();

    // Task to forward notifications to the client
    let notification_task = tokio::spawn(async move {
        while let Some(notification) = notification_rx.recv().await {
            let msg = JsonRpcMessage::Notification(notification);
            if let Ok(json) = serde_json::to_string(&msg) {
                if tx_for_notifications.send(json).await.is_err() {
                    break;
                }
            }
        }
    });

    // Task to send messages to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_tx.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Main message loop
    let server = state.server.clone();
    while let Some(msg) = ws_rx.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                debug!(session_id = %session_id, "Received message");
                match serde_json::from_str::<JsonRpcMessage>(&text) {
                    Ok(request) => {
                        if let Some(response) = server.handle_message(request, &mut session).await {
                            match serde_json::to_string(&response) {
                                Ok(json) => {
                                    if tx.send(json).await.is_err() {
                                        error!(session_id = %session_id, "Failed to send response");
                                        break;
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to serialize response: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(session_id = %session_id, error = %e, "Invalid JSON-RPC message");
                    }
                }
            }
            Ok(Message::Binary(_)) => {
                warn!(session_id = %session_id, "Received binary message (not supported)");
            }
            Ok(Message::Ping(_)) => {
                debug!(session_id = %session_id, "Received ping");
                // Axum handles pong automatically
            }
            Ok(Message::Pong(_)) => {
                debug!(session_id = %session_id, "Received pong");
            }
            Ok(Message::Close(_)) => {
                info!(session_id = %session_id, "Client disconnected");
                break;
            }
            Err(e) => {
                error!(session_id = %session_id, error = %e, "WebSocket error");
                break;
            }
        }
    }

    // Clean up
    notification_task.abort();
    send_task.abort();
    info!(session_id = %session_id, "WebSocket session ended");
}

/// A more complete WebSocket transport with proper bidirectional communication.
pub struct WebSocketTransport {
    server: Arc<McpServer>,
    buffer_size: usize,
}

impl WebSocketTransport {
    /// Create a new WebSocket transport.
    pub fn new(server: McpServer, buffer_size: usize) -> Self {
        Self {
            server: Arc::new(server),
            buffer_size,
        }
    }

    /// Handle a WebSocket connection.
    pub async fn handle_connection(&self, socket: WebSocket) {
        let mut session = Session::new();
        let session_id = session.id.clone();
        info!(session_id = %session_id, "WebSocket client connected");

        let (_notifier, mut notification_rx) = NotificationSender::channel(self.buffer_size);
        let (mut ws_tx, mut ws_rx) = socket.split();

        // Channel for sending responses back to the client
        let (response_tx, mut response_rx) = mpsc::channel::<String>(self.buffer_size);
        let tx_for_notifications = response_tx.clone();

        // Task to forward notifications
        let notification_task = tokio::spawn(async move {
            while let Some(notification) = notification_rx.recv().await {
                let msg = JsonRpcMessage::Notification(notification);
                if let Ok(json) = serde_json::to_string(&msg) {
                    if tx_for_notifications.send(json).await.is_err() {
                        break;
                    }
                }
            }
        });

        // Task to send messages to the client
        let send_task = tokio::spawn(async move {
            while let Some(msg) = response_rx.recv().await {
                if ws_tx.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        });

        // Main message loop
        let server = self.server.clone();
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(Message::Text(text)) => match serde_json::from_str::<JsonRpcMessage>(&text) {
                    Ok(request) => {
                        if let Some(response) = server.handle_message(request, &mut session).await {
                            if let Ok(json) = serde_json::to_string(&response) {
                                let _ = response_tx.send(json).await;
                            }
                        }
                    }
                    Err(e) => {
                        warn!(session_id = %session_id, error = %e, "Invalid message");
                    }
                },
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }

        notification_task.abort();
        send_task.abort();
        info!(session_id = %session_id, "WebSocket session ended");
    }
}
