//! Sync repository for device synchronization
//!
//! Provides operations for managing sync cursors and retrieving
//! delta changes for desktop client synchronization.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single delta item in a sync response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncDelta {
    /// File was created
    FileCreated {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        file_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
        size: i64,
        mime_type: String,
        content_hash: String,
        version_id: Uuid,
    },
    /// File was modified (new version)
    FileModified {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        file_id: Uuid,
        name: String,
        path: String,
        size: i64,
        mime_type: String,
        content_hash: String,
        version_id: Uuid,
        version_number: i32,
    },
    /// File was renamed
    FileRenamed {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        file_id: Uuid,
        old_name: String,
        new_name: String,
        old_path: String,
        new_path: String,
    },
    /// File was moved to a different folder
    FileMoved {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        file_id: Uuid,
        name: String,
        old_parent_id: Option<Uuid>,
        new_parent_id: Option<Uuid>,
        old_path: String,
        new_path: String,
    },
    /// File was deleted
    FileDeleted {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        file_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
    },
    /// File was restored from trash
    FileRestored {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        file_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
    },
    /// Folder was created
    FolderCreated {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        folder_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
    },
    /// Folder was renamed
    FolderRenamed {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        folder_id: Uuid,
        old_name: String,
        new_name: String,
        old_path: String,
        new_path: String,
    },
    /// Folder was moved
    FolderMoved {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        folder_id: Uuid,
        name: String,
        old_parent_id: Option<Uuid>,
        new_parent_id: Option<Uuid>,
        old_path: String,
        new_path: String,
    },
    /// Folder was deleted
    FolderDeleted {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        folder_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
    },
    /// Folder was restored
    FolderRestored {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        folder_id: Uuid,
        name: String,
        path: String,
        parent_id: Option<Uuid>,
    },
    /// Share was created
    ShareCreated {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        share_id: Uuid,
        resource_type: String,
        resource_id: Uuid,
        resource_name: String,
        permissions: String,
        scope: String,
        recipient_user_id: Option<Uuid>,
    },
    /// Share was revoked
    ShareRevoked {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        share_id: Uuid,
        resource_type: String,
        resource_id: Uuid,
    },
    /// Share was updated
    ShareUpdated {
        event_id: Uuid,
        timestamp: DateTime<Utc>,
        share_id: Uuid,
        resource_type: String,
        resource_id: Uuid,
        changes: Vec<String>, // e.g., ["permissions", "expires_at"]
    },
}

/// Result of a delta query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaResult {
    /// Delta items
    pub items: Vec<SyncDelta>,
    /// New cursor for the next page (None if no more items)
    pub next_cursor: Option<String>,
    /// Whether there are more items to fetch
    pub has_more: bool,
    /// Total count of items (may be estimated)
    pub total_count: Option<usize>,
}

impl DeltaResult {
    /// Create an empty delta result
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            next_cursor: None,
            has_more: false,
            total_count: Some(0),
        }
    }

    /// Create a delta result with items
    pub fn with_items(items: Vec<SyncDelta>, has_more: bool) -> Self {
        Self {
            items,
            next_cursor: None, // Will be set by the repository
            has_more,
            total_count: None,
        }
    }
}

/// Sync cursor information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCursor {
    /// User ID
    pub user_id: Uuid,
    /// Device ID
    pub device_id: Uuid,
    /// Cursor token
    pub cursor: String,
    /// Last event ID processed
    pub last_event_id: Uuid,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
}

/// Error type for cursor parsing
#[derive(Debug, Clone, PartialEq)]
pub enum CursorError {
    InvalidFormat,
    InvalidBase64,
    InvalidTimestamp,
    MissingNonce,
}

impl std::fmt::Display for CursorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CursorError::InvalidFormat => write!(f, "Invalid cursor format"),
            CursorError::InvalidBase64 => write!(f, "Invalid base64 encoding"),
            CursorError::InvalidTimestamp => write!(f, "Invalid timestamp in cursor"),
            CursorError::MissingNonce => write!(f, "Missing nonce in cursor"),
        }
    }
}

impl std::error::Error for CursorError {}

/// Parse a cursor token to extract the timestamp
///
/// Cursor format: base64(timestamp_millis + ":" + nonce)
pub fn parse_cursor(cursor: &str) -> Result<DateTime<Utc>, CursorError> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let decoded = STANDARD
        .decode(cursor)
        .map_err(|_| CursorError::InvalidBase64)?;
    let decoded_str = String::from_utf8(decoded).map_err(|_| CursorError::InvalidFormat)?;
    let parts: Vec<&str> = decoded_str.split(':').collect();

    if parts.len() != 2 {
        return Err(CursorError::InvalidFormat);
    }

    let timestamp_millis: i64 = parts[0]
        .parse()
        .map_err(|_| CursorError::InvalidTimestamp)?;

    chrono::DateTime::from_timestamp_millis(timestamp_millis).ok_or(CursorError::InvalidTimestamp)
}

/// Generate a new cursor token for the current time.
///
/// Cursor format: base64(timestamp_millis + ":" + uuid_v4)
pub fn generate_cursor() -> String {
    generate_cursor_at(Utc::now())
}

/// Generate a cursor token for a specific timestamp.
///
/// Cursor format: base64(timestamp_millis + ":" + uuid_v4)
pub fn generate_cursor_at(timestamp: DateTime<Utc>) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let timestamp_millis = timestamp.timestamp_millis();
    let nonce = Uuid::new_v4();
    let token = format!("{}:{}", timestamp_millis, nonce);
    STANDARD.encode(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_generation_and_parsing() {
        let cursor = generate_cursor();
        let parsed = parse_cursor(&cursor);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_cursor_parsing_invalid_base64() {
        let result = parse_cursor("not-valid-base64!!!");
        assert_eq!(result, Err(CursorError::InvalidBase64));
    }

    #[test]
    fn test_cursor_parsing_invalid_format() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let invalid = STANDARD.encode("no-colon-here");
        let result = parse_cursor(&invalid);
        assert_eq!(result, Err(CursorError::InvalidFormat));
    }
}
