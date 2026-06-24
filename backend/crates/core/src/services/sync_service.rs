//! Sync service for managing device synchronization
//!
//! This service provides the business logic for:
//! - Managing sync cursors/checkpoints for devices
//! - Converting events to sync deltas
//! - Handling pagination for large delta responses
//! - Conflict resolution guidance

use chrono::{DateTime, Utc};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

use rustshare_storage::repos::sync::{DeltaResult, SyncCursor, SyncDelta, SyncRepository};

/// Errors that can occur during sync operations
#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Cursor not found for device: {0}")]
    CursorNotFound(String),
    
    #[error("Invalid cursor: {0}")]
    InvalidCursor(String),
    
    #[error("Repository error: {0}")]
    Repository(#[from] rustshare_storage::repos::RepositoryError),
    
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for sync operations
pub type Result<T> = std::result::Result<T, SyncError>;

/// Sync service for managing device synchronization
pub struct SyncService<R: SyncRepository> {
    repository: Arc<R>,
    /// Maximum number of delta items per request
    max_page_size: usize,
    /// Default number of delta items per request
    default_page_size: usize,
}

impl<R: SyncRepository> SyncService<R> {
    /// Create a new sync service with default configuration
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            max_page_size: 1000,
            default_page_size: 100,
        }
    }
    
    /// Create a new sync service with custom page sizes
    pub fn with_page_sizes(
        repository: Arc<R>,
        max_page_size: usize,
        default_page_size: usize,
    ) -> Self {
        Self {
            repository,
            max_page_size,
            default_page_size,
        }
    }
    
    /// Get or create a sync cursor for a device
    ///
    /// This is called when a device initiates sync or needs to reset
    /// its cursor. The returned cursor represents the starting point
    /// for future delta queries.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The authenticated user ID
    /// * `device_id` - The device identifier (from device pairing)
    /// * `device_info` - Optional device description (e.g., "MacBook Pro - Safari")
    pub async fn get_or_create_cursor(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        _device_info: Option<String>,
    ) -> Result<SyncCursor> {
        info!(
            "Getting or creating sync cursor for user {} device {}",
            user_id, device_id
        );
        
        let cursor = self.repository.get_or_create_cursor(user_id, device_id).await?;
        
        debug!(
            "Cursor for user {} device {}: last_event_id={}",
            user_id, device_id, cursor.last_event_id
        );
        
        Ok(cursor)
    }
    
    /// Get delta changes since a cursor
    ///
    /// This is the main API for retrieving changes. It returns all
    /// events that have occurred since the given cursor position.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The authenticated user ID
    /// * `cursor` - The opaque cursor token from the previous response
    /// * `limit` - Maximum number of items to return (capped at max_page_size)
    ///
    /// # Returns
    ///
    /// A `DeltaResult` containing:
    /// - `items`: The list of changes
    /// - `next_cursor`: Cursor for the next page (None if no more items)
    /// - `has_more`: Whether there are more items to fetch
    pub async fn get_delta(
        &self,
        user_id: Uuid,
        cursor: &str,
        limit: Option<usize>,
    ) -> Result<DeltaResult> {
        let limit = limit.unwrap_or(self.default_page_size)
            .min(self.max_page_size)
            .max(1);
        
        debug!(
            "Getting delta for user {} with cursor {} (limit: {})",
            user_id, cursor, limit
        );
        
        let result = self.repository.get_delta(user_id, cursor, limit).await?;
        
        info!(
            "Retrieved {} delta items for user {} (has_more: {})",
            result.items.len(), user_id, result.has_more
        );
        
        Ok(result)
    }
    
    /// Update the cursor for a device
    ///
    /// Clients should call this after successfully applying a batch
    /// of deltas. This marks the sync checkpoint.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The authenticated user ID
    /// * `device_id` - The device identifier
    /// * `cursor` - The new cursor token
    /// * `last_event_id` - The ID of the last successfully processed event
    pub async fn update_cursor(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        cursor: &str,
        last_event_id: Uuid,
    ) -> Result<()> {
        debug!(
            "Updating cursor for user {} device {}: last_event_id={}",
            user_id, device_id, last_event_id
        );
        
        self.repository.update_cursor(user_id, device_id, cursor, last_event_id).await?;
        
        Ok(())
    }
    
    /// List all sync cursors for a user
    ///
    /// This is useful for device management UI - users can see
    /// which devices are syncing and when they last synced.
    pub async fn list_device_cursors(&self, user_id: Uuid) -> Result<Vec<DeviceSyncInfo>> {
        let cursors = self.repository.list_user_cursors(user_id).await?;
        
        let devices = cursors.into_iter()
            .map(|cursor| DeviceSyncInfo {
                device_id: cursor.device_id,
                last_sync_at: cursor.updated_at,
                last_event_id: cursor.last_event_id,
            })
            .collect();
        
        Ok(devices)
    }
    
    /// Delete a device's sync cursor
    ///
    /// Call this when a device is deauthorized or the user
    /// wants to reset sync for a specific device.
    pub async fn delete_device_cursor(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<()> {
        info!(
            "Deleting sync cursor for user {} device {}",
            user_id, device_id
        );
        
        self.repository.delete_cursor(user_id, device_id).await?;
        
        Ok(())
    }
    
    /// Perform a full sync reset for a device
    ///
    /// This creates a new cursor starting from the current time,
    /// effectively marking all existing content as "synced".
    /// Useful when a device needs to re-download everything.
    pub async fn reset_device_sync(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<SyncCursor> {
        warn!(
            "Performing full sync reset for user {} device {}",
            user_id, device_id
        );
        
        // Delete the old cursor - best effort, may not exist
        if let Err(e) = self.repository.delete_cursor(user_id, device_id).await {
            tracing::debug!(user_id = %user_id, device_id = %device_id, error = %e, "failed to delete old cursor");
        }
        
        // Create a new cursor starting from now
        let cursor = self.repository.get_or_create_cursor(user_id, device_id).await?;
        
        Ok(cursor)
    }
}

/// Information about a device's sync status
#[derive(Debug, Clone)]
pub struct DeviceSyncInfo {
    /// Device identifier
    pub device_id: Uuid,
    /// When the device last synced
    pub last_sync_at: DateTime<Utc>,
    /// Last event processed by the device
    pub last_event_id: Uuid,
}

/// Conflict resolution strategies
///
/// When a client uploads changes that conflict with server state,
/// these strategies determine how to resolve the conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Server version wins (discard client changes)
    ServerWins,
    /// Client version wins (overwrite server)
    ClientWins,
    /// Last write wins based on timestamp
    LastWriteWins,
    /// Rename client version
    Rename,
}

/// Conflict information for a sync operation
#[derive(Debug, Clone)]
pub struct ConflictInfo {
    /// The resource ID in conflict
    pub resource_id: Uuid,
    /// Type of resource ("file" or "folder")
    pub resource_type: String,
    /// Server version timestamp
    pub server_timestamp: DateTime<Utc>,
    /// Client version timestamp
    pub client_timestamp: DateTime<Utc>,
    /// Server version number (for optimistic concurrency)
    pub server_version: u64,
    /// Client version number
    pub client_version: u64,
}

impl ConflictInfo {
    /// Determine the recommended resolution strategy
    ///
    /// Phase 1 implements "last writer wins" with a deterministic
    /// tiebreaker (higher timestamp wins, if equal server wins).
    pub fn recommended_resolution(&self) -> ConflictResolution {
        if self.client_timestamp > self.server_timestamp {
            ConflictResolution::ClientWins
        } else {
            ConflictResolution::ServerWins
        }
    }
}

/// Operations that the sync service provides
///
/// This trait abstracts the sync service for testing and allows
/// for different implementations in the future.
#[allow(async_fn_in_trait)]
pub trait SyncServiceOps: Send + Sync {
    /// Get or create a sync cursor
    async fn get_or_create_cursor(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        device_info: Option<String>,
    ) -> Result<SyncCursor>;
    
    /// Get delta changes
    async fn get_delta(
        &self,
        user_id: Uuid,
        cursor: &str,
        limit: Option<usize>,
    ) -> Result<DeltaResult>;
    
    /// Update cursor after processing deltas
    async fn update_cursor(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        cursor: &str,
        last_event_id: Uuid,
    ) -> Result<()>;
    
    /// List device cursors for a user
    async fn list_device_cursors(&self, user_id: Uuid) -> Result<Vec<DeviceSyncInfo>>;
    
    /// Delete a device cursor
    async fn delete_device_cursor(&self, user_id: Uuid, device_id: Uuid) -> Result<()>;
    
    /// Reset sync for a device
    async fn reset_device_sync(&self, user_id: Uuid, device_id: Uuid) -> Result<SyncCursor>;
}

impl<R: SyncRepository> SyncServiceOps for SyncService<R> {
    async fn get_or_create_cursor(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        device_info: Option<String>,
    ) -> Result<SyncCursor> {
        self.get_or_create_cursor(user_id, device_id, device_info).await
    }
    
    async fn get_delta(
        &self,
        user_id: Uuid,
        cursor: &str,
        limit: Option<usize>,
    ) -> Result<DeltaResult> {
        self.get_delta(user_id, cursor, limit).await
    }
    
    async fn update_cursor(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        cursor: &str,
        last_event_id: Uuid,
    ) -> Result<()> {
        self.update_cursor(user_id, device_id, cursor, last_event_id).await
    }
    
    async fn list_device_cursors(&self, user_id: Uuid) -> Result<Vec<DeviceSyncInfo>> {
        self.list_device_cursors(user_id).await
    }
    
    async fn delete_device_cursor(&self, user_id: Uuid, device_id: Uuid) -> Result<()> {
        self.delete_device_cursor(user_id, device_id).await
    }
    
    async fn reset_device_sync(&self, user_id: Uuid, device_id: Uuid) -> Result<SyncCursor> {
        self.reset_device_sync(user_id, device_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_conflict_recommended_resolution() {
        use chrono::Duration;
        
        let now = Utc::now();
        
        // Client timestamp is newer
        let conflict = ConflictInfo {
            resource_id: Uuid::new_v4(),
            resource_type: "file".to_string(),
            server_timestamp: now - Duration::minutes(5),
            client_timestamp: now,
            server_version: 1,
            client_version: 2,
        };
        assert_eq!(conflict.recommended_resolution(), ConflictResolution::ClientWins);
        
        // Server timestamp is newer
        let conflict = ConflictInfo {
            resource_id: Uuid::new_v4(),
            resource_type: "file".to_string(),
            server_timestamp: now,
            client_timestamp: now - Duration::minutes(5),
            server_version: 2,
            client_version: 1,
        };
        assert_eq!(conflict.recommended_resolution(), ConflictResolution::ServerWins);
        
        // Same timestamp - server wins
        let conflict = ConflictInfo {
            resource_id: Uuid::new_v4(),
            resource_type: "file".to_string(),
            server_timestamp: now,
            client_timestamp: now,
            server_version: 1,
            client_version: 1,
        };
        assert_eq!(conflict.recommended_resolution(), ConflictResolution::ServerWins);
    }
}
