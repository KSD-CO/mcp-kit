//! Roots support for file system access.
//!
//! Clients can provide a list of root URIs that the server is allowed to access.
//! This is useful for sandboxing file operations and providing context about
//! the user's workspace.

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::Root;

/// Manages the client's declared root URIs.
///
/// Roots are directories or files that the client has exposed to the server.
/// The server should only access files within these roots.
#[derive(Clone, Default)]
pub struct RootsManager {
    inner: Arc<RwLock<RootsState>>,
}

#[derive(Default)]
struct RootsState {
    roots: Vec<Root>,
    uris: HashSet<String>,
}

impl RootsManager {
    /// Create a new roots manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the list of roots from the client.
    pub async fn set_roots(&self, roots: Vec<Root>) {
        let mut state = self.inner.write().await;
        state.uris = roots.iter().map(|r| r.uri.clone()).collect();
        state.roots = roots;
    }

    /// Get all declared roots.
    pub async fn roots(&self) -> Vec<Root> {
        let state = self.inner.read().await;
        state.roots.clone()
    }

    /// Check if a URI is within any declared root.
    ///
    /// Returns `true` if the URI starts with any root URI prefix.
    pub async fn is_within_roots(&self, uri: &str) -> bool {
        let state = self.inner.read().await;
        if state.roots.is_empty() {
            // No roots declared = allow all (backwards compatibility)
            return true;
        }
        state.uris.iter().any(|root| uri.starts_with(root))
    }

    /// Find the root that contains a given URI.
    pub async fn find_root(&self, uri: &str) -> Option<Root> {
        let state = self.inner.read().await;
        state
            .roots
            .iter()
            .find(|r| uri.starts_with(&r.uri))
            .cloned()
    }

    /// Get the number of declared roots.
    pub async fn count(&self) -> usize {
        let state = self.inner.read().await;
        state.roots.len()
    }

    /// Check if any roots are declared.
    pub async fn has_roots(&self) -> bool {
        let state = self.inner.read().await;
        !state.roots.is_empty()
    }

    /// Clear all roots.
    pub async fn clear(&self) {
        let mut state = self.inner.write().await;
        state.roots.clear();
        state.uris.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_and_get_roots() {
        let mgr = RootsManager::new();

        let roots = vec![
            Root {
                uri: "file:///home/user/project".to_string(),
                name: Some("Project".to_string()),
            },
            Root {
                uri: "file:///tmp".to_string(),
                name: None,
            },
        ];

        mgr.set_roots(roots.clone()).await;

        assert_eq!(mgr.count().await, 2);
        assert!(mgr.has_roots().await);
    }

    #[tokio::test]
    async fn test_is_within_roots() {
        let mgr = RootsManager::new();

        mgr.set_roots(vec![Root {
            uri: "file:///home/user/project".to_string(),
            name: None,
        }])
        .await;

        assert!(
            mgr.is_within_roots("file:///home/user/project/src/main.rs")
                .await
        );
        assert!(!mgr.is_within_roots("file:///etc/passwd").await);
    }

    #[tokio::test]
    async fn test_no_roots_allows_all() {
        let mgr = RootsManager::new();
        // No roots set = allow all
        assert!(mgr.is_within_roots("file:///anywhere").await);
    }
}
