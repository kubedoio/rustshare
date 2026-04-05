//! SQLite-based sync journal
//!
//! This module provides a journal-based approach for crash recovery.
//! All operations are transactional to ensure consistency.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::data_dir;

/// Database connection wrapper
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

/// File state tracking
#[derive(Debug, Clone)]
pub struct FileState {
    /// Server file ID
    pub file_id: Uuid,
    /// Server folder ID containing this file
    pub folder_id: Uuid,
    /// Local file path (relative to sync root)
    pub local_path: PathBuf,
    /// Server-side path
    pub server_path: String,
    /// File name
    pub name: String,
    /// Content hash (SHA-256)
    pub content_hash: String,
    /// File size in bytes
    pub size: i64,
    /// Last modified time (local)
    pub local_modified_at: DateTime<Utc>,
    /// Last modified time (server)
    pub server_modified_at: DateTime<Utc>,
    /// Version number on server
    pub version: i32,
    /// Whether the file is deleted locally
    pub is_deleted: bool,
}

/// Sync queue item type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncQueueItemType {
    Upload,
    Download,
    DeleteLocal,
    DeleteRemote,
    Rename,
}

/// Sync queue item
#[derive(Debug, Clone)]
pub struct SyncQueueItem {
    pub id: i64,
    pub item_type: SyncQueueItemType,
    pub file_id: Option<Uuid>,
    pub folder_id: Uuid,
    pub local_path: PathBuf,
    pub attempt_count: i32,
    pub created_at: DateTime<Utc>,
    pub retry_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

/// Sync cursor for a folder
#[derive(Debug, Clone)]
pub struct SyncCursor {
    pub folder_id: Uuid,
    pub cursor_token: String,
    pub last_event_id: Uuid,
    pub updated_at: DateTime<Utc>,
}

impl Database {
    /// Open or create the database
    pub fn open() -> Result<Self> {
        let db_path = data_dir()?.join("sync.db");
        let conn = Connection::open(&db_path)?;
        
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        
        tracing::debug!("Database opened at {}", db_path.display());
        Ok(db)
    }

    /// Open database at a specific path (for testing)
    pub fn open_at(path: &PathBuf) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        Ok(db)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Sync folders - which folders are synced locally
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS sync_folders (
                folder_id TEXT PRIMARY KEY,
                local_root_path TEXT NOT NULL,
                server_path TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                direction TEXT NOT NULL DEFAULT 'bidirectional',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
            [],
        )?;

        // File states - local file paths with server file IDs
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS file_states (
                file_id TEXT PRIMARY KEY,
                folder_id TEXT NOT NULL,
                local_path TEXT NOT NULL,
                server_path TEXT NOT NULL,
                name TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                size INTEGER NOT NULL,
                local_modified_at TEXT NOT NULL,
                server_modified_at TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                is_deleted INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (folder_id) REFERENCES sync_folders(folder_id),
                UNIQUE(folder_id, local_path)
            )
            "#,
            [],
        )?;

        // Sync queue - pending uploads/downloads
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS sync_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                item_type TEXT NOT NULL,
                file_id TEXT,
                folder_id TEXT NOT NULL,
                local_path TEXT NOT NULL,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                retry_at TEXT,
                error_message TEXT,
                FOREIGN KEY (folder_id) REFERENCES sync_folders(folder_id)
            )
            "#,
            [],
        )?;

        // Cursors - last sync cursor per folder
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS sync_cursors (
                folder_id TEXT PRIMARY KEY,
                cursor_token TEXT NOT NULL,
                last_event_id TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (folder_id) REFERENCES sync_folders(folder_id)
            )
            "#,
            [],
        )?;

        // Upload progress for resumable uploads
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS upload_progress (
                file_id TEXT PRIMARY KEY,
                upload_url TEXT,
                bytes_uploaded INTEGER NOT NULL DEFAULT 0,
                total_bytes INTEGER NOT NULL,
                chunk_size INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
            [],
        )?;

        // Create indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_file_states_folder ON file_states(folder_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_file_states_path ON file_states(local_path)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sync_queue_folder ON sync_queue(folder_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sync_queue_retry ON sync_queue(retry_at)",
            [],
        )?;

        tracing::debug!("Database schema initialized");
        Ok(())
    }

    // =========================================================================
    // Sync Folders
    // =========================================================================

    /// Add a synced folder
    pub fn add_sync_folder(
        &self,
        folder_id: Uuid,
        local_root_path: &PathBuf,
        server_path: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO sync_folders 
            (folder_id, local_root_path, server_path, enabled, direction, created_at, updated_at)
            VALUES (?1, ?2, ?3, 1, 'bidirectional', ?4, ?4)
            "#,
            params![
                folder_id.to_string(),
                local_root_path.to_string_lossy().to_string(),
                server_path,
                now
            ],
        )?;
        Ok(())
    }

    /// Remove a synced folder
    pub fn remove_sync_folder(&self, folder_id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM sync_folders WHERE folder_id = ?1",
            params![folder_id.to_string()],
        )?;
        Ok(())
    }

    /// Get all synced folders
    pub fn get_sync_folders(&self) -> Result<Vec<(Uuid, PathBuf, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT folder_id, local_root_path, server_path FROM sync_folders WHERE enabled = 1"
        )?;

        let rows = stmt.query_map([], |row| {
            let folder_id_str: String = row.get(0)?;
            let local_path_str: String = row.get(1)?;
            let server_path: String = row.get(2)?;
            
            Ok((
                Uuid::parse_str(&folder_id_str).unwrap_or_else(|_| Uuid::nil()),
                PathBuf::from(local_path_str),
                server_path,
            ))
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))
    }

    // =========================================================================
    // File States
    // =========================================================================

    /// Get file state by local path
    pub fn get_file_state(&self, folder_id: Uuid, local_path: &PathBuf) -> Result<Option<FileState>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT file_id, folder_id, local_path, server_path, name, 
                   content_hash, size, local_modified_at, server_modified_at, 
                   version, is_deleted
            FROM file_states 
            WHERE folder_id = ?1 AND local_path = ?2 AND is_deleted = 0
            "#
        )?;

        let local_path_str = local_path.to_string_lossy().to_string();
        let result = stmt.query_row(
            params![folder_id.to_string(), local_path_str],
            |row| Self::row_to_file_state(row),
        ).optional()?;

        Ok(result)
    }

    /// Get file state by server file ID
    pub fn get_file_state_by_id(&self, file_id: Uuid) -> Result<Option<FileState>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT file_id, folder_id, local_path, server_path, name, 
                   content_hash, size, local_modified_at, server_modified_at, 
                   version, is_deleted
            FROM file_states 
            WHERE file_id = ?1 AND is_deleted = 0
            "#
        )?;

        let result = stmt.query_row(
            params![file_id.to_string()],
            |row| Self::row_to_file_state(row),
        ).optional()?;

        Ok(result)
    }

    /// Set or update file state
    pub fn set_file_state(&self, state: &FileState) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO file_states 
            (file_id, folder_id, local_path, server_path, name, content_hash, size,
             local_modified_at, server_modified_at, version, is_deleted, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                state.file_id.to_string(),
                state.folder_id.to_string(),
                state.local_path.to_string_lossy().to_string(),
                state.server_path,
                state.name,
                state.content_hash,
                state.size,
                state.local_modified_at.to_rfc3339(),
                state.server_modified_at.to_rfc3339(),
                state.version,
                state.is_deleted as i32,
                now,
            ],
        )?;
        Ok(())
    }

    /// Mark a file as deleted
    pub fn mark_file_deleted(&self, file_id: Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE file_states SET is_deleted = 1, updated_at = ?1 WHERE file_id = ?2",
            params![now, file_id.to_string()],
        )?;
        Ok(())
    }

    /// Delete a file state permanently
    pub fn delete_file_state(&self, file_id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM file_states WHERE file_id = ?1",
            params![file_id.to_string()],
        )?;
        Ok(())
    }

    /// Get all file states in a folder
    pub fn get_folder_file_states(&self, folder_id: Uuid) -> Result<Vec<FileState>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT file_id, folder_id, local_path, server_path, name, 
                   content_hash, size, local_modified_at, server_modified_at, 
                   version, is_deleted
            FROM file_states 
            WHERE folder_id = ?1 AND is_deleted = 0
            "#
        )?;

        let rows = stmt.query_map(
            params![folder_id.to_string()],
            |row| Self::row_to_file_state(row),
        )?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))
    }

    fn row_to_file_state(row: &rusqlite::Row) -> std::result::Result<FileState, rusqlite::Error> {
        let file_id_str: String = row.get(0)?;
        let folder_id_str: String = row.get(1)?;
        let local_path_str: String = row.get(2)?;
        
        Ok(FileState {
            file_id: Uuid::parse_str(&file_id_str).unwrap_or_else(|_| Uuid::nil()),
            folder_id: Uuid::parse_str(&folder_id_str).unwrap_or_else(|_| Uuid::nil()),
            local_path: PathBuf::from(local_path_str),
            server_path: row.get(3)?,
            name: row.get(4)?,
            content_hash: row.get(5)?,
            size: row.get(6)?,
            local_modified_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            server_modified_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            version: row.get(9)?,
            is_deleted: row.get::<_, i32>(10)? != 0,
        })
    }

    // =========================================================================
    // Sync Queue
    // =========================================================================

    /// Add item to sync queue
    pub fn enqueue(&self, item_type: SyncQueueItemType, file_id: Option<Uuid>, folder_id: Uuid, local_path: &PathBuf) -> Result<()> {
        let now = Utc::now();
        let type_str = match item_type {
            SyncQueueItemType::Upload => "upload",
            SyncQueueItemType::Download => "download",
            SyncQueueItemType::DeleteLocal => "delete_local",
            SyncQueueItemType::DeleteRemote => "delete_remote",
            SyncQueueItemType::Rename => "rename",
        };

        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO sync_queue (item_type, file_id, folder_id, local_path, attempt_count, created_at, retry_at)
            VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)
            "#,
            params![
                type_str,
                file_id.map(|id| id.to_string()),
                folder_id.to_string(),
                local_path.to_string_lossy().to_string(),
                now.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Get pending items from sync queue
    pub fn get_pending_queue_items(&self, limit: usize) -> Result<Vec<SyncQueueItem>> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"
            SELECT id, item_type, file_id, folder_id, local_path, 
                   attempt_count, created_at, retry_at, error_message
            FROM sync_queue 
            WHERE retry_at IS NULL OR retry_at <= ?1
            ORDER BY created_at ASC
            LIMIT ?2
            "#
        )?;

        let rows = stmt.query_map(
            params![now, limit as i64],
            |row| Self::row_to_queue_item(row),
        )?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Database error: {}", e))
    }

    /// Mark queue item as failed with retry
    pub fn mark_queue_item_failed(&self, id: i64, error: &str, retry_at: DateTime<Utc>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            UPDATE sync_queue 
            SET attempt_count = attempt_count + 1, 
                error_message = ?1, 
                retry_at = ?2
            WHERE id = ?3
            "#,
            params![error, retry_at.to_rfc3339(), id],
        )?;
        Ok(())
    }

    /// Remove item from sync queue (on success)
    pub fn remove_from_queue(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM sync_queue WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Get queue statistics
    pub fn get_queue_stats(&self) -> Result<(usize, usize)> {
        let conn = self.conn.lock().unwrap();
        
        // Pending count
        let now = Utc::now().to_rfc3339();
        let pending: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sync_queue WHERE retry_at IS NULL OR retry_at <= ?1",
            params![now],
            |row| row.get(0),
        )?;

        // Failed count (items with attempts > 0)
        let failed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sync_queue WHERE attempt_count > 0",
            [],
            |row| row.get(0),
        )?;

        Ok((pending as usize, failed as usize))
    }

    fn row_to_queue_item(row: &rusqlite::Row) -> std::result::Result<SyncQueueItem, rusqlite::Error> {
        let type_str: String = row.get(1)?;
        let item_type = match type_str.as_str() {
            "upload" => SyncQueueItemType::Upload,
            "download" => SyncQueueItemType::Download,
            "delete_local" => SyncQueueItemType::DeleteLocal,
            "delete_remote" => SyncQueueItemType::DeleteRemote,
            "rename" => SyncQueueItemType::Rename,
            _ => SyncQueueItemType::Upload,
        };

        let file_id_str: Option<String> = row.get(2)?;
        let folder_id_str: String = row.get(3)?;
        let local_path_str: String = row.get(4)?;
        let retry_at_str: Option<String> = row.get(7)?;

        Ok(SyncQueueItem {
            id: row.get(0)?,
            item_type,
            file_id: file_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
            folder_id: Uuid::parse_str(&folder_id_str).unwrap_or_else(|_| Uuid::nil()),
            local_path: PathBuf::from(local_path_str),
            attempt_count: row.get(5)?,
            created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            retry_at: retry_at_str.and_then(|s| {
                DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)).ok()
            }),
            error_message: row.get(8)?,
        })
    }

    // =========================================================================
    // Sync Cursors
    // =========================================================================

    /// Get cursor for a folder
    pub fn get_cursor(&self, folder_id: Uuid) -> Result<Option<SyncCursor>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            r#"
            SELECT folder_id, cursor_token, last_event_id, updated_at
            FROM sync_cursors WHERE folder_id = ?1
            "#,
            params![folder_id.to_string()],
            |row| {
                let folder_id_str: String = row.get(0)?;
                let last_event_id_str: String = row.get(2)?;
                let updated_at_str: String = row.get(3)?;
                
                Ok(SyncCursor {
                    folder_id: Uuid::parse_str(&folder_id_str).unwrap_or_else(|_| Uuid::nil()),
                    cursor_token: row.get(1)?,
                    last_event_id: Uuid::parse_str(&last_event_id_str).unwrap_or_else(|_| Uuid::nil()),
                    updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            },
        ).optional()?;

        Ok(result)
    }

    /// Set cursor for a folder
    pub fn set_cursor(&self, cursor: &SyncCursor) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO sync_cursors 
            (folder_id, cursor_token, last_event_id, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![
                cursor.folder_id.to_string(),
                cursor.cursor_token,
                cursor.last_event_id.to_string(),
                cursor.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Reset cursor for a folder (full re-sync)
    pub fn reset_cursor(&self, folder_id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM sync_cursors WHERE folder_id = ?1",
            params![folder_id.to_string()],
        )?;
        Ok(())
    }

    // =========================================================================
    // Upload Progress (Resumable Uploads)
    // =========================================================================

    /// Save upload progress
    pub fn save_upload_progress(
        &self,
        file_id: Uuid,
        upload_url: Option<&str>,
        bytes_uploaded: i64,
        total_bytes: i64,
        chunk_size: i64,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO upload_progress 
            (file_id, upload_url, bytes_uploaded, total_bytes, chunk_size, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                file_id.to_string(),
                upload_url,
                bytes_uploaded,
                total_bytes,
                chunk_size,
                now,
            ],
        )?;
        Ok(())
    }

    /// Get upload progress
    pub fn get_upload_progress(&self, file_id: Uuid) -> Result<Option<(Option<String>, i64, i64, i64)>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT upload_url, bytes_uploaded, total_bytes, chunk_size FROM upload_progress WHERE file_id = ?1",
            params![file_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        ).optional()?;

        Ok(result)
    }

    /// Clear upload progress (on completion or cancellation)
    pub fn clear_upload_progress(&self, file_id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM upload_progress WHERE file_id = ?1",
            params![file_id.to_string()],
        )?;
        Ok(())
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
    fn test_sync_folder_operations() {
        let (db, _temp) = create_test_db();
        
        let folder_id = Uuid::new_v4();
        let local_path: PathBuf = "/test/folder".into();
        
        db.add_sync_folder(folder_id, &local_path, "/server/path").unwrap();
        
        let folders = db.get_sync_folders().unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].0, folder_id);
        
        db.remove_sync_folder(folder_id).unwrap();
        let folders = db.get_sync_folders().unwrap();
        assert!(folders.is_empty());
    }

    #[test]
    fn test_file_state_operations() {
        let (db, _temp) = create_test_db();
        
        let folder_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        
        let state = FileState {
            file_id,
            folder_id,
            local_path: "test.txt".into(),
            server_path: "/folder/test.txt".to_string(),
            name: "test.txt".to_string(),
            content_hash: "abc123".to_string(),
            size: 100,
            local_modified_at: Utc::now(),
            server_modified_at: Utc::now(),
            version: 1,
            is_deleted: false,
        };
        
        db.set_file_state(&state).unwrap();
        
        let retrieved = db.get_file_state(folder_id, &"test.txt".into()).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().file_id, file_id);
        
        // Mark deleted
        db.mark_file_deleted(file_id).unwrap();
        let retrieved = db.get_file_state(folder_id, &"test.txt".into()).unwrap();
        assert!(retrieved.is_none()); // Deleted files are filtered
    }

    #[test]
    fn test_sync_queue_operations() {
        let (db, _temp) = create_test_db();
        
        let folder_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        
        db.add_sync_folder(folder_id, &"/test".into(), "/server").unwrap();
        
        db.enqueue(SyncQueueItemType::Upload, Some(file_id), folder_id, &"test.txt".into()).unwrap();
        
        let pending = db.get_pending_queue_items(10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].item_type, SyncQueueItemType::Upload);
        
        // Remove from queue
        db.remove_from_queue(pending[0].id).unwrap();
        let pending = db.get_pending_queue_items(10).unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_cursor_operations() {
        let (db, _temp) = create_test_db();
        
        let folder_id = Uuid::new_v4();
        
        let cursor = SyncCursor {
            folder_id,
            cursor_token: "cursor123".to_string(),
            last_event_id: Uuid::new_v4(),
            updated_at: Utc::now(),
        };
        
        db.set_cursor(&cursor).unwrap();
        
        let retrieved = db.get_cursor(folder_id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().cursor_token, "cursor123");
        
        // Reset cursor
        db.reset_cursor(folder_id).unwrap();
        let retrieved = db.get_cursor(folder_id).unwrap();
        assert!(retrieved.is_none());
    }
}
