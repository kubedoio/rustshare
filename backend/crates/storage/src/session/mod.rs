//! Session management for zero-PostgreSQL authentication
//!
//! Provides stateless JWT-based sessions with optional revocation caching.
//! Two modes:
//! - Stateless: JWT validation only, revocation via short TTL cache
//! - With revocation cache: Redis or memory-backed revocation tracking

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

pub mod manager;

pub use manager::SessionManager;

/// Errors that can occur during session operations
#[derive(Debug, Error, Clone)]
pub enum SessionError {
    #[error("Invalid session token")]
    InvalidToken,
    
    #[error("Session has expired")]
    Expired,
    
    #[error("Session has been revoked")]
    Revoked,
    
    #[error("Session backend error: {0}")]
    BackendError(String),
}

/// Session claims embedded in JWT
#[derive(Debug, Clone)]
pub struct SessionClaims {
    /// User ID
    pub user_id: Uuid,
    /// User email
    pub email: String,
    /// Session ID (for revocation tracking)
    pub session_id: String,
    /// Token issued at
    pub issued_at: DateTime<Utc>,
    /// Token expires at
    pub expires_at: DateTime<Utc>,
    /// Session type (web, api, device)
    pub session_type: SessionType,
}

/// Session types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    /// Web browser session (HTTP-only cookie)
    Web,
    /// API token (Authorization header)
    Api,
    /// Device token (long-lived)
    Device,
    /// Share access token (limited permissions)
    Share,
}

impl SessionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionType::Web => "web",
            SessionType::Api => "api",
            SessionType::Device => "device",
            SessionType::Share => "share",
        }
    }
}

/// Session validation result
#[derive(Debug, Clone)]
pub enum ValidationResult {
    /// Session is valid
    Valid(SessionClaims),
    /// Session is invalid with reason
    Invalid(SessionError),
}

/// Session information returned on creation
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// Session token (JWT)
    pub token: String,
    /// Session ID
    pub session_id: String,
    /// Expires at
    pub expires_at: DateTime<Utc>,
    /// Cookie value (for web sessions)
    pub cookie_value: Option<String>,
}

/// Configuration for session management
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// JWT secret for signing
    pub jwt_secret: String,
    /// Session TTL
    pub session_ttl: Duration,
    /// Whether to use revocation cache
    pub use_revocation_cache: bool,
    /// Revocation cache TTL (should match session TTL)
    pub revocation_cache_ttl: Duration,
    /// Cookie name for web sessions
    pub cookie_name: String,
    /// Whether cookie should be secure (HTTPS only)
    pub cookie_secure: bool,
    /// Cookie same-site policy
    pub cookie_same_site: String,
}

impl SessionConfig {
    /// Create default configuration.
    ///
    /// The jwt_secret can be any type that converts into a String,
    /// such as `&str` or `String`.
    pub fn new(jwt_secret: impl Into<String>) -> Self {
        Self {
            jwt_secret: jwt_secret.into(),
            session_ttl: Duration::from_secs(24 * 3600), // 24 hours
            use_revocation_cache: true,
            revocation_cache_ttl: Duration::from_secs(24 * 3600),
            cookie_name: "rustshare_session".to_string(),
            cookie_secure: true,
            cookie_same_site: "strict".to_string(),
        }
    }
    
    /// Load from environment
    pub fn from_env() -> anyhow::Result<Self> {
        let jwt_secret = std::env::var("JWT_SECRET")?;
        let session_ttl_secs = std::env::var("SESSION_TTL_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24 * 3600);
        let use_revocation_cache = std::env::var("SESSION_USE_REVOCATION_CACHE")
            .ok()
            .map(|s| s == "true" || s == "1")
            .unwrap_or(true);
        let cookie_secure = std::env::var("SESSION_COOKIE_SECURE")
            .ok()
            .map(|s| s == "true" || s == "1")
            .unwrap_or(true);
        
        Ok(Self {
            jwt_secret,
            session_ttl: Duration::from_secs(session_ttl_secs),
            use_revocation_cache,
            revocation_cache_ttl: Duration::from_secs(session_ttl_secs),
            cookie_name: std::env::var("SESSION_COOKIE_NAME")
                .unwrap_or_else(|_| "rustshare_session".to_string()),
            cookie_secure,
            cookie_same_site: std::env::var("SESSION_COOKIE_SAME_SITE")
                .unwrap_or_else(|_| "strict".to_string()),
        })
    }
}

/// Trait for session storage backends (revocation cache)
#[async_trait]
pub trait SessionStorage: Send + Sync {
    /// Store a session hash as valid
    async fn store_session(
        &self,
        session_hash: &str,
        user_id: Uuid,
        ttl: Duration,
    ) -> Result<(), SessionError>;
    
    /// Mark a session as revoked
    async fn revoke_session(&self, session_hash: &str, ttl: Duration) -> Result<(), SessionError>;
    
    /// Check if a session is revoked
    async fn is_revoked(&self, session_hash: &str) -> Result<bool, SessionError>;
}

use std::collections::HashMap;
use std::sync::Mutex;

/// Simple in-memory session storage (for standalone mode)
pub struct InMemorySessionStorage {
    sessions: Mutex<HashMap<String, (Uuid, bool)>>,
}

impl InMemorySessionStorage {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemorySessionStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionStorage for InMemorySessionStorage {
    async fn store_session(
        &self,
        session_hash: &str,
        user_id: Uuid,
        _ttl: Duration,
    ) -> Result<(), SessionError> {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(session_hash.to_string(), (user_id, false));
        Ok(())
    }

    async fn revoke_session(&self, session_hash: &str, _ttl: Duration) -> Result<(), SessionError> {
        let mut sessions = self.sessions.lock().unwrap();
        // Mark as revoked if exists, or insert as revoked if missing
        sessions
            .entry(session_hash.to_string())
            .and_modify(|(_, revoked)| *revoked = true)
            .or_insert((Uuid::nil(), true));
        Ok(())
    }

    async fn is_revoked(&self, session_hash: &str) -> Result<bool, SessionError> {
        let sessions = self.sessions.lock().unwrap();
        Ok(sessions
            .get(session_hash)
            .map(|(_, revoked)| *revoked)
            .unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_config_from_env() {
        // Set required env vars
        std::env::set_var("JWT_SECRET", "test-secret");
        
        let config = SessionConfig::from_env().unwrap();
        assert_eq!(config.jwt_secret, "test-secret");
        assert!(config.use_revocation_cache);
    }

    #[test]
    fn test_session_types() {
        assert_eq!(SessionType::Web.as_str(), "web");
        assert_eq!(SessionType::Api.as_str(), "api");
        assert_eq!(SessionType::Device.as_str(), "device");
        assert_eq!(SessionType::Share.as_str(), "share");
    }

    #[tokio::test]
    async fn test_in_memory_session_storage_store_and_revoke() {
        let storage = InMemorySessionStorage::new();
        let user_id = Uuid::new_v4();

        // Store session
        storage.store_session("session1", user_id, Duration::from_secs(3600)).await.unwrap();
        assert!(!storage.is_revoked("session1").await.unwrap());

        // Revoke session
        storage.revoke_session("session1", Duration::from_secs(3600)).await.unwrap();
        assert!(storage.is_revoked("session1").await.unwrap());
    }

    #[tokio::test]
    async fn test_in_memory_session_storage_revoke_unknown_session() {
        let storage = InMemorySessionStorage::new();

        // Revoke a session that was never stored
        storage.revoke_session("unknown_session", Duration::from_secs(3600)).await.unwrap();
        assert!(storage.is_revoked("unknown_session").await.unwrap());
    }

    #[tokio::test]
    async fn test_in_memory_session_storage_multiple_sessions() {
        let storage = InMemorySessionStorage::new();
        let user_id = Uuid::new_v4();

        storage.store_session("session_a", user_id, Duration::from_secs(3600)).await.unwrap();
        storage.store_session("session_b", user_id, Duration::from_secs(3600)).await.unwrap();

        assert!(!storage.is_revoked("session_a").await.unwrap());
        assert!(!storage.is_revoked("session_b").await.unwrap());

        storage.revoke_session("session_a", Duration::from_secs(3600)).await.unwrap();

        assert!(storage.is_revoked("session_a").await.unwrap());
        assert!(!storage.is_revoked("session_b").await.unwrap());
    }
}
