//! RustFS-backed implementation of the sync repository

use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use super::{parse_cursor, DeltaResult, SyncCursor, SyncDelta, SyncRepository};
use crate::metadata_v2::schemas::{EventDocument, EventType, SyncCursorDocument};
use crate::metadata_v2::{
    EventLogStore, MetadataDocumentStore, MetadataDocumentStoreExt, PutOptions,
};
use crate::repos::{PathBuilder, RepositoryError};

/// RustFS-backed sync repository
pub struct RustFsSyncRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    event_store: Arc<dyn EventLogStore>,
    path_builder: PathBuilder,
}

impl RustFsSyncRepository {
    /// Create a new RustFS sync repository
    pub fn new(
        doc_store: Arc<dyn MetadataDocumentStore>,
        event_store: Arc<dyn EventLogStore>,
        path_builder: PathBuilder,
    ) -> Self {
        Self {
            doc_store,
            event_store,
            path_builder,
        }
    }

    /// Build the path for a sync cursor document
    fn cursor_path(&self, user_id: Uuid, device_id: Uuid) -> String {
        format!(
            "{}/{}/sync/cursors/{}/{}.json",
            self.path_builder.base_prefix(),
            self.path_builder.namespace(),
            user_id,
            device_id
        )
    }

    /// Convert an EventDocument to a SyncDelta
    fn event_to_delta(&self, event: &EventDocument) -> Option<SyncDelta> {
        match event.event_type {
            EventType::FileUploaded => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::FileCreated {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    file_id: payload.get("file_id")?.as_str()?.parse().ok()?,
                    name: payload.get("name")?.as_str()?.to_string(),
                    path: payload.get("path")?.as_str()?.to_string(),
                    parent_id: payload
                        .get("parent_folder_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    size: payload.get("size")?.as_i64()?,
                    mime_type: payload.get("mime_type")?.as_str()?.to_string(),
                    content_hash: payload.get("content_hash")?.as_str()?.to_string(),
                    version_id: payload.get("version_id")?.as_str()?.parse().ok()?,
                })
            }
            EventType::FileModified => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::FileModified {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    file_id: payload.get("file_id")?.as_str()?.parse().ok()?,
                    name: payload.get("name")?.as_str()?.to_string(),
                    path: payload.get("path")?.as_str()?.to_string(),
                    size: payload.get("new_size")?.as_i64()?,
                    mime_type: payload.get("mime_type")?.as_str()?.to_string(),
                    content_hash: payload.get("new_content_hash")?.as_str()?.to_string(),
                    version_id: payload.get("new_version_id")?.as_str()?.parse().ok()?,
                    version_number: payload.get("new_version")?.as_i64()? as i32,
                })
            }
            EventType::FileRenamed => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::FileRenamed {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    file_id: payload.get("file_id")?.as_str()?.parse().ok()?,
                    old_name: payload.get("old_name")?.as_str()?.to_string(),
                    new_name: payload.get("new_name")?.as_str()?.to_string(),
                    old_path: payload.get("old_path")?.as_str()?.to_string(),
                    new_path: payload.get("new_path")?.as_str()?.to_string(),
                })
            }
            EventType::FileMoved => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::FileMoved {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    file_id: payload.get("file_id")?.as_str()?.parse().ok()?,
                    name: payload.get("name")?.as_str()?.to_string(),
                    old_parent_id: payload
                        .get("old_parent_folder_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    new_parent_id: payload
                        .get("new_parent_folder_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    old_path: payload.get("old_path")?.as_str()?.to_string(),
                    new_path: payload.get("new_path")?.as_str()?.to_string(),
                })
            }
            EventType::FileDeleted => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::FileDeleted {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    file_id: payload.get("file_id")?.as_str()?.parse().ok()?,
                    name: payload.get("file_name")?.as_str()?.to_string(),
                    path: payload.get("path")?.as_str()?.to_string(),
                    parent_id: payload
                        .get("folder_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                })
            }
            EventType::FileRestored => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::FileRestored {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    file_id: payload.get("file_id")?.as_str()?.parse().ok()?,
                    name: payload.get("name")?.as_str()?.to_string(),
                    path: payload.get("path")?.as_str()?.to_string(),
                    parent_id: payload
                        .get("parent_folder_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                })
            }
            EventType::FolderCreated => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::FolderCreated {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    folder_id: payload.get("folder_id")?.as_str()?.parse().ok()?,
                    name: payload.get("name")?.as_str()?.to_string(),
                    path: payload.get("path")?.as_str()?.to_string(),
                    parent_id: payload
                        .get("parent_folder_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                })
            }
            EventType::FolderRenamed => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::FolderRenamed {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    folder_id: payload.get("folder_id")?.as_str()?.parse().ok()?,
                    old_name: payload.get("old_name")?.as_str()?.to_string(),
                    new_name: payload.get("new_name")?.as_str()?.to_string(),
                    old_path: payload.get("old_path")?.as_str()?.to_string(),
                    new_path: payload.get("new_path")?.as_str()?.to_string(),
                })
            }
            EventType::FolderMoved => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::FolderMoved {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    folder_id: payload.get("folder_id")?.as_str()?.parse().ok()?,
                    name: payload.get("name")?.as_str()?.to_string(),
                    old_parent_id: payload
                        .get("old_parent_folder_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    new_parent_id: payload
                        .get("new_parent_folder_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    old_path: payload.get("old_path")?.as_str()?.to_string(),
                    new_path: payload.get("new_path")?.as_str()?.to_string(),
                })
            }
            EventType::FolderDeleted => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::FolderDeleted {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    folder_id: payload.get("folder_id")?.as_str()?.parse().ok()?,
                    name: payload.get("name")?.as_str()?.to_string(),
                    path: payload.get("path")?.as_str()?.to_string(),
                    parent_id: payload
                        .get("parent_folder_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                })
            }
            EventType::FolderRestored => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::FolderRestored {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    folder_id: payload.get("folder_id")?.as_str()?.parse().ok()?,
                    name: payload.get("name")?.as_str()?.to_string(),
                    path: payload.get("path")?.as_str()?.to_string(),
                    parent_id: payload
                        .get("parent_folder_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                })
            }
            EventType::ShareCreated => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::ShareCreated {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    share_id: payload.get("share_id")?.as_str()?.parse().ok()?,
                    resource_type: payload.get("resource_type")?.as_str()?.to_string(),
                    resource_id: payload.get("resource_id")?.as_str()?.parse().ok()?,
                    resource_name: payload.get("resource_name")?.as_str()?.to_string(),
                    permissions: payload.get("permissions")?.as_str()?.to_string(),
                    scope: payload.get("scope")?.as_str()?.to_string(),
                    recipient_user_id: payload
                        .get("recipient_user_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                })
            }
            EventType::ShareRevoked => {
                let payload: serde_json::Value = event.payload.clone();
                Some(SyncDelta::ShareRevoked {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    share_id: payload.get("share_id")?.as_str()?.parse().ok()?,
                    resource_type: payload.get("resource_type")?.as_str()?.to_string(),
                    resource_id: payload.get("resource_id")?.as_str()?.parse().ok()?,
                })
            }
            EventType::ShareUpdated => {
                let payload: serde_json::Value = event.payload.clone();
                let changes = payload
                    .get("changes")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                Some(SyncDelta::ShareUpdated {
                    event_id: event.id,
                    timestamp: event.occurred_at,
                    share_id: payload.get("share_id")?.as_str()?.parse().ok()?,
                    resource_type: payload.get("resource_type")?.as_str()?.to_string(),
                    resource_id: payload.get("resource_id")?.as_str()?.parse().ok()?,
                    changes,
                })
            }
        }
    }
}

#[async_trait]
impl SyncRepository for RustFsSyncRepository {
    async fn get_or_create_cursor(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<SyncCursor, RepositoryError> {
        let path = self.cursor_path(user_id, device_id);

        // Try to get existing cursor
        if let Some((doc, _)) = self
            .doc_store
            .get::<SyncCursorDocument>(&path)
            .await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?
        {
            return Ok(doc.into());
        }

        // Create new cursor starting from now
        let now = Utc::now();
        let cursor = SyncCursorDocument::generate_cursor(now);
        // Use a nil UUID for the initial last_event_id
        let last_event_id = Uuid::nil();

        let doc = SyncCursorDocument::new(
            user_id,
            device_id,
            cursor,
            last_event_id,
            None, // device_info
        );

        self.doc_store
            .put(&path, &doc, PutOptions::default())
            .await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;

        Ok(doc.into())
    }

    async fn update_cursor(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        cursor: &str,
        last_event_id: Uuid,
    ) -> Result<(), RepositoryError> {
        let path = self.cursor_path(user_id, device_id);

        // Get existing cursor or create new one
        let mut doc = if let Some((existing, _)) = self
            .doc_store
            .get::<SyncCursorDocument>(&path)
            .await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?
        {
            existing
        } else {
            return Err(RepositoryError::NotFound(format!(
                "Cursor not found for device {}",
                device_id
            )));
        };

        doc.update(cursor.to_string(), last_event_id);

        self.doc_store
            .put(&path, &doc, PutOptions::default())
            .await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;

        Ok(())
    }

    async fn get_delta(
        &self,
        user_id: Uuid,
        since_cursor: &str,
        limit: usize,
    ) -> Result<DeltaResult, RepositoryError> {
        // Parse the cursor to get the timestamp
        let since_timestamp = parse_cursor(since_cursor)
            .map_err(|e| RepositoryError::ValidationError(format!("Invalid cursor: {}", e)))?;

        // Query events from the event store
        let events = self
            .event_store
            .read_since(since_timestamp, limit + 1) // Fetch one extra to determine has_more
            .await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;

        let has_more = events.len() > limit;
        let events_to_process: Vec<_> = events.into_iter().take(limit).collect();

        // Convert events to deltas
        let mut items = Vec::new();
        let mut last_timestamp = since_timestamp;

        for event in &events_to_process {
            // Filter events to only include those relevant to this user
            // For now, we include events where the user is the actor
            // In a more sophisticated implementation, we might filter by
            // resource ownership or share relationships
            if event.actor_id == user_id || self.is_event_relevant_to_user(event, user_id).await {
                if let Some(delta) = self.event_to_delta(event) {
                    items.push(delta);
                }
            }
            last_timestamp = event.occurred_at;
        }

        // Generate next cursor if there are more items
        let next_cursor = if has_more {
            Some(SyncCursorDocument::generate_cursor(last_timestamp))
        } else {
            None
        };

        Ok(DeltaResult {
            items,
            next_cursor,
            has_more,
            total_count: None, // We could estimate this if needed
        })
    }

    async fn list_user_cursors(&self, user_id: Uuid) -> Result<Vec<SyncCursor>, RepositoryError> {
        let prefix = format!(
            "{}/{}/sync/cursors/{}/",
            self.path_builder.base_prefix(),
            self.path_builder.namespace(),
            user_id
        );

        let keys = self
            .doc_store
            .list_prefix(&prefix)
            .await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;

        let mut cursors = Vec::new();
        for key in keys {
            if let Some((doc, _)) = self
                .doc_store
                .get::<SyncCursorDocument>(&key)
                .await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))?
            {
                cursors.push(doc.into());
            }
        }

        Ok(cursors)
    }

    async fn delete_cursor(&self, user_id: Uuid, device_id: Uuid) -> Result<(), RepositoryError> {
        let path = self.cursor_path(user_id, device_id);

        self.doc_store
            .delete(&path)
            .await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;

        Ok(())
    }
}

impl RustFsSyncRepository {
    /// Check if an event is relevant to a user
    ///
    /// This determines whether the user should receive this event in their delta feed.
    /// Events are relevant if:
    /// - The user performed the action (already checked in caller)
    /// - The event affects a resource owned by the user
    /// - The event affects a resource shared with the user
    async fn is_event_relevant_to_user(&self, event: &EventDocument, _user_id: Uuid) -> bool {
        // For Phase 1, we keep this simple and include events where the user
        // is the actor. More sophisticated filtering can be added later.

        // Note: In a full implementation, we would check:
        // 1. Does the user own the resource?
        // 2. Is the resource shared with the user?
        // 3. Is this a share event involving the user?

        // For now, we rely on the actor_id check in the caller
        // and include share events that might affect the user
        matches!(
            event.event_type,
            EventType::ShareCreated | EventType::ShareRevoked | EventType::ShareUpdated
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests would require a full integration setup with
    // a running RustFS instance. For unit tests, we would typically
    // use mock implementations of the dependencies.

    #[test]
    fn test_event_to_delta_file_created() {
        // This would test the conversion logic with mock events
    }
}
