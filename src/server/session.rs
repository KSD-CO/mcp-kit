use uuid::Uuid;

use crate::types::{ClientCapabilities, ClientInfo, Root};

#[cfg(feature = "auth")]
use crate::auth::AuthenticatedIdentity;

/// Unique session identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Per-connection session data
#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub client_info: Option<ClientInfo>,
    pub client_capabilities: Option<ClientCapabilities>,
    pub protocol_version: Option<String>,
    pub initialized: bool,
    /// Roots declared by the client.
    pub roots: Vec<Root>,
    /// Populated by the transport layer after successful authentication.
    /// `None` means the request was unauthenticated (or auth is not configured).
    #[cfg(feature = "auth")]
    pub identity: Option<AuthenticatedIdentity>,
}

impl Session {
    pub fn new() -> Self {
        Self {
            id: SessionId::new(),
            client_info: None,
            client_capabilities: None,
            protocol_version: None,
            initialized: false,
            roots: Vec::new(),
            #[cfg(feature = "auth")]
            identity: None,
        }
    }

    /// Check if the client supports sampling.
    pub fn supports_sampling(&self) -> bool {
        self.client_capabilities
            .as_ref()
            .and_then(|c| c.sampling.as_ref())
            .is_some()
    }

    /// Check if the client supports roots.
    pub fn supports_roots(&self) -> bool {
        self.client_capabilities
            .as_ref()
            .and_then(|c| c.roots.as_ref())
            .map(|r| r.list_changed.unwrap_or(false))
            .unwrap_or(false)
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
