//! Coordination Store Implementation
//!
//! Provides ephemeral runtime coordination including:
//! - Distributed leases
//! - Event fanout
//! - Rate limiting
//!
//! Redis is optional - core operations work without it.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Lease for distributed coordination
#[derive(Debug, Clone)]
pub struct Lease {
    pub key: String,
    pub token: String,
    pub is_dummy: bool,
}

impl Lease {
    /// Create a real lease
    pub fn new(key: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            token: token.into(),
            is_dummy: false,
        }
    }

    /// Create a dummy lease (for when Redis is unavailable)
    pub fn dummy(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            token: "dummy".to_string(),
            is_dummy: true,
        }
    }

    /// Check if this is a dummy lease
    pub fn is_dummy_lease(&self) -> bool {
        self.is_dummy
    }
}

/// Coordination store for ephemeral runtime state
#[async_trait]
pub trait CoordinationStore: Send + Sync {
    /// Acquire a lease for the given key
    /// Returns a lease if successful, or an error if the key is already leased
    async fn acquire_lease(&self, key: &str, ttl_secs: u64) -> Result<Lease>;

    /// Release a previously acquired lease
    async fn release_lease(&self, lease: Lease) -> Result<()>;

    /// Publish a message to a channel
    async fn publish(&self, channel: &str, message: &str) -> Result<()>;

    /// Check rate limit for a key
    /// Returns true if the request is allowed, false if rate limited
    async fn check_rate_limit(&self, key: &str, max_requests: u32, window_secs: u64)
        -> Result<bool>;

    /// Check if coordination is available
    fn is_available(&self) -> bool;

    /// Claim a job for processing
    /// Returns true if the job was claimed, false if already claimed by another worker
    async fn claim_job(&self, job_id: &str, worker_id: &str, ttl: Duration) -> Result<bool>;

    /// Release a job claim
    async fn release_job_claim(&self, job_id: &str, worker_id: &str) -> Result<()>;

    /// Send a heartbeat for a job claim to extend its TTL
    /// Returns true if heartbeat was successful, false if claim was lost
    async fn heartbeat_job(&self, job_id: &str, worker_id: &str, ttl: Duration) -> Result<bool>;

    /// Get the last heartbeat time for a job
    async fn get_job_heartbeat(&self, job_id: &str) -> Result<Option<DateTime<Utc>>>;
}

/// Memory-based coordination store for testing and single-node deployments
pub struct MemoryCoordinationStore {
    available: bool,
}

impl MemoryCoordinationStore {
    /// Create a new memory coordination store
    pub fn new() -> Self {
        Self { available: true }
    }

    /// Create an unavailable store (simulating Redis loss)
    pub fn unavailable() -> Self {
        Self { available: false }
    }

    /// Mark as unavailable
    pub fn set_unavailable(&mut self) {
        self.available = false;
    }
}

impl Default for MemoryCoordinationStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CoordinationStore for MemoryCoordinationStore {
    async fn acquire_lease(&self, key: &str, _ttl_secs: u64) -> Result<Lease> {
        if !self.available {
            return Ok(Lease::dummy(key));
        }
        // Memory store always grants leases (single-node behavior)
        Ok(Lease::new(key, uuid::Uuid::new_v4().to_string()))
    }

    async fn release_lease(&self, _lease: Lease) -> Result<()> {
        // No-op for memory store
        Ok(())
    }

    async fn publish(&self, _channel: &str, _message: &str) -> Result<()> {
        // No-op for memory store
        Ok(())
    }

    async fn check_rate_limit(
        &self,
        _key: &str,
        _max_requests: u32,
        _window_secs: u64,
    ) -> Result<bool> {
        // Memory store always allows
        Ok(true)
    }

    fn is_available(&self) -> bool {
        self.available
    }

    async fn claim_job(&self, _job_id: &str, _worker_id: &str, _ttl: Duration) -> Result<bool> {
        // Memory store always allows claims (single-node behavior)
        Ok(true)
    }

    async fn release_job_claim(&self, _job_id: &str, _worker_id: &str) -> Result<()> {
        // No-op for memory store
        Ok(())
    }

    async fn heartbeat_job(&self, _job_id: &str, _worker_id: &str, _ttl: Duration) -> Result<bool> {
        // Memory store always returns true for heartbeat (single-node behavior)
        Ok(true)
    }

    async fn get_job_heartbeat(&self, _job_id: &str) -> Result<Option<DateTime<Utc>>> {
        // Memory store doesn't track heartbeats
        Ok(None)
    }
}

/// Redis-based coordination store for distributed deployments
#[cfg(feature = "redis")]
pub struct RedisCoordinationStore {
    client: redis::aio::MultiplexedConnection,
}

#[cfg(feature = "redis")]
impl RedisCoordinationStore {
    /// Create a new Redis coordination store
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self { client: conn })
    }

    /// Create from environment
    pub async fn from_env() -> Result<Self> {
        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());
        Self::new(&redis_url).await
    }
}

#[cfg(feature = "redis")]
#[async_trait]
impl CoordinationStore for RedisCoordinationStore {
    async fn acquire_lease(&self, key: &str, ttl_secs: u64) -> Result<Lease> {
        use redis::AsyncCommands;

        let token = uuid::Uuid::new_v4().to_string();
        let lease_key = format!("lease:{}", key);

        // Try to set with NX (only if not exists) and EX (expiration)
        let result: Option<String> = redis::cmd("SET")
            .arg(&lease_key)
            .arg(&token)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut self.client.clone())
            .await?;

        match result {
            Some(_) => Ok(Lease::new(key, token)),
            None => Err(anyhow::anyhow!("Lease already held for key: {}", key)),
        }
    }

    async fn release_lease(&self, lease: Lease) -> Result<()> {
        use redis::AsyncCommands;

        let lease_key = format!("lease:{}", lease.key);

        // Only delete if the token matches (we still hold the lease)
        let script = redis::Script::new(
            r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("del", KEYS[1])
            else
                return 0
            end
            "#,
        );

        script
            .key(&lease_key)
            .arg(&lease.token)
            .invoke_async(&mut self.client.clone())
            .await?;

        Ok(())
    }

    async fn publish(&self, channel: &str, message: &str) -> Result<()> {
        use redis::AsyncCommands;

        let _: () = self
            .client
            .clone()
            .publish(channel, message)
            .await?;

        Ok(())
    }

    async fn check_rate_limit(
        &self,
        key: &str,
        max_requests: u32,
        window_secs: u64,
    ) -> Result<bool> {
        use redis::AsyncCommands;

        let rate_key = format!("rate_limit:{}", key);
        let window = window_secs as i64;

        let script = redis::Script::new(
            r#"
            local current = redis.call("GET", KEYS[1])
            if current == false then
                current = 0
            else
                current = tonumber(current)
            end
            
            if current >= tonumber(ARGV[1]) then
                return 0
            end
            
            redis.call("INCR", KEYS[1])
            redis.call("EXPIRE", KEYS[1], ARGV[2])
            return 1
            "#,
        );

        let allowed: i32 = script
            .key(&rate_key)
            .arg(max_requests)
            .arg(window)
            .invoke_async(&mut self.client.clone())
            .await?;

        Ok(allowed == 1)
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn claim_job(&self, job_id: &str, worker_id: &str, ttl: Duration) -> Result<bool> {
        use redis::AsyncCommands;

        let job_key = format!("job_claim:{}", job_id);
        let result: Option<String> = redis::cmd("SET")
            .arg(&job_key)
            .arg(worker_id)
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs())
            .query_async(&mut self.client.clone())
            .await?;

        Ok(result.is_some())
    }

    async fn release_job_claim(&self, job_id: &str, _worker_id: &str) -> Result<()> {
        use redis::AsyncCommands;

        let job_key = format!("job_claim:{}", job_id);
        let _: () = self.client.clone().del(&job_key).await?;
        Ok(())
    }

    async fn heartbeat_job(&self, job_id: &str, worker_id: &str, ttl: Duration) -> Result<bool> {
        use redis::AsyncCommands;

        let job_key = format!("job_claim:{}", job_id);
        let current: Option<String> = self.client.clone().get(&job_key).await?;

        // Only extend if we still hold the claim
        if current.as_ref() == Some(&worker_id.to_string()) {
            let _: () = self
                .client
                .clone()
                .expire(&job_key, ttl.as_secs() as i64)
                .await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn get_job_heartbeat(&self, job_id: &str) -> Result<Option<DateTime<Utc>>> {
        use redis::AsyncCommands;

        let job_key = format!("job_claim:{}", job_id);
        let ttl: i64 = self.client.clone().ttl(&job_key).await?;

        if ttl > 0 {
            // Job is active, return current time as approximate heartbeat
            Ok(Some(Utc::now()))
        } else {
            Ok(None)
        }
    }
}

/// Factory for creating coordination stores
pub struct CoordinationStoreFactory;

impl CoordinationStoreFactory {
    /// Create a memory-based store (no Redis dependency)
    pub fn create_memory() -> Arc<dyn CoordinationStore> {
        Arc::new(MemoryCoordinationStore::new())
    }

    /// Create a memory-based store marked as unavailable (for testing Redis loss)
    pub fn create_unavailable() -> Arc<dyn CoordinationStore> {
        Arc::new(MemoryCoordinationStore::unavailable())
    }

    /// Create from environment (uses Redis if available, otherwise memory)
    pub async fn from_env() -> Arc<dyn CoordinationStore> {
        #[cfg(feature = "redis")]
        {
            if let Ok(redis_url) = std::env::var("REDIS_URL") {
                match RedisCoordinationStore::new(&redis_url).await {
                    Ok(store) => return Arc::new(store),
                    Err(e) => {
                        tracing::warn!("Failed to connect to Redis: {}, using memory store", e);
                    }
                }
            }
        }

        Arc::new(MemoryCoordinationStore::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_coordination() {
        let store = MemoryCoordinationStore::new();

        // Acquire lease
        let lease = store.acquire_lease("test-key", 30).await.unwrap();
        assert!(!lease.is_dummy_lease());
        assert_eq!(lease.key, "test-key");

        // Release lease
        store.release_lease(lease).await.unwrap();

        // Rate limit always allows
        assert!(store.check_rate_limit("key", 10, 60).await.unwrap());

        // Publish no-op
        store.publish("channel", "message").await.unwrap();

        // Available
        assert!(store.is_available());
    }

    #[tokio::test]
    async fn test_unavailable_coordination() {
        let store = MemoryCoordinationStore::unavailable();

        // Acquire returns dummy lease
        let lease = store.acquire_lease("test-key", 30).await.unwrap();
        assert!(lease.is_dummy_lease());

        // Not available
        assert!(!store.is_available());
    }
}
