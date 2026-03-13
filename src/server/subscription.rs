//! Resource subscription management.
//!
//! Tracks which resources clients have subscribed to, enabling the server
//! to send targeted `notifications/resources/updated` when resources change.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::server::session::SessionId;

/// Manages resource subscriptions across all sessions.
///
/// Thread-safe and can be shared across handlers and background tasks.
#[derive(Clone, Default)]
pub struct SubscriptionManager {
    inner: Arc<RwLock<SubscriptionState>>,
}

#[derive(Default)]
struct SubscriptionState {
    /// Map from resource URI to set of subscribed session IDs
    by_resource: HashMap<String, HashSet<SessionId>>,
    /// Map from session ID to set of subscribed resource URIs
    by_session: HashMap<SessionId, HashSet<String>>,
}

impl SubscriptionManager {
    /// Create a new subscription manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe a session to a resource.
    ///
    /// Returns `true` if this is a new subscription, `false` if already subscribed.
    pub async fn subscribe(&self, session_id: &SessionId, uri: &str) -> bool {
        let mut state = self.inner.write().await;

        let resource_subs = state
            .by_resource
            .entry(uri.to_string())
            .or_insert_with(HashSet::new);
        let is_new = resource_subs.insert(session_id.clone());

        if is_new {
            state
                .by_session
                .entry(session_id.clone())
                .or_insert_with(HashSet::new)
                .insert(uri.to_string());
        }

        is_new
    }

    /// Unsubscribe a session from a resource.
    ///
    /// Returns `true` if the subscription existed, `false` otherwise.
    pub async fn unsubscribe(&self, session_id: &SessionId, uri: &str) -> bool {
        let mut state = self.inner.write().await;

        let removed = if let Some(resource_subs) = state.by_resource.get_mut(uri) {
            let removed = resource_subs.remove(session_id);
            if resource_subs.is_empty() {
                state.by_resource.remove(uri);
            }
            removed
        } else {
            false
        };

        if removed {
            if let Some(session_subs) = state.by_session.get_mut(session_id) {
                session_subs.remove(uri);
                if session_subs.is_empty() {
                    state.by_session.remove(session_id);
                }
            }
        }

        removed
    }

    /// Unsubscribe a session from all resources.
    ///
    /// Call this when a session disconnects.
    pub async fn unsubscribe_all(&self, session_id: &SessionId) {
        let mut state = self.inner.write().await;

        if let Some(uris) = state.by_session.remove(session_id) {
            for uri in uris {
                if let Some(resource_subs) = state.by_resource.get_mut(&uri) {
                    resource_subs.remove(session_id);
                    if resource_subs.is_empty() {
                        state.by_resource.remove(&uri);
                    }
                }
            }
        }
    }

    /// Get all session IDs subscribed to a resource.
    pub async fn subscribers(&self, uri: &str) -> Vec<SessionId> {
        let state = self.inner.read().await;
        state
            .by_resource
            .get(uri)
            .map(|subs| subs.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all resources a session is subscribed to.
    pub async fn subscriptions(&self, session_id: &SessionId) -> Vec<String> {
        let state = self.inner.read().await;
        state
            .by_session
            .get(session_id)
            .map(|subs| subs.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Check if a session is subscribed to a resource.
    pub async fn is_subscribed(&self, session_id: &SessionId, uri: &str) -> bool {
        let state = self.inner.read().await;
        state
            .by_resource
            .get(uri)
            .map(|subs| subs.contains(session_id))
            .unwrap_or(false)
    }

    /// Get the number of subscribers for a resource.
    pub async fn subscriber_count(&self, uri: &str) -> usize {
        let state = self.inner.read().await;
        state.by_resource.get(uri).map(|s| s.len()).unwrap_or(0)
    }

    /// Get total number of active subscriptions.
    pub async fn total_subscriptions(&self) -> usize {
        let state = self.inner.read().await;
        state.by_resource.values().map(|s| s.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_subscribe_unsubscribe() {
        let mgr = SubscriptionManager::new();
        let session = SessionId::new();

        assert!(mgr.subscribe(&session, "file:///test.txt").await);
        assert!(!mgr.subscribe(&session, "file:///test.txt").await); // duplicate

        assert!(mgr.is_subscribed(&session, "file:///test.txt").await);
        assert_eq!(mgr.subscriber_count("file:///test.txt").await, 1);

        assert!(mgr.unsubscribe(&session, "file:///test.txt").await);
        assert!(!mgr.is_subscribed(&session, "file:///test.txt").await);
    }

    #[tokio::test]
    async fn test_unsubscribe_all() {
        let mgr = SubscriptionManager::new();
        let session = SessionId::new();

        mgr.subscribe(&session, "file:///a.txt").await;
        mgr.subscribe(&session, "file:///b.txt").await;

        assert_eq!(mgr.subscriptions(&session).await.len(), 2);

        mgr.unsubscribe_all(&session).await;

        assert_eq!(mgr.subscriptions(&session).await.len(), 0);
        assert_eq!(mgr.total_subscriptions().await, 0);
    }
}
