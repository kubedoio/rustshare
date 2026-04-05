//! Cursor management for incremental sync
//!
//! Cursors track the last synced position in the server's event log,
//! allowing the client to efficiently fetch only changes since the last sync.

use anyhow::Result;
use chrono::{DateTime, Utc};
use tracing::{info, trace};
use uuid::Uuid;

use crate::db::{Database, SyncCursor};

/// Cursor manager handles getting, creating, and updating sync cursors
#[derive(Clone)]
pub struct CursorManager {
    db: Database,
}

impl CursorManager {
    /// Create a new cursor manager
    pub fn new(db: &Database) -> Self {
        Self { db: db.clone() }
    }

    /// Get or create a cursor for a folder
    ///
    /// If no cursor exists, creates a new one starting from the current time.
    pub async fn get_or_create_cursor(&self, folder_id: Uuid) -> Result<SyncCursor> {
        // Try to load existing cursor
        if let Some(cursor) = self.db.get_cursor(folder_id)? {
            trace!("Loaded existing cursor for folder {}: {}", folder_id, cursor.cursor_token);
            return Ok(cursor);
        }

        // Create new cursor
        let now = Utc::now();
        let cursor_token = Self::generate_cursor(now);
        let cursor = SyncCursor {
            folder_id,
            cursor_token,
            last_event_id: Uuid::nil(),
            updated_at: now,
        };

        // Save to database
        self.db.set_cursor(&cursor)?;
        
        info!("Created new cursor for folder {}: {}", folder_id, cursor.cursor_token);
        
        Ok(cursor)
    }

    /// Update the cursor position after processing deltas
    pub fn update_cursor(&self, folder_id: Uuid, cursor_token: String, last_event_id: Uuid) -> Result<()> {
        let cursor = SyncCursor {
            folder_id,
            cursor_token,
            last_event_id,
            updated_at: Utc::now(),
        };

        self.db.set_cursor(&cursor)?;
        
        trace!("Updated cursor for folder {}: {}", folder_id, cursor.cursor_token);
        
        Ok(())
    }

    /// Reset a cursor (force full re-sync)
    pub fn reset_cursor(&self, folder_id: Uuid) -> Result<()> {
        self.db.reset_cursor(folder_id)?;
        
        info!("Reset cursor for folder {}", folder_id);
        
        Ok(())
    }

    /// Get the current cursor for a folder (if exists)
    pub fn get_cursor(&self, folder_id: Uuid) -> Result<Option<SyncCursor>> {
        self.db.get_cursor(folder_id)
    }

    /// Generate a new cursor token from a timestamp
    ///
    /// The cursor format is a base64-encoded timestamp that the server
    /// can use to determine which events to return.
    fn generate_cursor(timestamp: DateTime<Utc>) -> String {
        // Format: "v1:" + base64 encoded timestamp
        let timestamp_str = timestamp.to_rfc3339();
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(timestamp_str);
        format!("v1:{}", encoded)
    }

    /// Parse a cursor token to extract the timestamp
    pub fn parse_cursor(cursor: &str) -> Result<DateTime<Utc>> {
        if let Some(encoded) = cursor.strip_prefix("v1:") {
            use base64::Engine;
            let decoded = base64::engine::general_purpose::STANDARD.decode(encoded)?;
            let timestamp_str = String::from_utf8(decoded)?;
            let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)?;
            Ok(timestamp.with_timezone(&Utc))
        } else {
            anyhow::bail!("Invalid cursor format")
        }
    }
}

/// Cursor validation
#[derive(Debug)]
pub struct CursorValidation {
    pub is_valid: bool,
    pub reason: Option<String>,
}

/// Validates a cursor token
pub fn validate_cursor(cursor: &str) -> CursorValidation {
    if !cursor.starts_with("v1:") {
        return CursorValidation {
            is_valid: false,
            reason: Some("Invalid cursor version".to_string()),
        };
    }

    match CursorManager::parse_cursor(cursor) {
        Ok(timestamp) => {
            // Check if cursor is not too old (e.g., server may have expired old events)
            let age = Utc::now() - timestamp;
            if age.num_days() > 30 {
                CursorValidation {
                    is_valid: false,
                    reason: Some("Cursor too old, full sync required".to_string()),
                }
            } else {
                CursorValidation {
                    is_valid: true,
                    reason: None,
                }
            }
        }
        Err(e) => CursorValidation {
            is_valid: false,
            reason: Some(format!("Failed to parse cursor: {}", e)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Database::open_at(&db_path).unwrap();
        (db, temp_dir)
    }

    #[test]
    fn test_cursor_generation_and_parsing() {
        let now = Utc::now();
        let cursor = CursorManager::generate_cursor(now);
        
        assert!(cursor.starts_with("v1:"));
        
        let parsed = CursorManager::parse_cursor(&cursor).unwrap();
        // Compare with some tolerance for sub-millisecond differences
        assert!((parsed - now).num_milliseconds().abs() < 1000);
    }

    #[test]
    fn test_cursor_validation() {
        let valid_cursor = CursorManager::generate_cursor(Utc::now());
        let validation = validate_cursor(&valid_cursor);
        assert!(validation.is_valid);

        let old_timestamp = Utc::now() - chrono::Duration::days(31);
        let old_cursor = CursorManager::generate_cursor(old_timestamp);
        let validation = validate_cursor(&old_cursor);
        assert!(!validation.is_valid);
    }
}
