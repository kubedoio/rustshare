//! Coordination layer for multi-object mutations
//!
//! Provides optimistic concurrency control and lease-based locking
//! for operations that span multiple metadata documents.

use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, Instant};
use tracing::debug;
use uuid::Uuid;

/// Lease token for coordination
#[derive(Debug, Clone)]
pub struct Lease {
    /// Resource being leased
    pub resource_id: String,
    /// Unique lease token
    pub token: String,
    /// Lease expiration time
    pub expires_at: Instant,
    /// Owner identifier (for debugging)
    pub owner: String,
}

impl Lease {
    /// Check if lease is still valid
    pub fn is_valid(&self) -> bool {
        Instant::now() < self.expires_at
    }
}

/// Coordination strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinationStrategy {
    /// Use conditional writes only (optimistic concurrency)
    OptimisticOnly,
    /// Use leases only
    LeaseOnly,
    /// Prefer optimistic, fallback to leases
    Hybrid,
}

/// Error types for coordination
#[derive(Debug, thiserror::Error)]
pub enum CoordinationError {
    #[error("Resource {resource_id} is already leased")]
    AlreadyLeased { resource_id: String },
    
    #[error("Lease {token} for {resource_id} is invalid or expired")]
    InvalidLease { resource_id: String, token: String },
    
    #[error("Precondition failed: ETag mismatch for {resource_id}")]
    PreconditionFailed { resource_id: String },
    
    #[error("Resource {resource_id} was modified concurrently")]
    ConcurrentModification { resource_id: String },
    
    #[error("Coordination timeout for {resource_id}")]
    Timeout { resource_id: String },
    
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Metadata coordination trait
#[async_trait]
pub trait MetadataCoordination: Send + Sync {
    /// Attempt to acquire a lease for a resource
    async fn acquire_lease(
        &self,
        resource_id: &str,
        ttl_secs: u64,
        owner: &str,
    ) -> Result<Lease, CoordinationError>;
    
    /// Release a lease
    async fn release_lease(&self, lease: &Lease) -> Result<(), CoordinationError>;
    
    /// Check if a resource has an active lease
    async fn check_lease(&self, resource_id: &str) -> Result<Option<Lease>, CoordinationError>;
    
    /// Extend an existing lease
    async fn extend_lease(
        &self,
        lease: &Lease,
        additional_secs: u64,
    ) -> Result<Lease, CoordinationError>;
    
    /// Execute a coordinated multi-resource operation
    async fn coordinate<
        T: Send,
        F: FnOnce() -> futures::future::BoxFuture<'static, Result<T, CoordinationError>> + Send,
    >(
        &self,
        resource_ids: Vec<String>,
        ttl_secs: u64,
        owner: &str,
        operation: F,
    ) -> Result<T, CoordinationError>;
}

/// In-memory lease coordinator (for single-node deployments)
pub struct InMemoryCoordination {
    leases: Mutex<HashMap<String, Lease>>,
}

impl InMemoryCoordination {
    pub fn new() -> Self {
        Self {
            leases: Mutex::new(HashMap::new()),
        }
    }
    
    /// Clean up expired leases
    fn cleanup_expired(&self) {
        let mut leases = self.leases.lock().unwrap();
        leases.retain(|_, lease| lease.is_valid());
    }
}

impl Default for InMemoryCoordination {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetadataCoordination for InMemoryCoordination {
    async fn acquire_lease(
        &self,
        resource_id: &str,
        ttl_secs: u64,
        owner: &str,
    ) -> Result<Lease, CoordinationError> {
        self.cleanup_expired();
        
        let mut leases = self.leases.lock().unwrap();
        
        // Check if already leased
        if let Some(existing) = leases.get(resource_id) {
            if existing.is_valid() {
                return Err(CoordinationError::AlreadyLeased {
                    resource_id: resource_id.to_string(),
                });
            }
        }
        
        let lease = Lease {
            resource_id: resource_id.to_string(),
            token: Uuid::new_v4().to_string(),
            expires_at: Instant::now() + Duration::from_secs(ttl_secs),
            owner: owner.to_string(),
        };
        
        leases.insert(resource_id.to_string(), lease.clone());
        
        Ok(lease)
    }
    
    async fn release_lease(&self, lease: &Lease) -> Result<(), CoordinationError> {
        let mut leases = self.leases.lock().unwrap();
        
        if let Some(existing) = leases.get(&lease.resource_id) {
            if existing.token == lease.token {
                leases.remove(&lease.resource_id);
                return Ok(());
            }
        }
        
        Err(CoordinationError::InvalidLease {
            resource_id: lease.resource_id.clone(),
            token: lease.token.clone(),
        })
    }
    
    async fn check_lease(&self, resource_id: &str) -> Result<Option<Lease>, CoordinationError> {
        self.cleanup_expired();
        
        let leases = self.leases.lock().unwrap();
        
        if let Some(lease) = leases.get(resource_id) {
            if lease.is_valid() {
                return Ok(Some(lease.clone()));
            }
        }
        
        Ok(None)
    }
    
    async fn extend_lease(
        &self,
        lease: &Lease,
        additional_secs: u64,
    ) -> Result<Lease, CoordinationError> {
        let mut leases = self.leases.lock().unwrap();
        
        if let Some(existing) = leases.get(&lease.resource_id) {
            if existing.token == lease.token {
                let new_lease = Lease {
                    resource_id: lease.resource_id.clone(),
                    token: lease.token.clone(),
                    expires_at: Instant::now() + Duration::from_secs(additional_secs),
                    owner: lease.owner.clone(),
                };
                
                leases.insert(lease.resource_id.clone(), new_lease.clone());
                return Ok(new_lease);
            }
        }
        
        Err(CoordinationError::InvalidLease {
            resource_id: lease.resource_id.clone(),
            token: lease.token.clone(),
        })
    }
    
    async fn coordinate<
        T: Send,
        F: FnOnce() -> futures::future::BoxFuture<'static, Result<T, CoordinationError>> + Send,
    >(
        &self,
        resource_ids: Vec<String>,
        ttl_secs: u64,
        owner: &str,
        operation: F,
    ) -> Result<T, CoordinationError> {
        // Acquire all leases first (to avoid deadlock)
        let mut leases = Vec::new();
        
        for resource_id in &resource_ids {
            match self.acquire_lease(resource_id, ttl_secs, owner).await {
                Ok(lease) => leases.push(lease),
                Err(e) => {
                    // Release any leases we acquired - best effort
                    for lease in leases {
                        if let Err(release_err) = self.release_lease(&lease).await {
                            tracing::debug!(token = %lease.token, resource = %lease.resource_id, error = %release_err, "failed to release lease during cleanup");
                        }
                    }
                    return Err(e);
                }
            }
        }
        
        // Execute the operation
        let result = operation().await;
        
        // Release all leases - best effort
        for lease in leases {
            if let Err(e) = self.release_lease(&lease).await {
                tracing::debug!(token = %lease.token, resource = %lease.resource_id, error = %e, "failed to release lease");
            }
        }
        
        result
    }
}

/// Object-store backed coordination using lease documents
pub struct ObjectStoreCoordination {
    doc_store: Arc<dyn MetadataDocumentStore>,
    _owner_id: String,
}

impl ObjectStoreCoordination {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, owner_id: String) -> Self {
        Self {
            doc_store,
            _owner_id: owner_id,
        }
    }
    
    fn lease_key(&self, resource_id: &str) -> String {
        format!("coordination/leases/{}.json", resource_id)
    }
}

/// Lease document stored in object storage
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeaseDocument {
    pub resource_id: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub owner: String,
    pub created_at: DateTime<Utc>,
}

#[async_trait]
impl MetadataCoordination for ObjectStoreCoordination {
    async fn acquire_lease(
        &self,
        resource_id: &str,
        ttl_secs: u64,
        owner: &str,
    ) -> Result<Lease, CoordinationError> {
        let key = self.lease_key(resource_id);
        
        // Check if there's an existing valid lease
        if let Some((existing, _)) = self.doc_store.get::<LeaseDocument>(&key).await.map_err(|e| {
            CoordinationError::Other(anyhow::anyhow!("Failed to check existing lease: {}", e))
        })? {
            if Utc::now() < existing.expires_at {
                return Err(CoordinationError::AlreadyLeased {
                    resource_id: resource_id.to_string(),
                });
            }
        }
        
        // Create new lease document
        let token = Uuid::new_v4().to_string();
        let lease_doc = LeaseDocument {
            resource_id: resource_id.to_string(),
            token: token.clone(),
            expires_at: Utc::now() + chrono::Duration::seconds(ttl_secs as i64),
            owner: owner.to_string(),
            created_at: Utc::now(),
        };
        
        // Use if-none-match to ensure atomicity
        let opts = PutOptions {
            if_none_match: Some("*".to_string()),
            ..Default::default()
        };
        
        match self.doc_store.put(&key, &lease_doc, opts).await {
            Ok(_) => Ok(Lease {
                resource_id: resource_id.to_string(),
                token,
                expires_at: Instant::now() + Duration::from_secs(ttl_secs),
                owner: owner.to_string(),
            }),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("Precondition") || err_str.contains("409") {
                    Err(CoordinationError::AlreadyLeased {
                        resource_id: resource_id.to_string(),
                    })
                } else {
                    Err(CoordinationError::Other(e))
                }
            }
        }
    }
    
    async fn release_lease(&self, lease: &Lease) -> Result<(), CoordinationError> {
        let key = self.lease_key(&lease.resource_id);
        
        // Check that we own the lease
        if let Some((existing, _metadata)) = self.doc_store.get::<LeaseDocument>(&key).await.map_err(
            |e| CoordinationError::Other(anyhow::anyhow!("Failed to get lease: {}", e)),
        )? {
            if existing.token != lease.token {
                return Err(CoordinationError::InvalidLease {
                    resource_id: lease.resource_id.clone(),
                    token: lease.token.clone(),
                });
            }
            
            // Delete with ETag check
            // We actually delete the lease document
            self.doc_store.delete(&key).await.map_err(|e| {
                CoordinationError::Other(anyhow::anyhow!("Failed to delete lease: {}", e))
            })?;
            
            Ok(())
        } else {
            // Lease doesn't exist, consider it released
            Ok(())
        }
    }
    
    async fn check_lease(&self, resource_id: &str) -> Result<Option<Lease>, CoordinationError> {
        let key = self.lease_key(resource_id);
        
        if let Some((existing, _)) = self.doc_store.get::<LeaseDocument>(&key).await.map_err(|e| {
            CoordinationError::Other(anyhow::anyhow!("Failed to check lease: {}", e))
        })? {
            if Utc::now() < existing.expires_at {
                let ttl_remaining = (existing.expires_at - Utc::now()).num_seconds() as u64;
                
                return Ok(Some(Lease {
                    resource_id: existing.resource_id,
                    token: existing.token,
                    expires_at: Instant::now() + Duration::from_secs(ttl_remaining),
                    owner: existing.owner,
                }));
            }
        }
        
        Ok(None)
    }
    
    async fn extend_lease(
        &self,
        lease: &Lease,
        additional_secs: u64,
    ) -> Result<Lease, CoordinationError> {
        let key = self.lease_key(&lease.resource_id);
        
        if let Some((existing, metadata)) = self.doc_store.get::<LeaseDocument>(&key).await.map_err(
            |e| CoordinationError::Other(anyhow::anyhow!("Failed to get lease: {}", e)),
        )? {
            if existing.token != lease.token {
                return Err(CoordinationError::InvalidLease {
                    resource_id: lease.resource_id.clone(),
                    token: lease.token.clone(),
                });
            }
            
            let updated = LeaseDocument {
                resource_id: existing.resource_id,
                token: existing.token,
                expires_at: Utc::now() + chrono::Duration::seconds(additional_secs as i64),
                owner: existing.owner,
                created_at: existing.created_at,
            };
            
            let opts = PutOptions {
                if_match: Some(metadata.etag),
                ..Default::default()
            };
            
            self.doc_store.put(&key, &updated, opts).await.map_err(|e| {
                if e.to_string().contains("Precondition") {
                    CoordinationError::ConcurrentModification {
                        resource_id: lease.resource_id.clone(),
                    }
                } else {
                    CoordinationError::Other(e)
                }
            })?;
            
            Ok(Lease {
                resource_id: lease.resource_id.clone(),
                token: lease.token.clone(),
                expires_at: Instant::now() + Duration::from_secs(additional_secs),
                owner: lease.owner.clone(),
            })
        } else {
            Err(CoordinationError::InvalidLease {
                resource_id: lease.resource_id.clone(),
                token: lease.token.clone(),
            })
        }
    }
    
    async fn coordinate<
        T: Send,
        F: FnOnce() -> futures::future::BoxFuture<'static, Result<T, CoordinationError>> + Send,
    >(
        &self,
        resource_ids: Vec<String>,
        ttl_secs: u64,
        owner: &str,
        operation: F,
    ) -> Result<T, CoordinationError> {
        // Acquire all leases
        let mut leases = Vec::new();
        
        for resource_id in &resource_ids {
            match self.acquire_lease(resource_id, ttl_secs, owner).await {
                Ok(lease) => leases.push(lease),
                Err(e) => {
                    // Release any acquired leases - best effort
                    for lease in leases {
                        if let Err(release_err) = self.release_lease(&lease).await {
                            tracing::debug!(token = %lease.token, resource = %lease.resource_id, error = %release_err, "failed to release lease during cleanup");
                        }
                    }
                    return Err(e);
                }
            }
        }
        
        // Execute operation
        let result = operation().await;
        
        // Release all leases - best effort
        for lease in leases {
            if let Err(e) = self.release_lease(&lease).await {
                tracing::debug!(token = %lease.token, resource = %lease.resource_id, error = %e, "failed to release lease");
            }
        }
        
        result
    }
}

/// No-op coordination (for backends with strong native consistency)
pub struct NoOpCoordination;

#[async_trait]
impl MetadataCoordination for NoOpCoordination {
    async fn acquire_lease(
        &self,
        _resource_id: &str,
        _ttl_secs: u64,
        _owner: &str,
    ) -> Result<Lease, CoordinationError> {
        // Return a dummy lease that is always valid
        Ok(Lease {
            resource_id: _resource_id.to_string(),
            token: "noop".to_string(),
            expires_at: Instant::now() + Duration::from_secs(3600),
            owner: _owner.to_string(),
        })
    }
    
    async fn release_lease(&self, _lease: &Lease) -> Result<(), CoordinationError> {
        Ok(())
    }
    
    async fn check_lease(&self, _resource_id: &str) -> Result<Option<Lease>, CoordinationError> {
        Ok(None)
    }
    
    async fn extend_lease(
        &self,
        lease: &Lease,
        _additional_secs: u64,
    ) -> Result<Lease, CoordinationError> {
        Ok(lease.clone())
    }
    
    async fn coordinate<
        T: Send,
        F: FnOnce() -> futures::future::BoxFuture<'static, Result<T, CoordinationError>> + Send,
    >(
        &self,
        _resource_ids: Vec<String>,
        _ttl_secs: u64,
        _owner: &str,
        operation: F,
    ) -> Result<T, CoordinationError> {
        // Just execute the operation without coordination
        operation().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    
    #[tokio::test]
    async fn test_in_memory_coordination() {
        let coord = InMemoryCoordination::new();
        
        // Acquire lease
        let lease = coord.acquire_lease("resource1", 60, "test-owner").await.unwrap();
        assert_eq!(lease.resource_id, "resource1");
        assert!(lease.is_valid());
        
        // Try to acquire again (should fail)
        let result = coord.acquire_lease("resource1", 60, "other-owner").await;
        assert!(matches!(result, Err(CoordinationError::AlreadyLeased { .. })));
        
        // Check lease
        let checked = coord.check_lease("resource1").await.unwrap();
        assert!(checked.is_some());
        
        // Release lease
        coord.release_lease(&lease).await.unwrap();
        
        // Now we can acquire again
        let lease2 = coord.acquire_lease("resource1", 60, "test-owner").await.unwrap();
        assert!(lease2.is_valid());
    }
    
    #[tokio::test]
    async fn test_coordinated_operation() {
        let coord = InMemoryCoordination::new();
        
        let resources = vec![
            "resource1".to_string(),
            "resource2".to_string(),
        ];
        
        let result = coord.coordinate(
            resources,
            60,
            "test",
            || async move {
                // This operation runs while holding both leases
                Ok::<_, CoordinationError>("success")
            }.boxed()
        ).await;
        
        assert_eq!(result.unwrap(), "success");
        
        // Verify leases are released
        assert!(coord.check_lease("resource1").await.unwrap().is_none());
        assert!(coord.check_lease("resource2").await.unwrap().is_none());
    }
}
