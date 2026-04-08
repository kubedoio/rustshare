//! In-memory coordination store implementation
//!
//! Suitable for standalone deployments where all state can be held
//! in process memory. State is lost on process restart (by design -
//! all state is ephemeral).

use super::*;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// In-memory coordination store
pub struct InMemoryCoordinationStore {
    inner: Arc<RwLock<StoreState>>,
}

struct StoreState {
    locks: HashMap<String, LockEntry>,
    leases: HashMap<String, LeaseEntry>,
    job_claims: HashMap<String, JobClaimEntry>,
    rate_limits: HashMap<String, RateLimitEntry>,
    sessions: HashMap<String, SessionEntry>,
    idempotency_keys: HashMap<String, IdempotencyEntry>,
    presence: HashMap<String, UserPresence>,
}

struct LockEntry {
    lock_id: String,
    expires_at: DateTime<Utc>,
}

struct LeaseEntry {
    owner: String,
    expires_at: DateTime<Utc>,
}

struct JobClaimEntry {
    worker_id: String,
    expires_at: DateTime<Utc>,
}

struct RateLimitEntry {
    count: u32,
    window_start: DateTime<Utc>,
    window_duration: Duration,
}

struct SessionEntry {
    user_id: String,
    expires_at: DateTime<Utc>,
    revoked: bool,
}

struct IdempotencyEntry {
    created_at: DateTime<Utc>,
    ttl: Duration,
}

struct UserPresence {
    connections: HashMap<String, DateTime<Utc>>, // connection_id -> expires_at
}

impl InMemoryCoordinationStore {
    /// Create a new in-memory coordination store
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StoreState {
                locks: HashMap::new(),
                leases: HashMap::new(),
                job_claims: HashMap::new(),
                rate_limits: HashMap::new(),
                sessions: HashMap::new(),
                idempotency_keys: HashMap::new(),
                presence: HashMap::new(),
            })),
        }
    }

    /// Clean up expired entries
    async fn cleanup_expired(&self) {
        let now = Utc::now();
        let mut state = self.inner.write().await;

        state.locks.retain(|_, entry| entry.expires_at > now);
        state.leases.retain(|_, entry| entry.expires_at > now);
        state.job_claims.retain(|_, entry| entry.expires_at > now);
        state.rate_limits.retain(|_, entry| {
            entry.window_start
                + chrono::Duration::from_std(entry.window_duration).unwrap_or_default()
                > now
        });
        state.sessions.retain(|_, entry| entry.expires_at > now);
        state.idempotency_keys.retain(|_, entry| {
            entry.created_at + chrono::Duration::from_std(entry.ttl).unwrap_or_default() > now
        });
        state.presence.retain(|_, presence| {
            presence
                .connections
                .retain(|_, expires_at| *expires_at > now);
            !presence.connections.is_empty()
        });
    }
}

impl Default for InMemoryCoordinationStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CoordinationStore for InMemoryCoordinationStore {
    // =========================================================================
    // Locks
    // =========================================================================

    async fn acquire_lock(
        &self,
        resource_id: &str,
        owner: &str,
        ttl: Duration,
    ) -> Result<LockToken, CoordinationError> {
        self.cleanup_expired().await;

        let mut state = self.inner.write().await;

        // Check if already locked
        if let Some(existing) = state.locks.get(resource_id) {
            if existing.expires_at > Utc::now() {
                return Err(CoordinationError::AlreadyLocked {
                    resource_id: resource_id.to_string(),
                });
            }
        }

        let lock_id = Uuid::new_v4().to_string();
        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default();

        state.locks.insert(
            resource_id.to_string(),
            LockEntry {
                lock_id: lock_id.clone(),
                expires_at,
            },
        );

        Ok(LockToken {
            lock_id,
            resource_id: resource_id.to_string(),
            expires_at,
            owner: owner.to_string(),
        })
    }

    async fn release_lock(&self, token: &LockToken) -> Result<(), CoordinationError> {
        self.cleanup_expired().await;

        let mut state = self.inner.write().await;

        if let Some(entry) = state.locks.get(&token.resource_id) {
            if entry.lock_id == token.lock_id {
                state.locks.remove(&token.resource_id);
                return Ok(());
            }
        }

        Err(CoordinationError::LockNotHeld {
            resource_id: token.resource_id.clone(),
            lock_id: token.lock_id.clone(),
        })
    }

    async fn extend_lock(
        &self,
        token: &LockToken,
        additional_ttl: Duration,
    ) -> Result<LockToken, CoordinationError> {
        self.cleanup_expired().await;

        let mut state = self.inner.write().await;

        if let Some(entry) = state.locks.get_mut(&token.resource_id) {
            if entry.lock_id == token.lock_id {
                let new_expires_at =
                    Utc::now() + chrono::Duration::from_std(additional_ttl).unwrap_or_default();
                entry.expires_at = new_expires_at;

                return Ok(LockToken {
                    lock_id: token.lock_id.clone(),
                    resource_id: token.resource_id.clone(),
                    expires_at: new_expires_at,
                    owner: token.owner.clone(),
                });
            }
        }

        Err(CoordinationError::LockNotHeld {
            resource_id: token.resource_id.clone(),
            lock_id: token.lock_id.clone(),
        })
    }

    async fn is_locked(&self, resource_id: &str) -> Result<bool, CoordinationError> {
        self.cleanup_expired().await;

        let state = self.inner.read().await;

        if let Some(entry) = state.locks.get(resource_id) {
            Ok(entry.expires_at > Utc::now())
        } else {
            Ok(false)
        }
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
        self.cleanup_expired().await;

        let mut state = self.inner.write().await;

        // Check if already leased
        if let Some(existing) = state.leases.get(resource_id) {
            if existing.expires_at > Utc::now() {
                return Err(CoordinationError::AlreadyLocked {
                    resource_id: resource_id.to_string(),
                });
            }
        }

        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default();

        state.leases.insert(
            resource_id.to_string(),
            LeaseEntry {
                owner: owner.to_string(),
                expires_at,
            },
        );

        Ok(LeaseInfo {
            resource_id: resource_id.to_string(),
            owner: owner.to_string(),
            expires_at,
        })
    }

    async fn release_lease(&self, resource_id: &str, owner: &str) -> Result<(), CoordinationError> {
        self.cleanup_expired().await;

        let mut state = self.inner.write().await;

        if let Some(entry) = state.leases.get(resource_id) {
            if entry.owner == owner {
                state.leases.remove(resource_id);
                return Ok(());
            }
        }

        // Lease doesn't exist or not owned by this owner - consider it released
        Ok(())
    }

    async fn extend_lease(
        &self,
        resource_id: &str,
        owner: &str,
        additional_ttl: Duration,
    ) -> Result<LeaseInfo, CoordinationError> {
        self.cleanup_expired().await;

        let mut state = self.inner.write().await;

        if let Some(entry) = state.leases.get_mut(resource_id) {
            if entry.owner == owner {
                let new_expires_at =
                    Utc::now() + chrono::Duration::from_std(additional_ttl).unwrap_or_default();
                entry.expires_at = new_expires_at;

                return Ok(LeaseInfo {
                    resource_id: resource_id.to_string(),
                    owner: owner.to_string(),
                    expires_at: new_expires_at,
                });
            }
        }

        Err(CoordinationError::LockNotHeld {
            resource_id: resource_id.to_string(),
            lock_id: "lease".to_string(),
        })
    }

    async fn get_lease(&self, resource_id: &str) -> Result<Option<LeaseInfo>, CoordinationError> {
        self.cleanup_expired().await;

        let state = self.inner.read().await;

        if let Some(entry) = state.leases.get(resource_id) {
            if entry.expires_at > Utc::now() {
                return Ok(Some(LeaseInfo {
                    resource_id: resource_id.to_string(),
                    owner: entry.owner.clone(),
                    expires_at: entry.expires_at,
                }));
            }
        }

        Ok(None)
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
        self.cleanup_expired().await;

        let mut state = self.inner.write().await;

        // Check if already claimed
        if let Some(existing) = state.job_claims.get(job_id) {
            if existing.expires_at > Utc::now() {
                return Ok(false);
            }
        }

        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default();

        state.job_claims.insert(
            job_id.to_string(),
            JobClaimEntry {
                worker_id: worker_id.to_string(),
                expires_at,
            },
        );

        Ok(true)
    }

    async fn release_job_claim(
        &self,
        job_id: &str,
        worker_id: &str,
    ) -> Result<(), CoordinationError> {
        self.cleanup_expired().await;

        let mut state = self.inner.write().await;

        if let Some(entry) = state.job_claims.get(job_id) {
            if entry.worker_id == worker_id {
                state.job_claims.remove(job_id);
            }
        }

        Ok(())
    }

    async fn heartbeat_job(
        &self,
        job_id: &str,
        worker_id: &str,
        additional_ttl: Duration,
    ) -> Result<bool, CoordinationError> {
        self.cleanup_expired().await;

        let mut state = self.inner.write().await;

        if let Some(entry) = state.job_claims.get_mut(job_id) {
            if entry.worker_id == worker_id {
                entry.expires_at =
                    Utc::now() + chrono::Duration::from_std(additional_ttl).unwrap_or_default();
                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn get_job_claim(&self, job_id: &str) -> Result<Option<WorkerClaim>, CoordinationError> {
        self.cleanup_expired().await;

        let state = self.inner.read().await;

        if let Some(entry) = state.job_claims.get(job_id) {
            if entry.expires_at > Utc::now() {
                return Ok(Some(WorkerClaim {
                    job_id: job_id.to_string(),
                    worker_id: entry.worker_id.clone(),
                    expires_at: entry.expires_at,
                }));
            }
        }

        Ok(None)
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
        self.cleanup_expired().await;

        let mut state = self.inner.write().await;
        let now = Utc::now();

        let entry = state
            .rate_limits
            .entry(key.to_string())
            .or_insert_with(|| RateLimitEntry {
                count: 0,
                window_start: now,
                window_duration: window,
            });

        // Check if window has expired
        let window_end = entry.window_start
            + chrono::Duration::from_std(entry.window_duration).unwrap_or_default();
        if now > window_end {
            // Reset window
            entry.count = 0;
            entry.window_start = now;
            entry.window_duration = window;
        }

        entry.count += 1;

        if entry.count > max_requests {
            let retry_after = (window_end - now).num_seconds().max(0) as u64;
            Ok(RateLimitStatus::denied(
                entry.count,
                max_requests,
                retry_after,
            ))
        } else {
            Ok(RateLimitStatus::allowed(entry.count, max_requests))
        }
    }

    async fn reset_rate_limit(&self, key: &str) -> Result<(), CoordinationError> {
        let mut state = self.inner.write().await;
        state.rate_limits.remove(key);
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
        let mut state = self.inner.write().await;

        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default();

        state.sessions.insert(
            token_hash.to_string(),
            SessionEntry {
                user_id: user_id.to_string(),
                expires_at,
                revoked: false,
            },
        );

        Ok(())
    }

    async fn revoke_session(
        &self,
        token_hash: &str,
        ttl: Duration,
    ) -> Result<(), CoordinationError> {
        let mut state = self.inner.write().await;

        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default();

        // Mark as revoked (or create revoked entry if not exists)
        state.sessions.insert(
            token_hash.to_string(),
            SessionEntry {
                user_id: String::new(),
                expires_at,
                revoked: true,
            },
        );

        Ok(())
    }

    async fn is_session_valid(&self, token_hash: &str) -> Result<bool, CoordinationError> {
        self.cleanup_expired().await;

        let state = self.inner.read().await;

        if let Some(entry) = state.sessions.get(token_hash) {
            Ok(entry.expires_at > Utc::now() && !entry.revoked)
        } else {
            // Unknown session - valid if not explicitly revoked (stateless fallback)
            Ok(true)
        }
    }

    async fn get_cached_session(
        &self,
        token_hash: &str,
    ) -> Result<Option<CachedSession>, CoordinationError> {
        self.cleanup_expired().await;

        let state = self.inner.read().await;

        if let Some(entry) = state.sessions.get(token_hash) {
            Ok(Some(CachedSession {
                token_hash: token_hash.to_string(),
                user_id: entry.user_id.clone(),
                expires_at: entry.expires_at,
                revoked: entry.revoked,
            }))
        } else {
            Ok(None)
        }
    }

    // =========================================================================
    // Idempotency
    // =========================================================================

    async fn check_idempotency_key(
        &self,
        key: &str,
        ttl: Duration,
    ) -> Result<bool, CoordinationError> {
        self.cleanup_expired().await;

        let mut state = self.inner.write().await;

        if state.idempotency_keys.contains_key(key) {
            return Ok(false);
        }

        state.idempotency_keys.insert(
            key.to_string(),
            IdempotencyEntry {
                created_at: Utc::now(),
                ttl,
            },
        );

        Ok(true)
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
        let mut state = self.inner.write().await;

        let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default();

        let presence = state
            .presence
            .entry(user_id.to_string())
            .or_insert_with(|| UserPresence {
                connections: HashMap::new(),
            });

        presence
            .connections
            .insert(connection_id.to_string(), expires_at);

        Ok(())
    }

    async fn mark_user_disconnected(
        &self,
        user_id: &str,
        connection_id: &str,
    ) -> Result<(), CoordinationError> {
        let mut state = self.inner.write().await;

        if let Some(presence) = state.presence.get_mut(user_id) {
            presence.connections.remove(connection_id);
        }

        Ok(())
    }

    async fn get_user_connections(&self, user_id: &str) -> Result<Vec<String>, CoordinationError> {
        self.cleanup_expired().await;

        let state = self.inner.read().await;

        if let Some(presence) = state.presence.get(user_id) {
            Ok(presence.connections.keys().cloned().collect())
        } else {
            Ok(Vec::new())
        }
    }

    async fn is_user_online(&self, user_id: &str) -> Result<bool, CoordinationError> {
        self.cleanup_expired().await;

        let state = self.inner.read().await;

        if let Some(presence) = state.presence.get(user_id) {
            Ok(!presence.connections.is_empty())
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_lock_expiration() {
        let coord = InMemoryCoordinationStore::new();

        // Acquire lock with short TTL
        let lock = coord
            .acquire_lock("resource1", "owner1", Duration::from_millis(50))
            .await
            .unwrap();
        assert!(lock.is_valid());

        // Should be locked
        assert!(coord.is_locked("resource1").await.unwrap());

        // Wait for expiration
        sleep(Duration::from_millis(100)).await;

        // Cleanup happens on next operation
        coord.cleanup_expired().await;

        // Should no longer be locked
        assert!(!coord.is_locked("resource1").await.unwrap());

        // Another owner can now acquire
        let lock2 = coord
            .acquire_lock("resource1", "owner2", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(lock2.is_valid());
    }

    #[tokio::test]
    async fn test_rate_limit_window_reset() {
        let coord = InMemoryCoordinationStore::new();

        // Use up the rate limit
        for _ in 0..5 {
            coord
                .check_rate_limit("key1", 5, Duration::from_millis(100))
                .await
                .unwrap();
        }

        // Next should be denied
        let status = coord
            .check_rate_limit("key1", 5, Duration::from_millis(100))
            .await
            .unwrap();
        assert!(!status.allowed);

        // Wait for window to expire
        sleep(Duration::from_millis(150)).await;

        // Cleanup
        coord.cleanup_expired().await;

        // Should be allowed again (new window)
        let status = coord
            .check_rate_limit("key1", 5, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(status.allowed);
        assert_eq!(status.current_count, 1);
    }
}
