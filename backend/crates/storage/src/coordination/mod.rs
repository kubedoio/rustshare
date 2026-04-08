//! Coordination store abstraction
//!
//! Provides a unified interface for ephemeral coordination primitives:
//! - Distributed locks and leases
//! - Worker claim coordination
//! - Rate limiting counters
//! - Session revocation cache
//! - Idempotency keys
//!
//! Two implementations:
//! - InMemoryCoordinationStore: For standalone deployments
//! - RedisCoordinationStore: For distributed deployments

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::time::Duration;
use thiserror::Error;

pub mod memory;
#[cfg(feature = "redis-coordination")]
pub mod redis;

pub use memory::InMemoryCoordinationStore;
#[cfg(feature = "redis-coordination")]
pub use redis::RedisCoordinationStore;

/// Errors that can occur during coordination operations
#[derive(Debug, Error, Clone)]
pub enum CoordinationError {
    #[error("Resource {resource_id} is already locked")]
    AlreadyLocked { resource_id: String },

    #[error("Lock {lock_id} for {resource_id} is not held or expired")]
    LockNotHeld {
        resource_id: String,
        lock_id: String,
    },

    #[error("Coordination timeout for {resource_id}")]
    Timeout { resource_id: String },

    #[error("Coordination backend error: {0}")]
    BackendError(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Lock token representing a held lock
#[derive(Debug, Clone)]
pub struct LockToken {
    /// Unique lock identifier
    pub lock_id: String,
    /// Resource being locked
    pub resource_id: String,
    /// Lock expiration time
    pub expires_at: DateTime<Utc>,
    /// Owner identifier (for debugging)
    pub owner: String,
}

impl LockToken {
    /// Check if the lock is still valid
    pub fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at
    }
}

/// Lease information for a resource
#[derive(Debug, Clone)]
pub struct LeaseInfo {
    /// Resource being leased
    pub resource_id: String,
    /// Current lease holder
    pub owner: String,
    /// Lease expiration time
    pub expires_at: DateTime<Utc>,
}

/// Worker claim for job processing
#[derive(Debug, Clone)]
pub struct WorkerClaim {
    /// Job ID being claimed
    pub job_id: String,
    /// Worker instance ID
    pub worker_id: String,
    /// Claim expiration time
    pub expires_at: DateTime<Utc>,
}

/// Session entry in the revocation cache
#[derive(Debug, Clone)]
pub struct CachedSession {
    /// Session token hash
    pub token_hash: String,
    /// User ID
    pub user_id: String,
    /// Session expiration
    pub expires_at: DateTime<Utc>,
    /// Whether session was explicitly revoked
    pub revoked: bool,
}

/// Core trait for coordination primitives
///
/// This trait provides ephemeral coordination services that are NOT
/// canonical truth. All state stored through this trait is reconstructible
/// or safely discardable.
#[async_trait]
pub trait CoordinationStore: Send + Sync {
    // =========================================================================
    // Locks (short-term mutual exclusion)
    // =========================================================================

    /// Acquire a distributed lock on a resource
    ///
    /// Returns a LockToken if successful. The lock will expire after `ttl`
    /// if not released or extended.
    ///
    /// # Errors
    /// - AlreadyLocked if another holder has the lock
    /// - Timeout if lock cannot be acquired within timeout
    async fn acquire_lock(
        &self,
        resource_id: &str,
        owner: &str,
        ttl: Duration,
    ) -> Result<LockToken, CoordinationError>;

    /// Release a lock
    ///
    /// # Errors
    /// - LockNotHeld if the lock is not held by this token
    async fn release_lock(&self, token: &LockToken) -> Result<(), CoordinationError>;

    /// Extend a lock's TTL
    ///
    /// # Errors
    /// - LockNotHeld if the lock is not held by this token
    async fn extend_lock(
        &self,
        token: &LockToken,
        additional_ttl: Duration,
    ) -> Result<LockToken, CoordinationError>;

    /// Check if a resource is currently locked
    async fn is_locked(&self, resource_id: &str) -> Result<bool, CoordinationError>;

    // =========================================================================
    // Leases (longer-term resource claims)
    // =========================================================================

    /// Acquire a lease on a resource
    ///
    /// Similar to locks but intended for longer durations (e.g., job processing).
    async fn acquire_lease(
        &self,
        resource_id: &str,
        owner: &str,
        ttl: Duration,
    ) -> Result<LeaseInfo, CoordinationError>;

    /// Release a lease
    async fn release_lease(&self, resource_id: &str, owner: &str) -> Result<(), CoordinationError>;

    /// Extend a lease
    async fn extend_lease(
        &self,
        resource_id: &str,
        owner: &str,
        additional_ttl: Duration,
    ) -> Result<LeaseInfo, CoordinationError>;

    /// Get current lease info for a resource
    async fn get_lease(&self, resource_id: &str) -> Result<Option<LeaseInfo>, CoordinationError>;

    // =========================================================================
    // Job Coordination
    // =========================================================================

    /// Claim a job for processing
    ///
    /// Atomically checks if job is available and claims it.
    /// Returns true if claim was successful.
    async fn claim_job(
        &self,
        job_id: &str,
        worker_id: &str,
        ttl: Duration,
    ) -> Result<bool, CoordinationError>;

    /// Release a job claim
    async fn release_job_claim(
        &self,
        job_id: &str,
        worker_id: &str,
    ) -> Result<(), CoordinationError>;

    /// Extend a job claim (heartbeat)
    async fn heartbeat_job(
        &self,
        job_id: &str,
        worker_id: &str,
        additional_ttl: Duration,
    ) -> Result<bool, CoordinationError>;

    /// Check if a job is claimed and by whom
    async fn get_job_claim(&self, job_id: &str) -> Result<Option<WorkerClaim>, CoordinationError>;

    // =========================================================================
    // Rate Limiting
    // =========================================================================

    /// Check and increment a rate limit counter
    ///
    /// Returns the current count after incrementing.
    /// The counter will expire after `window`.
    async fn check_rate_limit(
        &self,
        key: &str,
        max_requests: u32,
        window: Duration,
    ) -> Result<RateLimitStatus, CoordinationError>;

    /// Reset a rate limit counter
    async fn reset_rate_limit(&self, key: &str) -> Result<(), CoordinationError>;

    // =========================================================================
    // Session Management
    // =========================================================================

    /// Add a session to the active session cache
    async fn cache_session(
        &self,
        token_hash: &str,
        user_id: &str,
        ttl: Duration,
    ) -> Result<(), CoordinationError>;

    /// Mark a session as revoked
    async fn revoke_session(
        &self,
        token_hash: &str,
        ttl: Duration,
    ) -> Result<(), CoordinationError>;

    /// Check if a session is valid (exists and not revoked)
    async fn is_session_valid(&self, token_hash: &str) -> Result<bool, CoordinationError>;

    /// Get cached session info
    async fn get_cached_session(
        &self,
        token_hash: &str,
    ) -> Result<Option<CachedSession>, CoordinationError>;

    // =========================================================================
    // Idempotency
    // =========================================================================

    /// Check if an idempotency key has been used
    ///
    /// Returns true if the key is new and was recorded, false if already seen.
    async fn check_idempotency_key(
        &self,
        key: &str,
        ttl: Duration,
    ) -> Result<bool, CoordinationError>;

    // =========================================================================
    // Presence / WebSocket
    // =========================================================================

    /// Mark a user as connected
    async fn mark_user_connected(
        &self,
        user_id: &str,
        connection_id: &str,
        ttl: Duration,
    ) -> Result<(), CoordinationError>;

    /// Mark a user as disconnected
    async fn mark_user_disconnected(
        &self,
        user_id: &str,
        connection_id: &str,
    ) -> Result<(), CoordinationError>;

    /// Get active connections for a user
    async fn get_user_connections(&self, user_id: &str) -> Result<Vec<String>, CoordinationError>;

    /// Check if a user has any active connections
    async fn is_user_online(&self, user_id: &str) -> Result<bool, CoordinationError>;
}

/// Status of a rate limit check
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitStatus {
    /// Number of requests in current window
    pub current_count: u32,
    /// Maximum allowed requests
    pub max_requests: u32,
    /// Whether the request is allowed
    pub allowed: bool,
    /// Seconds until the window resets
    pub retry_after: Option<u64>,
}

impl RateLimitStatus {
    /// Create an allowed status
    pub fn allowed(current_count: u32, max_requests: u32) -> Self {
        Self {
            current_count,
            max_requests,
            allowed: true,
            retry_after: None,
        }
    }

    /// Create a denied status
    pub fn denied(current_count: u32, max_requests: u32, retry_after: u64) -> Self {
        Self {
            current_count,
            max_requests,
            allowed: false,
            retry_after: Some(retry_after),
        }
    }
}

/// Factory for creating coordination stores based on configuration
pub struct CoordinationStoreFactory;

impl CoordinationStoreFactory {
    /// Create an in-memory coordination store
    pub fn create_memory() -> Box<dyn CoordinationStore> {
        Box::new(InMemoryCoordinationStore::new())
    }

    /// Create a Redis coordination store
    #[cfg(feature = "redis-coordination")]
    pub async fn create_redis(
        redis_url: &str,
    ) -> Result<Box<dyn CoordinationStore>, CoordinationError> {
        let store = RedisCoordinationStore::new(redis_url)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;
        Ok(Box::new(store))
    }

    /// Create coordination store based on configuration
    pub async fn create(
        use_redis: bool,
        _redis_url: Option<&str>,
    ) -> Result<Box<dyn CoordinationStore>, CoordinationError> {
        if use_redis {
            #[cfg(feature = "redis-coordination")]
            {
                let url = redis_url.ok_or_else(|| {
                    CoordinationError::InvalidConfig(
                        "Redis URL required when Redis coordination is enabled".to_string(),
                    )
                })?;
                Self::create_redis(url).await
            }
            #[cfg(not(feature = "redis-coordination"))]
            {
                Err(CoordinationError::InvalidConfig(
                    "Redis coordination requested but redis-coordination feature not enabled"
                        .to_string(),
                ))
            }
        } else {
            Ok(Self::create_memory())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_coordination() {
        let coord = InMemoryCoordinationStore::new();

        // Test lock acquisition
        let lock = coord
            .acquire_lock("resource1", "owner1", Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(lock.resource_id, "resource1");
        assert!(lock.is_valid());

        // Test duplicate lock fails
        let result = coord
            .acquire_lock("resource1", "owner2", Duration::from_secs(60))
            .await;
        assert!(matches!(
            result,
            Err(CoordinationError::AlreadyLocked { .. })
        ));

        // Test lock release
        coord.release_lock(&lock).await.unwrap();

        // Test lock can be acquired after release
        let lock2 = coord
            .acquire_lock("resource1", "owner2", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(lock2.is_valid());
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let coord = InMemoryCoordinationStore::new();

        // First 5 requests should be allowed
        for i in 1..=5 {
            let status = coord
                .check_rate_limit("key1", 5, Duration::from_secs(60))
                .await
                .unwrap();
            assert!(status.allowed);
            assert_eq!(status.current_count, i);
        }

        // 6th request should be denied
        let status = coord
            .check_rate_limit("key1", 5, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!status.allowed);
        assert_eq!(status.current_count, 6);
        assert!(status.retry_after.is_some());
    }

    #[tokio::test]
    async fn test_session_cache() {
        let coord = InMemoryCoordinationStore::new();

        // Cache a session
        coord
            .cache_session("token1", "user1", Duration::from_secs(3600))
            .await
            .unwrap();

        // Check session is valid
        assert!(coord.is_session_valid("token1").await.unwrap());

        // Revoke session
        coord
            .revoke_session("token1", Duration::from_secs(3600))
            .await
            .unwrap();

        // Check session is no longer valid
        assert!(!coord.is_session_valid("token1").await.unwrap());
    }

    #[tokio::test]
    async fn test_job_claiming() {
        let coord = InMemoryCoordinationStore::new();

        // Claim a job
        let claimed = coord
            .claim_job("job1", "worker1", Duration::from_secs(300))
            .await
            .unwrap();
        assert!(claimed);

        // Try to claim same job with different worker
        let claimed = coord
            .claim_job("job1", "worker2", Duration::from_secs(300))
            .await
            .unwrap();
        assert!(!claimed);

        // Check claim
        let claim = coord.get_job_claim("job1").await.unwrap();
        assert!(claim.is_some());
        assert_eq!(claim.unwrap().worker_id, "worker1");

        // Release claim
        coord.release_job_claim("job1", "worker1").await.unwrap();

        // Now another worker can claim
        let claimed = coord
            .claim_job("job1", "worker2", Duration::from_secs(300))
            .await
            .unwrap();
        assert!(claimed);
    }

    #[tokio::test]
    async fn test_idempotency_keys() {
        let coord = InMemoryCoordinationStore::new();

        // First check should return true (new key)
        let is_new = coord
            .check_idempotency_key("key1", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(is_new);

        // Second check should return false (already seen)
        let is_new = coord
            .check_idempotency_key("key1", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!is_new);

        // Different key should return true
        let is_new = coord
            .check_idempotency_key("key2", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(is_new);
    }

    #[tokio::test]
    async fn test_presence() {
        let coord = InMemoryCoordinationStore::new();

        // Mark user connected
        coord
            .mark_user_connected("user1", "conn1", Duration::from_secs(60))
            .await
            .unwrap();
        coord
            .mark_user_connected("user1", "conn2", Duration::from_secs(60))
            .await
            .unwrap();

        // Check user is online
        assert!(coord.is_user_online("user1").await.unwrap());

        // Get connections
        let conns = coord.get_user_connections("user1").await.unwrap();
        assert_eq!(conns.len(), 2);

        // Disconnect one
        coord
            .mark_user_disconnected("user1", "conn1")
            .await
            .unwrap();

        // Still online
        assert!(coord.is_user_online("user1").await.unwrap());

        // Disconnect other
        coord
            .mark_user_disconnected("user1", "conn2")
            .await
            .unwrap();

        // Now offline
        assert!(!coord.is_user_online("user1").await.unwrap());
    }
}
