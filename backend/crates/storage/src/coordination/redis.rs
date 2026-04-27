//! Redis-backed coordination store implementation
//!
//! Suitable for distributed deployments where multiple RustShare
//! instances need to coordinate. Requires Redis 6.0+ for optimal
//! operation (uses specific commands for atomic operations).

use super::*;
use chrono::Utc;
use ::redis::AsyncCommands;
use std::time::Duration;

/// Redis coordination store
pub struct RedisCoordinationStore {
    client: ::redis::Client,
    connection_manager: ::redis::aio::ConnectionManager,
    key_prefix: String,
}

impl RedisCoordinationStore {
    /// Create a new Redis coordination store
    pub async fn new(redis_url: &str) -> anyhow::Result<Self> {
        let client = ::redis::Client::open(redis_url)?;
        let connection_manager = ::redis::aio::ConnectionManager::new(client.clone()).await?;

        Ok(Self {
            client,
            connection_manager,
            key_prefix: "rustshare:coord:".to_string(),
        })
    }

    /// Create with custom key prefix
    pub async fn with_prefix(redis_url: &str, prefix: String) -> anyhow::Result<Self> {
        let client = ::redis::Client::open(redis_url)?;
        let connection_manager = ::redis::aio::ConnectionManager::new(client.clone()).await?;

        Ok(Self {
            client,
            connection_manager,
            key_prefix: prefix,
        })
    }

    fn key(&self, suffix: &str) -> String {
        format!("{}{}", self.key_prefix, suffix)
    }
}

#[async_trait]
impl CoordinationStore for RedisCoordinationStore {
    // =========================================================================
    // Locks
    // =========================================================================

    async fn acquire_lock(
        &self,
        resource_id: &str,
        owner: &str,
        ttl: Duration,
    ) -> Result<LockToken, CoordinationError> {
        let key = self.key(&format!("lock:{}", resource_id));
        let lock_id = uuid::Uuid::new_v4().to_string();
        let value = format!("{}:{}", lock_id, owner);

        let mut conn = self.connection_manager.clone();

        // Use SET with NX (only if not exists) and EX (expiration)
        let result: Option<String> = ::redis::cmd("SET")
            .arg(&key)
            .arg(&value)
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs() as usize)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        if result.is_none() {
            return Err(CoordinationError::AlreadyLocked {
                resource_id: resource_id.to_string(),
            });
        }

        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default();

        Ok(LockToken {
            lock_id,
            resource_id: resource_id.to_string(),
            expires_at,
            owner: owner.to_string(),
        })
    }

    async fn release_lock(&self, token: &LockToken) -> Result<(), CoordinationError> {
        let key = self.key(&format!("lock:{}", token.resource_id));
        let expected_value = format!("{}:{}", token.lock_id, token.owner);

        let mut conn = self.connection_manager.clone();

        // Use a Lua script to check and delete atomically
        let script = r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("del", KEYS[1])
            else
                return 0
            end
        "#;

        let result: i32 = ::redis::Script::new(script)
            .key(&key)
            .arg(&expected_value)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        if result == 0 {
            return Err(CoordinationError::LockNotHeld {
                resource_id: token.resource_id.clone(),
                lock_id: token.lock_id.clone(),
            });
        }

        Ok(())
    }

    async fn extend_lock(
        &self,
        token: &LockToken,
        additional_ttl: Duration,
    ) -> Result<LockToken, CoordinationError> {
        let key = self.key(&format!("lock:{}", token.resource_id));
        let expected_value = format!("{}:{}", token.lock_id, token.owner);
        let new_ttl_secs = additional_ttl.as_secs() as usize;

        let mut conn = self.connection_manager.clone();

        // Lua script to verify ownership and extend
        let script = r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("expire", KEYS[1], ARGV[2])
            else
                return 0
            end
        "#;

        let result: i32 = ::redis::Script::new(script)
            .key(&key)
            .arg(&expected_value)
            .arg(new_ttl_secs)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        if result == 0 {
            return Err(CoordinationError::LockNotHeld {
                resource_id: token.resource_id.clone(),
                lock_id: token.lock_id.clone(),
            });
        }

        let new_expires_at =
            Utc::now() + chrono::Duration::from_std(additional_ttl).unwrap_or_default();

        Ok(LockToken {
            lock_id: token.lock_id.clone(),
            resource_id: token.resource_id.clone(),
            expires_at: new_expires_at,
            owner: token.owner.clone(),
        })
    }

    async fn is_locked(&self, resource_id: &str) -> Result<bool, CoordinationError> {
        let key = self.key(&format!("lock:{}", resource_id));
        let mut conn = self.connection_manager.clone();

        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(exists)
    }

    // =========================================================================
    // Leases
    // =========================================================================

    async fn acquire_lease(
        &self,
        resource_id: &str,
        owner: &str,
        ttl: Duration,
    ) -> Result<LeaseInfo, CoordinationError> {
        // Leases use the same underlying mechanism as locks but with different semantics
        let key = self.key(&format!("lease:{}", resource_id));
        let value = format!("{}", owner);

        let mut conn = self.connection_manager.clone();

        let result: Option<String> = ::redis::cmd("SET")
            .arg(&key)
            .arg(&value)
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs() as usize)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        if result.is_none() {
            return Err(CoordinationError::AlreadyLocked {
                resource_id: resource_id.to_string(),
            });
        }

        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default();

        Ok(LeaseInfo {
            resource_id: resource_id.to_string(),
            owner: owner.to_string(),
            expires_at,
        })
    }

    async fn release_lease(&self, resource_id: &str, owner: &str) -> Result<(), CoordinationError> {
        let key = self.key(&format!("lease:{}", resource_id));

        let mut conn = self.connection_manager.clone();

        // Lua script to check owner and delete
        let script = r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("del", KEYS[1])
            else
                return 0
            end
        "#;

        let _: i32 = ::redis::Script::new(script)
            .key(&key)
            .arg(owner)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(())
    }

    async fn extend_lease(
        &self,
        resource_id: &str,
        owner: &str,
        additional_ttl: Duration,
    ) -> Result<LeaseInfo, CoordinationError> {
        let key = self.key(&format!("lease:{}", resource_id));
        let new_ttl_secs = additional_ttl.as_secs() as usize;

        let mut conn = self.connection_manager.clone();

        let script = r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("expire", KEYS[1], ARGV[2])
            else
                return 0
            end
        "#;

        let result: i32 = ::redis::Script::new(script)
            .key(&key)
            .arg(owner)
            .arg(new_ttl_secs)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        if result == 0 {
            return Err(CoordinationError::LockNotHeld {
                resource_id: resource_id.to_string(),
                lock_id: "lease".to_string(),
            });
        }

        let new_expires_at =
            Utc::now() + chrono::Duration::from_std(additional_ttl).unwrap_or_default();

        Ok(LeaseInfo {
            resource_id: resource_id.to_string(),
            owner: owner.to_string(),
            expires_at: new_expires_at,
        })
    }

    async fn get_lease(&self, resource_id: &str) -> Result<Option<LeaseInfo>, CoordinationError> {
        let key = self.key(&format!("lease:{}", resource_id));
        let mut conn = self.connection_manager.clone();

        let ttl: i64 = conn
            .ttl(&key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        if ttl <= 0 {
            return Ok(None);
        }

        let owner: String = conn
            .get(&key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        let expires_at = Utc::now() + chrono::Duration::seconds(ttl);

        Ok(Some(LeaseInfo {
            resource_id: resource_id.to_string(),
            owner,
            expires_at,
        }))
    }

    // =========================================================================
    // Job Coordination
    // =========================================================================

    async fn claim_job(
        &self,
        job_id: &str,
        worker_id: &str,
        ttl: Duration,
    ) -> Result<bool, CoordinationError> {
        let key = self.key(&format!("job:{}", job_id));
        let value = format!("{}", worker_id);

        let mut conn = self.connection_manager.clone();

        let result: Option<String> = ::redis::cmd("SET")
            .arg(&key)
            .arg(&value)
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs() as usize)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(result.is_some())
    }

    async fn release_job_claim(
        &self,
        job_id: &str,
        worker_id: &str,
    ) -> Result<(), CoordinationError> {
        let key = self.key(&format!("job:{}", job_id));

        let mut conn = self.connection_manager.clone();

        let script = r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("del", KEYS[1])
            else
                return 0
            end
        "#;

        let _: i32 = ::redis::Script::new(script)
            .key(&key)
            .arg(worker_id)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(())
    }

    async fn heartbeat_job(
        &self,
        job_id: &str,
        worker_id: &str,
        additional_ttl: Duration,
    ) -> Result<bool, CoordinationError> {
        let key = self.key(&format!("job:{}", job_id));
        let new_ttl_secs = additional_ttl.as_secs() as usize;

        let mut conn = self.connection_manager.clone();

        let script = r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("expire", KEYS[1], ARGV[2])
            else
                return 0
            end
        "#;

        let result: i32 = ::redis::Script::new(script)
            .key(&key)
            .arg(worker_id)
            .arg(new_ttl_secs)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(result == 1)
    }

    async fn get_job_claim(&self, job_id: &str) -> Result<Option<WorkerClaim>, CoordinationError> {
        let key = self.key(&format!("job:{}", job_id));
        let mut conn = self.connection_manager.clone();

        let ttl: i64 = conn
            .ttl(&key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        if ttl <= 0 {
            return Ok(None);
        }

        let worker_id: String = conn
            .get(&key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        let expires_at = Utc::now() + chrono::Duration::seconds(ttl);

        Ok(Some(WorkerClaim {
            job_id: job_id.to_string(),
            worker_id,
            expires_at,
        }))
    }

    // =========================================================================
    // Rate Limiting
    // =========================================================================

    async fn check_rate_limit(
        &self,
        key: &str,
        max_requests: u32,
        window: Duration,
    ) -> Result<RateLimitStatus, CoordinationError> {
        let full_key = self.key(&format!("ratelimit:{}", key));
        let window_secs = window.as_secs() as usize;
        let mut conn = self.connection_manager.clone();

        // Lua script for atomic increment and check
        let script = r#"
            local current = redis.call("INCR", KEYS[1])
            if current == 1 then
                redis.call("EXPIRE", KEYS[1], ARGV[1])
            end
            local ttl = redis.call("TTL", KEYS[1])
            return {current, ttl}
        "#;

        let result: Vec<i64> = ::redis::Script::new(script)
            .key(&full_key)
            .arg(window_secs)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        let current_count = result[0] as u32;
        let retry_after = result[1].max(0) as u64;

        if current_count > max_requests {
            Ok(RateLimitStatus::denied(
                current_count,
                max_requests,
                retry_after,
            ))
        } else {
            Ok(RateLimitStatus::allowed(current_count, max_requests))
        }
    }

    async fn reset_rate_limit(&self, key: &str) -> Result<(), CoordinationError> {
        let full_key = self.key(&format!("ratelimit:{}", key));
        let mut conn = self.connection_manager.clone();

        let _: () = conn
            .del(&full_key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(())
    }

    // =========================================================================
    // Session Management
    // =========================================================================

    async fn cache_session(
        &self,
        token_hash: &str,
        user_id: &str,
        ttl: Duration,
    ) -> Result<(), CoordinationError> {
        let key = self.key(&format!("session:{}", token_hash));
        let mut conn = self.connection_manager.clone();

        // Store as hash with user_id and revoked flag
        let _: () = conn
            .hset(&key, "user_id", user_id)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;
        let _: () = conn
            .hset(&key, "revoked", "0")
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;
        let _: () = conn
            .expire(&key, ttl.as_secs() as usize)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(())
    }

    async fn revoke_session(
        &self,
        token_hash: &str,
        ttl: Duration,
    ) -> Result<(), CoordinationError> {
        let key = self.key(&format!("session:{}", token_hash));
        let mut conn = self.connection_manager.clone();

        // Mark as revoked - either update existing or create new entry
        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        if exists {
            let _: () = conn
                .hset(&key, "revoked", "1")
                .await
                .map_err(|e| CoordinationError::BackendError(e.to_string()))?;
            // Extend TTL to ensure revocation persists
            let _: () = conn
                .expire(&key, ttl.as_secs() as usize)
                .await
                .map_err(|e| CoordinationError::BackendError(e.to_string()))?;
        } else {
            // Create a revoked entry
            let _: () = conn
                .hset(&key, "user_id", "")
                .await
                .map_err(|e| CoordinationError::BackendError(e.to_string()))?;
            let _: () = conn
                .hset(&key, "revoked", "1")
                .await
                .map_err(|e| CoordinationError::BackendError(e.to_string()))?;
            let _: () = conn
                .expire(&key, ttl.as_secs() as usize)
                .await
                .map_err(|e| CoordinationError::BackendError(e.to_string()))?;
        }

        Ok(())
    }

    async fn is_session_valid(&self, token_hash: &str) -> Result<bool, CoordinationError> {
        let key = self.key(&format!("session:{}", token_hash));
        let mut conn = self.connection_manager.clone();

        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        if !exists {
            // Session not in cache - valid if not explicitly revoked (stateless fallback)
            return Ok(true);
        }

        let revoked: String = conn
            .hget(&key, "revoked")
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(revoked == "0")
    }

    async fn get_cached_session(
        &self,
        token_hash: &str,
    ) -> Result<Option<CachedSession>, CoordinationError> {
        let key = self.key(&format!("session:{}", token_hash));
        let mut conn = self.connection_manager.clone();

        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        if !exists {
            return Ok(None);
        }

        let user_id: String = conn
            .hget(&key, "user_id")
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;
        let revoked: String = conn
            .hget(&key, "revoked")
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;
        let ttl: i64 = conn
            .ttl(&key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        let expires_at = if ttl > 0 {
            Utc::now() + chrono::Duration::seconds(ttl)
        } else {
            Utc::now()
        };

        Ok(Some(CachedSession {
            token_hash: token_hash.to_string(),
            user_id,
            expires_at,
            revoked: revoked == "1",
        }))
    }

    // =========================================================================
    // Idempotency
    // =========================================================================

    async fn check_idempotency_key(
        &self,
        key: &str,
        ttl: Duration,
    ) -> Result<bool, CoordinationError> {
        let full_key = self.key(&format!("idempotency:{}", key));
        let mut conn = self.connection_manager.clone();

        let result: Option<String> = ::redis::cmd("SET")
            .arg(&full_key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs() as usize)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(result.is_some())
    }

    // =========================================================================
    // Presence
    // =========================================================================

    async fn mark_user_connected(
        &self,
        user_id: &str,
        connection_id: &str,
        ttl: Duration,
    ) -> Result<(), CoordinationError> {
        let key = self.key(&format!("presence:{}", user_id));
        let mut conn = self.connection_manager.clone();

        // Use a hash to store connection IDs with their expiration
        // Also set expiration on the key itself
        let _: () = conn
            .hset(&key, connection_id, "1")
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;
        let _: () = conn
            .expire(&key, ttl.as_secs() as usize)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(())
    }

    async fn mark_user_disconnected(
        &self,
        user_id: &str,
        connection_id: &str,
    ) -> Result<(), CoordinationError> {
        let key = self.key(&format!("presence:{}", user_id));
        let mut conn = self.connection_manager.clone();

        let _: () = conn
            .hdel(&key, connection_id)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(())
    }

    async fn get_user_connections(&self, user_id: &str) -> Result<Vec<String>, CoordinationError> {
        let key = self.key(&format!("presence:{}", user_id));
        let mut conn = self.connection_manager.clone();

        let connections: Vec<String> = conn
            .hkeys(&key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(connections)
    }

    async fn is_user_online(&self, user_id: &str) -> Result<bool, CoordinationError> {
        let key = self.key(&format!("presence:{}", user_id));
        let mut conn = self.connection_manager.clone();

        let count: i32 = conn
            .hlen(&key)
            .await
            .map_err(|e| CoordinationError::BackendError(e.to_string()))?;

        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running Redis instance
    // They are marked as ignored by default to avoid CI failures

    #[tokio::test]
    #[ignore = "requires Redis"]
    async fn test_redis_lock_operations() {
        let store = RedisCoordinationStore::new("redis://127.0.0.1:6379")
            .await
            .expect("Failed to connect to Redis");

        // Acquire lock
        let lock = store
            .acquire_lock("test-resource", "owner1", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(lock.is_valid());

        // Duplicate should fail
        let result = store
            .acquire_lock("test-resource", "owner2", Duration::from_secs(60))
            .await;
        assert!(matches!(
            result,
            Err(CoordinationError::AlreadyLocked { .. })
        ));

        // Release
        store.release_lock(&lock).await.unwrap();

        // Now can acquire
        let lock2 = store
            .acquire_lock("test-resource", "owner2", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(lock2.is_valid());
    }

    #[tokio::test]
    #[ignore = "requires Redis"]
    async fn test_redis_job_claiming() {
        let store = RedisCoordinationStore::new("redis://127.0.0.1:6379")
            .await
            .expect("Failed to connect to Redis");

        // Claim job
        let claimed = store
            .claim_job("test-job", "worker1", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(claimed);

        // Second claim should fail
        let claimed = store
            .claim_job("test-job", "worker2", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(!claimed);

        // Release
        store
            .release_job_claim("test-job", "worker1")
            .await
            .unwrap();

        // Now can claim
        let claimed = store
            .claim_job("test-job", "worker2", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(claimed);
    }
}
