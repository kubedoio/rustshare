use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::domain::*;

/// Unique identifier for an event
pub type EventId = Uuid;

/// Event aggregate type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AggregateType {
    User,
    File,
    Folder,
    Share,
}

/// Event types in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase", tag = "type")]
pub enum EventType {
    // User events
    UserCreated,
    UserUpdated,
    UserDeleted,

    // File events
    FileUploaded,
    FileModified,
    FileRenamed,
    FileMoved,
    FileDeleted,
    FileRestored,

    // Folder events
    FolderCreated,
    FolderRenamed,
    FolderMoved,
    FolderDeleted,

    // Share events
    ShareCreated,
    ShareRevoked,
    ShareUpdated,
    ShareReceivedByUser,
    SharePermissionChanged,
    ShareRevokedFromUser,

    // Notification events
    NotificationCreated,

    // Sync events
    ConflictDetected,
    ConflictResolved,
    ReplicationStateChanged,
}

impl EventType {
    /// Returns the variant name as a plain string for WebSocket notifications.
    pub fn type_name(&self) -> &'static str {
        match self {
            EventType::UserCreated => "UserCreated",
            EventType::UserUpdated => "UserUpdated",
            EventType::UserDeleted => "UserDeleted",
            EventType::FileUploaded => "FileUploaded",
            EventType::FileModified => "FileModified",
            EventType::FileRenamed => "FileRenamed",
            EventType::FileMoved => "FileMoved",
            EventType::FileDeleted => "FileDeleted",
            EventType::FileRestored => "FileRestored",
            EventType::FolderCreated => "FolderCreated",
            EventType::FolderRenamed => "FolderRenamed",
            EventType::FolderMoved => "FolderMoved",
            EventType::FolderDeleted => "FolderDeleted",
            EventType::ShareCreated => "ShareCreated",
            EventType::ShareRevoked => "ShareRevoked",
            EventType::ShareUpdated => "ShareUpdated",
            EventType::ShareReceivedByUser => "ShareReceivedByUser",
            EventType::SharePermissionChanged => "SharePermissionChanged",
            EventType::ShareRevokedFromUser => "ShareRevokedFromUser",
            EventType::NotificationCreated => "NotificationCreated",
            EventType::ConflictDetected => "ConflictDetected",
            EventType::ConflictResolved => "ConflictResolved",
            EventType::ReplicationStateChanged => "ReplicationStateChanged",
        }
    }
}

/// Event stored in the event store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub event_type: EventType,
    pub aggregate_id: Uuid,
    pub aggregate_type: AggregateType,
    pub payload: JsonValue,
    pub user_id: UserId,
    pub timestamp: DateTime<Utc>,
    pub version: i32,
}

impl Event {
    /// Create a new event
    pub fn new(
        event_type: EventType,
        aggregate_id: Uuid,
        aggregate_type: AggregateType,
        payload: JsonValue,
        user_id: UserId,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            aggregate_id,
            aggregate_type,
            payload,
            user_id,
            timestamp: Utc::now(),
            version: 1,
        }
    }
}

/// File uploaded event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadedPayload {
    pub file_id: FileId,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub content_hash: String,
    pub storage_key: String,
    pub mime_type: String,
    pub owner_id: UserId,
    pub parent_folder_id: Option<FolderId>,
    pub actor_type: String,
    pub actor_user_id: Option<UserId>,
    pub actor_share_id: Option<ShareId>,
    pub actor_share_session_id: Option<Uuid>,
    pub actor_display_name: Option<String>,
}

/// Folder created event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderCreatedPayload {
    pub folder_id: FolderId,
    pub name: String,
    pub path: String,
    pub parent_folder_id: Option<FolderId>,
    pub owner_id: UserId,
}

/// File modified event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileModifiedPayload {
    pub file_id: FileId,
    pub old_version: i32,
    pub new_version: i32,
    pub old_content_hash: String,
    pub new_content_hash: String,
    pub old_size: i64,
    pub new_size: i64,
    pub storage_key: String,
    pub modified_by: UserId,
}

/// Folder renamed event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderRenamedPayload {
    pub folder_id: FolderId,
    pub old_name: String,
    pub new_name: String,
    pub old_path: String,
    pub new_path: String,
    pub renamed_by: UserId,
}

/// Folder moved event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderMovedPayload {
    pub folder_id: FolderId,
    pub old_parent_folder_id: Option<FolderId>,
    pub new_parent_folder_id: Option<FolderId>,
    pub old_path: String,
    pub new_path: String,
    pub moved_by: UserId,
}

/// Folder deleted event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderDeletedPayload {
    pub folder_id: FolderId,
    pub name: String,
    pub path: String,
    pub deleted_by: UserId,
}

/// File deleted event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDeletedPayload {
    pub file_id: FileId,
    pub file_name: String,
    pub folder_id: Option<FolderId>,
}

/// File moved event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRenamedPayload {
    pub file_id: FileId,
    pub old_name: String,
    pub new_name: String,
    pub old_path: String,
    pub new_path: String,
    pub renamed_by: UserId,
}

/// File moved event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMovedPayload {
    pub file_id: FileId,
    pub old_parent_folder_id: Option<FolderId>,
    pub new_parent_folder_id: Option<FolderId>,
    pub old_path: String,
    pub new_path: String,
    pub moved_by: UserId,
}

/// File restored event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRestoredPayload {
    pub file_id: FileId,
    pub old_version: i32,
    pub new_version: i32,
    pub restored_from_version: i32,
    pub content_hash: String,
    pub size: i64,
    pub restored_by: UserId,
}

/// Share created event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareCreatedPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub share_token: String,
    pub permissions: SharePermissions,
    pub password_protected: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: UserId,
}

/// Share revoked event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRevokedPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub revoked_by: UserId,
}

/// Share updated event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareUpdatedPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub password_changed: bool,
    pub expires_at_changed: bool,
    pub new_expires_at: Option<DateTime<Utc>>,
    pub updated_by: UserId,
}

/// Share received by user event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareReceivedByUserPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub received_by: UserId,
    pub shared_by: UserId,
    pub timestamp: DateTime<Utc>,
}

/// Share permission changed event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharePermissionChangedPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub old_permissions: SharePermissions,
    pub new_permissions: SharePermissions,
    pub changed_by: UserId,
    pub timestamp: DateTime<Utc>,
}

/// Share revoked from user event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRevokedFromUserPayload {
    pub share_id: ShareId,
    pub file_id: FileId,
    pub revoked_from: UserId,
    pub revoked_by: UserId,
    pub timestamp: DateTime<Utc>,
}

/// Notification created event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationCreatedPayload {
    pub notification_id: Uuid,
    pub user_id: UserId,
    pub title: String,
    pub message: String,
    pub notification_type: String,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub action_url: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Replication state changed event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStateChangedPayload {
    pub file_id: FileId,
    pub file_version_id: VersionId,
    pub replication_state: ReplicationState,
    pub job_status: Option<String>,
    pub attempt_count: i32,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let payload = serde_json::json!({
            "file_id": file_id.to_string(),
            "name": "test.txt"
        });

        let event = Event::new(
            EventType::FileUploaded,
            file_id,
            AggregateType::File,
            payload,
            user_id,
        );

        assert_eq!(event.event_type, EventType::FileUploaded);
        assert_eq!(event.aggregate_id, file_id);
        assert_eq!(event.version, 1);
    }

    #[test]
    fn test_event_type_serialization() {
        let event_type = EventType::FileUploaded;
        let json = serde_json::to_string(&event_type).unwrap();
        assert_eq!(json, r#"{"type":"FileUploaded"}"#);
    }

    #[test]
    fn test_event_type_name() {
        assert_eq!(EventType::FileUploaded.type_name(), "FileUploaded");
        assert_eq!(EventType::FileModified.type_name(), "FileModified");
        assert_eq!(EventType::FolderCreated.type_name(), "FolderCreated");
        assert_eq!(EventType::ShareCreated.type_name(), "ShareCreated");
        assert_eq!(EventType::ConflictDetected.type_name(), "ConflictDetected");
    }

    #[test]
    fn test_share_event_type_serialization() {
        let event_type = EventType::ShareCreated;
        let json = serde_json::to_string(&event_type).unwrap();
        assert_eq!(json, r#"{"type":"ShareCreated"}"#);

        let event_type = EventType::ShareRevoked;
        let json = serde_json::to_string(&event_type).unwrap();
        assert_eq!(json, r#"{"type":"ShareRevoked"}"#);

        let event_type = EventType::ShareUpdated;
        let json = serde_json::to_string(&event_type).unwrap();
        assert_eq!(json, r#"{"type":"ShareUpdated"}"#);
    }
}
