//! Request cancellation support.
//!
//! Tracks pending requests and allows cancellation via the
//! `notifications/cancelled` notification from clients.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::protocol::RequestId;
use crate::server::session::SessionId;

/// Manages pending requests and their cancellation tokens.
///
/// When a client sends `notifications/cancelled`, we can abort the
/// corresponding in-flight request.
#[derive(Clone, Default)]
pub struct CancellationManager {
    inner: Arc<RwLock<CancellationState>>,
}

#[derive(Default)]
struct CancellationState {
    /// Map from (session_id, request_id) to cancellation token
    pending: HashMap<(SessionId, RequestId), CancellationToken>,
}

impl CancellationManager {
    /// Create a new cancellation manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new pending request and get a cancellation token.
    ///
    /// The handler should check `token.is_cancelled()` periodically or
    /// use `token.cancelled().await` to respond to cancellation.
    pub async fn register(
        &self,
        session_id: &SessionId,
        request_id: &RequestId,
    ) -> CancellationToken {
        let token = CancellationToken::new();
        let mut state = self.inner.write().await;
        state
            .pending
            .insert((session_id.clone(), request_id.clone()), token.clone());
        token
    }

    /// Remove a completed request from tracking.
    ///
    /// Call this when the request completes (success or error).
    pub async fn complete(&self, session_id: &SessionId, request_id: &RequestId) {
        let mut state = self.inner.write().await;
        state
            .pending
            .remove(&(session_id.clone(), request_id.clone()));
    }

    /// Cancel a pending request.
    ///
    /// Returns `true` if the request was found and cancelled, `false` otherwise.
    pub async fn cancel(&self, session_id: &SessionId, request_id: &RequestId) -> bool {
        let state = self.inner.read().await;
        if let Some(token) = state.pending.get(&(session_id.clone(), request_id.clone())) {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// Cancel all pending requests for a session.
    ///
    /// Call this when a session disconnects.
    pub async fn cancel_all(&self, session_id: &SessionId) {
        let state = self.inner.read().await;
        for ((sid, _), token) in state.pending.iter() {
            if sid == session_id {
                token.cancel();
            }
        }
        drop(state);

        // Clean up entries
        let mut state = self.inner.write().await;
        state.pending.retain(|(sid, _), _| sid != session_id);
    }

    /// Get the number of pending requests.
    pub async fn pending_count(&self) -> usize {
        let state = self.inner.read().await;
        state.pending.len()
    }

    /// Check if a request is still pending (not cancelled).
    pub async fn is_pending(&self, session_id: &SessionId, request_id: &RequestId) -> bool {
        let state = self.inner.read().await;
        state
            .pending
            .get(&(session_id.clone(), request_id.clone()))
            .map(|t| !t.is_cancelled())
            .unwrap_or(false)
    }
}

/// A guard that automatically completes the request when dropped.
///
/// Use this to ensure requests are properly cleaned up even on early returns.
pub struct RequestGuard {
    manager: CancellationManager,
    session_id: SessionId,
    request_id: RequestId,
    token: CancellationToken,
}

impl RequestGuard {
    /// Create a new request guard.
    pub async fn new(
        manager: CancellationManager,
        session_id: SessionId,
        request_id: RequestId,
    ) -> Self {
        let token = manager.register(&session_id, &request_id).await;
        Self {
            manager,
            session_id,
            request_id,
            token,
        }
    }

    /// Get the cancellation token.
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// Check if the request has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Wait for cancellation.
    pub async fn cancelled(&self) {
        self.token.cancelled().await
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        // We can't await in drop, so spawn a task
        let manager = self.manager.clone();
        let session_id = self.session_id.clone();
        let request_id = self.request_id.clone();
        tokio::spawn(async move {
            manager.complete(&session_id, &request_id).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_cancel() {
        let mgr = CancellationManager::new();
        let session = SessionId::new();
        let request_id = RequestId::Number(1);

        let token = mgr.register(&session, &request_id).await;
        assert!(!token.is_cancelled());
        assert!(mgr.is_pending(&session, &request_id).await);

        mgr.cancel(&session, &request_id).await;
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn test_complete_removes() {
        let mgr = CancellationManager::new();
        let session = SessionId::new();
        let request_id = RequestId::Number(1);

        mgr.register(&session, &request_id).await;
        assert_eq!(mgr.pending_count().await, 1);

        mgr.complete(&session, &request_id).await;
        assert_eq!(mgr.pending_count().await, 0);
    }
}
