use anyhow::Result;
use chrono::{TimeZone, Utc};
use rusqlite::{params, Connection, Transaction};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use sync_domain::{EntryType, HydrationState, LocalEntry, SyncRoot};
use uuid::Uuid;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init(&conn)?;
        Ok(Self { conn })
    }

    fn init(conn: &Connection) -> Result<()> {
        // Ensure WAL mode for better concurrency
        let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS sync_roots (
                id BLOB PRIMARY KEY,
                remote_path TEXT NOT NULL,
                local_path TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS local_inventory (
                path TEXT PRIMARY KEY,
                entry_type TEXT NOT NULL,
                size INTEGER NOT NULL,
                hash TEXT NOT NULL,
                mtime_ms INTEGER NOT NULL,
                last_synced_version TEXT,
                hydration_state TEXT NOT NULL DEFAULT 'materialized'
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS sync_cursor (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                cursor TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS conflicts (
                id BLOB PRIMARY KEY,
                local_path TEXT NOT NULL,
                remote_version TEXT NOT NULL,
                conflict_path TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS activity_log (
                id BLOB PRIMARY KEY,
                event_type TEXT NOT NULL,
                path TEXT NOT NULL,
                status TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS sync_filters (
                id BLOB PRIMARY KEY,
                root_id BLOB NOT NULL,
                pattern TEXT NOT NULL,
                filter_type TEXT NOT NULL,
                FOREIGN KEY(root_id) REFERENCES sync_roots(id)
            )",
            [],
        )?;

        Self::create_sync_tables(conn)?;

        Ok(())
    }

    fn create_sync_tables(conn: &Connection) -> Result<()> {
        // File states - track sync state for each file
        conn.execute(
            "CREATE TABLE IF NOT EXISTS file_states (
                id INTEGER PRIMARY KEY,
                root_id BLOB NOT NULL,
                relative_path TEXT NOT NULL,
                local_hash TEXT,
                remote_hash TEXT,
                remote_file_id BLOB,
                local_modified_at INTEGER,
                remote_modified_at INTEGER,
                size INTEGER,
                is_directory BOOLEAN DEFAULT 0,
                sync_status TEXT DEFAULT 'synced',
                tombstone_side TEXT,
                tombstone_at INTEGER,
                last_sync_at INTEGER,
                UNIQUE(root_id, relative_path)
            )",
            [],
        )?;
        Self::ensure_file_states_columns(conn)?;

        // Sync queue - pending operations
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sync_queue (
                id INTEGER PRIMARY KEY,
                root_id BLOB NOT NULL,
                operation TEXT NOT NULL,
                relative_path TEXT NOT NULL,
                priority INTEGER DEFAULT 0,
                retry_count INTEGER DEFAULT 0,
                last_error TEXT,
                created_at INTEGER,
                execute_at INTEGER
            )",
            [],
        )?;

        // Upload sessions - resumable uploads
        conn.execute(
            "CREATE TABLE IF NOT EXISTS upload_sessions (
                id INTEGER PRIMARY KEY,
                file_state_id INTEGER UNIQUE,
                session_id TEXT,
                total_chunks INTEGER,
                uploaded_chunks INTEGER DEFAULT 0,
                chunk_size INTEGER DEFAULT 5242880,
                expires_at INTEGER
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS broken_remote_entries (
                root_id BLOB NOT NULL,
                relative_path TEXT NOT NULL,
                remote_file_id BLOB NOT NULL,
                error TEXT NOT NULL,
                first_seen_at INTEGER NOT NULL,
                last_seen_at INTEGER NOT NULL,
                PRIMARY KEY (root_id, relative_path, remote_file_id)
            )",
            [],
        )?;

        // Sync cursors - delta tracking per root
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sync_cursors (
                root_id BLOB PRIMARY KEY,
                cursor TEXT,
                updated_at INTEGER
            )",
            [],
        )?;

        Ok(())
    }

    fn ensure_file_states_columns(conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare("PRAGMA table_info(file_states)")?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let existing: HashSet<String> = columns.into_iter().collect();

        if !existing.contains("remote_file_id") {
            conn.execute("ALTER TABLE file_states ADD COLUMN remote_file_id BLOB", [])?;
        }
        if !existing.contains("tombstone_side") {
            conn.execute("ALTER TABLE file_states ADD COLUMN tombstone_side TEXT", [])?;
        }
        if !existing.contains("tombstone_at") {
            conn.execute("ALTER TABLE file_states ADD COLUMN tombstone_at INTEGER", [])?;
        }

        Ok(())
    }

    pub fn transaction(&mut self) -> Result<Transaction<'_>> {
        Ok(self.conn.transaction()?)
    }

    pub fn save_sync_root(&self, root: &SyncRoot) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sync_roots (id, remote_path, local_path) VALUES (?, ?, ?)",
            params![
                root.id.as_bytes(),
                root.remote_path,
                root.local_path.to_str().unwrap()
            ],
        )?;
        Ok(())
    }

    pub fn get_sync_roots(&self) -> Result<Vec<SyncRoot>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, remote_path, local_path FROM sync_roots")?;
        let roots = stmt.query_map([], |row| {
            let id_bytes: Vec<u8> = row.get(0)?;
            Ok(SyncRoot {
                id: Uuid::from_slice(&id_bytes).unwrap_or_else(|_| Uuid::nil()),
                remote_path: row.get(1)?,
                local_path: PathBuf::from(row.get::<_, String>(2)?),
            })
        })?;

        let mut result = Vec::new();
        for root in roots {
            result.push(root?);
        }
        Ok(result)
    }

    pub fn remove_sync_root(&self, root_id: Uuid) -> Result<bool> {
        // Delete associated filters first
        self.conn.execute(
            "DELETE FROM sync_filters WHERE root_id = ?",
            params![root_id.as_bytes()],
        )?;

        // Delete the sync root
        let deleted = self.conn.execute(
            "DELETE FROM sync_roots WHERE id = ?",
            params![root_id.as_bytes()],
        )?;

        Ok(deleted > 0)
    }

    pub fn update_inventory(&self, entry: &LocalEntry) -> Result<()> {
        let entry_type = match entry.entry_type {
            EntryType::File => "file",
            EntryType::Directory => "directory",
        };

        self.conn.execute(
            "INSERT OR REPLACE INTO local_inventory (path, entry_type, size, hash, mtime_ms, last_synced_version, hydration_state) 
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                entry.path.to_str().unwrap(),
                entry_type,
                entry.size as i64,
                entry.hash,
                entry.mtime.timestamp_millis(),
                entry.last_synced_version,
                format!("{:?}", entry.hydration_state).to_lowercase(),
            ],
        )?;
        Ok(())
    }

    pub fn get_inventory_entry(&self, path: &Path) -> Result<Option<LocalEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, entry_type, size, hash, mtime_ms, last_synced_version, hydration_state FROM local_inventory WHERE path = ?"
        )?;

        let entry = stmt
            .query_row(params![path.to_str().unwrap()], |row| {
                let entry_type_str: String = row.get(1)?;
                let entry_type = if entry_type_str == "file" {
                    EntryType::File
                } else {
                    EntryType::Directory
                };
                let mtime_ms: i64 = row.get(4)?;

                let hydration_str: String = row.get(6)?;
                let hydration_state = match hydration_str.as_str() {
                    "placeholder" => HydrationState::Placeholder,
                    "pinned" => HydrationState::Pinned,
                    _ => HydrationState::Materialized,
                };

                Ok(LocalEntry {
                    path: PathBuf::from(row.get::<_, String>(0)?),
                    entry_type,
                    size: row.get::<_, i64>(2)? as u64,
                    hash: row.get(3)?,
                    mtime: Utc.timestamp_millis_opt(mtime_ms).unwrap(),
                    last_synced_version: row.get(5)?,
                    hydration_state,
                })
            })
            .ok();

        Ok(entry)
    }

    pub fn get_sync_cursor(&self) -> Result<Option<String>> {
        let res: Result<String, _> =
            self.conn
                .query_row("SELECT cursor FROM sync_cursor WHERE id = 1", [], |row| {
                    row.get(0)
                });
        match res {
            Ok(cursor) => Ok(Some(cursor)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_sync_cursor(&self, cursor: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sync_cursor (id, cursor) VALUES (1, ?)",
            params![cursor],
        )?;
        Ok(())
    }

    pub fn add_filter(&self, root_id: Uuid, pattern: &str, filter_type: &str) -> Result<()> {
        let id = Uuid::new_v4();
        self.conn.execute(
            "INSERT INTO sync_filters (id, root_id, pattern, filter_type) VALUES (?, ?, ?, ?)",
            params![id.as_bytes(), root_id.as_bytes(), pattern, filter_type],
        )?;
        Ok(())
    }

    pub fn get_filters(&self, root_id: Uuid) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT pattern FROM sync_filters WHERE root_id = ? AND filter_type = 'exclude'",
        )?;
        let rows = stmt.query_map(params![root_id.as_bytes()], |row| row.get::<_, String>(0))?;

        let mut patterns = Vec::new();
        for row in rows {
            patterns.push(row?);
        }
        Ok(patterns)
    }

    // ============================================================================
    // File States (Sync Engine)
    // ============================================================================

    pub fn get_file_state(&self, root_id: Uuid, relative_path: &Path) -> Result<Option<FileState>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, local_hash, remote_hash, local_modified_at, remote_modified_at, 
                    remote_file_id, size, is_directory, sync_status, tombstone_side, tombstone_at, last_sync_at 
             FROM file_states WHERE root_id = ? AND relative_path = ?"
        )?;

        let result = stmt.query_row(
            params![root_id.as_bytes(), relative_path.to_str().unwrap()],
            |row| {
                let remote_file_id: Option<Vec<u8>> = row.get(5)?;
                Ok(FileState {
                    id: row.get(0)?,
                    root_id,
                    relative_path: relative_path.to_path_buf(),
                    local_hash: row.get(1)?,
                    remote_hash: row.get(2)?,
                    local_modified_at: row.get(3)?,
                    remote_modified_at: row.get(4)?,
                    remote_file_id: remote_file_id
                        .and_then(|bytes| Uuid::from_slice(&bytes).ok()),
                    size: row.get(6)?,
                    is_directory: row.get(7)?,
                    sync_status: row.get(8)?,
                    tombstone_side: row.get(9)?,
                    tombstone_at: row.get(10)?,
                    last_sync_at: row.get(11)?,
                })
            }
        );

        match result {
            Ok(state) => Ok(Some(state)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn upsert_file_state(&self, state: &FileState) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO file_states 
             (root_id, relative_path, local_hash, remote_hash, remote_file_id, local_modified_at, 
              remote_modified_at, size, is_directory, sync_status, tombstone_side, tombstone_at, last_sync_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(root_id, relative_path) DO UPDATE SET
             local_hash = excluded.local_hash,
             remote_hash = excluded.remote_hash,
             remote_file_id = excluded.remote_file_id,
             local_modified_at = excluded.local_modified_at,
             remote_modified_at = excluded.remote_modified_at,
             size = excluded.size,
             is_directory = excluded.is_directory,
             sync_status = excluded.sync_status,
             tombstone_side = excluded.tombstone_side,
             tombstone_at = excluded.tombstone_at,
             last_sync_at = excluded.last_sync_at",
            params![
                state.root_id.as_bytes(),
                state.relative_path.to_str().unwrap(),
                state.local_hash,
                state.remote_hash,
                state.remote_file_id.map(|id| id.as_bytes().to_vec()),
                state.local_modified_at,
                state.remote_modified_at,
                state.size,
                state.is_directory,
                state.sync_status,
                state.tombstone_side,
                state.tombstone_at,
                state.last_sync_at,
            ],
        )?;
        
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete_file_state(&self, root_id: Uuid, relative_path: &Path) -> Result<bool> {
        let deleted = self.conn.execute(
            "DELETE FROM file_states WHERE root_id = ? AND relative_path = ?",
            params![root_id.as_bytes(), relative_path.to_str().unwrap()],
        )?;
        Ok(deleted > 0)
    }

    /// Get all file states for a sync root
    pub fn get_all_file_states(&self, root_id: Uuid) -> Result<Vec<FileState>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, relative_path, local_hash, remote_hash, local_modified_at, 
                    remote_file_id, remote_modified_at, size, is_directory, sync_status, tombstone_side, tombstone_at, last_sync_at 
             FROM file_states 
             WHERE root_id = ?"
        )?;

        let rows = stmt.query_map(params![root_id.as_bytes()], |row| {
            let remote_file_id: Option<Vec<u8>> = row.get(5)?;
            Ok(FileState {
                id: row.get(0)?,
                root_id,
                relative_path: PathBuf::from(row.get::<_, String>(1)?),
                local_hash: row.get(2)?,
                remote_hash: row.get(3)?,
                local_modified_at: row.get(4)?,
                remote_file_id: remote_file_id
                    .and_then(|bytes| Uuid::from_slice(&bytes).ok()),
                remote_modified_at: row.get(6)?,
                size: row.get(7)?,
                is_directory: row.get(8)?,
                sync_status: row.get(9)?,
                tombstone_side: row.get(10)?,
                tombstone_at: row.get(11)?,
                last_sync_at: row.get(12)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // ============================================================================
    // Upload Sessions (Resumable Uploads)
    // ============================================================================

    pub fn get_upload_session(&self, file_state_id: i64) -> Result<Option<UploadSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, total_chunks, uploaded_chunks, chunk_size, expires_at 
             FROM upload_sessions WHERE file_state_id = ?"
        )?;

        let result = stmt.query_row([file_state_id], |row| {
            Ok(UploadSession {
                id: row.get(0)?,
                file_state_id,
                session_id: row.get(1)?,
                total_chunks: row.get(2)?,
                uploaded_chunks: row.get(3)?,
                chunk_size: row.get(4)?,
                expires_at: row.get(5)?,
            })
        });

        match result {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn create_upload_session(&self, session: &UploadSession) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO upload_sessions 
             (file_state_id, session_id, total_chunks, uploaded_chunks, chunk_size, expires_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(file_state_id) DO UPDATE SET
             session_id = excluded.session_id,
             total_chunks = excluded.total_chunks,
             uploaded_chunks = excluded.uploaded_chunks,
             chunk_size = excluded.chunk_size,
             expires_at = excluded.expires_at",
            params![
                session.file_state_id,
                session.session_id,
                session.total_chunks,
                session.uploaded_chunks,
                session.chunk_size,
                session.expires_at,
            ],
        )?;
        
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_uploaded_chunks(&self, session_id: &str, uploaded_chunks: i32) -> Result<bool> {
        let updated = self.conn.execute(
            "UPDATE upload_sessions SET uploaded_chunks = ? WHERE session_id = ?",
            params![uploaded_chunks, session_id],
        )?;
        Ok(updated > 0)
    }

    pub fn delete_upload_session(&self, session_id: &str) -> Result<bool> {
        let deleted = self.conn.execute(
            "DELETE FROM upload_sessions WHERE session_id = ?",
            [session_id],
        )?;
        Ok(deleted > 0)
    }

    pub fn is_upload_session_expired(&self, session: &UploadSession) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        session.expires_at.map(|exp| now > exp).unwrap_or(false)
    }

    pub fn upsert_broken_remote_entry(
        &self,
        root_id: Uuid,
        relative_path: &Path,
        remote_file_id: Uuid,
        error: &str,
        observed_at: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO broken_remote_entries
             (root_id, relative_path, remote_file_id, error, first_seen_at, last_seen_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(root_id, relative_path, remote_file_id) DO UPDATE SET
             error = excluded.error,
             last_seen_at = excluded.last_seen_at",
            params![
                root_id.as_bytes(),
                relative_path.to_str().unwrap(),
                remote_file_id.as_bytes(),
                error,
                observed_at,
                observed_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_broken_remote_entries(&self, root_id: Uuid) -> Result<Vec<BrokenRemoteEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT relative_path, remote_file_id, error, first_seen_at, last_seen_at
             FROM broken_remote_entries
             WHERE root_id = ?
             ORDER BY last_seen_at DESC, relative_path ASC",
        )?;

        let rows = stmt.query_map(params![root_id.as_bytes()], |row| {
            let remote_file_id: Vec<u8> = row.get(1)?;
            Ok(BrokenRemoteEntry {
                root_id,
                relative_path: PathBuf::from(row.get::<_, String>(0)?),
                remote_file_id: Uuid::from_slice(&remote_file_id).unwrap_or_else(|_| Uuid::nil()),
                error: row.get(2)?,
                first_seen_at: row.get(3)?,
                last_seen_at: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn clear_broken_remote_entries_for_path(
        &self,
        root_id: Uuid,
        relative_path: &Path,
    ) -> Result<bool> {
        let deleted = self.conn.execute(
            "DELETE FROM broken_remote_entries WHERE root_id = ? AND relative_path = ?",
            params![root_id.as_bytes(), relative_path.to_str().unwrap()],
        )?;
        Ok(deleted > 0)
    }

    pub fn clear_broken_remote_entries(&self, root_id: Uuid) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM broken_remote_entries WHERE root_id = ?",
            params![root_id.as_bytes()],
        )?;
        Ok(deleted)
    }

    pub fn clear_all_broken_remote_entries(&self) -> Result<usize> {
        let deleted = self
            .conn
            .execute("DELETE FROM broken_remote_entries", [])?;
        Ok(deleted)
    }

    pub fn mark_file_tombstone(
        &self,
        root_id: Uuid,
        relative_path: &Path,
        source_side: &str,
        deleted_at: i64,
    ) -> Result<()> {
        let existing = self.get_file_state(root_id, relative_path)?;
        let state = FileState {
            id: existing.as_ref().and_then(|state| state.id),
            root_id,
            relative_path: relative_path.to_path_buf(),
            local_hash: existing.as_ref().and_then(|state| state.local_hash.clone()),
            remote_hash: existing.as_ref().and_then(|state| state.remote_hash.clone()),
            remote_file_id: existing.as_ref().and_then(|state| state.remote_file_id),
            local_modified_at: existing.as_ref().and_then(|state| state.local_modified_at),
            remote_modified_at: existing.as_ref().and_then(|state| state.remote_modified_at),
            size: existing.as_ref().and_then(|state| state.size),
            is_directory: existing.as_ref().and_then(|state| state.is_directory).or(Some(false)),
            sync_status: Some("tombstone".to_string()),
            tombstone_side: Some(source_side.to_string()),
            tombstone_at: Some(deleted_at),
            last_sync_at: Some(deleted_at),
        };
        self.upsert_file_state(&state)?;
        Ok(())
    }
}

/// Represents the sync state of a file
#[derive(Debug, Clone)]
pub struct FileState {
    pub id: Option<i64>,
    pub root_id: Uuid,
    pub relative_path: PathBuf,
    pub local_hash: Option<String>,
    pub remote_hash: Option<String>,
    pub remote_file_id: Option<Uuid>,
    pub local_modified_at: Option<i64>,
    pub remote_modified_at: Option<i64>,
    pub size: Option<i64>,
    pub is_directory: Option<bool>,
    pub sync_status: Option<String>,
    pub tombstone_side: Option<String>,
    pub tombstone_at: Option<i64>,
    pub last_sync_at: Option<i64>,
}

/// Represents an upload session for resumable uploads
#[derive(Debug, Clone)]
pub struct UploadSession {
    pub id: Option<i64>,
    pub file_state_id: i64,
    pub session_id: String,
    pub total_chunks: i32,
    pub uploaded_chunks: i32,
    pub chunk_size: i32,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct BrokenRemoteEntry {
    pub root_id: Uuid,
    pub relative_path: PathBuf,
    pub remote_file_id: Uuid,
    pub error: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_remote_entries_can_be_upserted_and_cleared() {
        let tempdir = tempfile::tempdir().unwrap();
        let db = Database::open(&tempdir.path().join("state.db")).unwrap();
        let root_id = Uuid::new_v4();
        let remote_file_id = Uuid::new_v4();
        let path = Path::new("Notes/missing.md");

        db.upsert_broken_remote_entry(root_id, path, remote_file_id, "404 Not Found", 100)
            .unwrap();
        db.upsert_broken_remote_entry(root_id, path, remote_file_id, "still 404", 200)
            .unwrap();

        let entries = db.get_broken_remote_entries(root_id).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].relative_path, path);
        assert_eq!(entries[0].remote_file_id, remote_file_id);
        assert_eq!(entries[0].error, "still 404");
        assert_eq!(entries[0].first_seen_at, 100);
        assert_eq!(entries[0].last_seen_at, 200);

        assert!(db
            .clear_broken_remote_entries_for_path(root_id, path)
            .unwrap());
        assert!(db.get_broken_remote_entries(root_id).unwrap().is_empty());
    }

    #[test]
    fn broken_remote_entries_can_be_cleared_per_root_and_globally() {
        let tempdir = tempfile::tempdir().unwrap();
        let db = Database::open(&tempdir.path().join("state.db")).unwrap();
        let root_a = Uuid::new_v4();
        let root_b = Uuid::new_v4();

        db.upsert_broken_remote_entry(
            root_a,
            Path::new("Notes/missing-a.md"),
            Uuid::new_v4(),
            "404 Not Found",
            100,
        )
        .unwrap();
        db.upsert_broken_remote_entry(
            root_a,
            Path::new("Notes/missing-b.md"),
            Uuid::new_v4(),
            "404 Not Found",
            100,
        )
        .unwrap();
        db.upsert_broken_remote_entry(
            root_b,
            Path::new("Other/missing-c.md"),
            Uuid::new_v4(),
            "404 Not Found",
            100,
        )
        .unwrap();

        assert_eq!(db.clear_broken_remote_entries(root_a).unwrap(), 2);
        assert!(db.get_broken_remote_entries(root_a).unwrap().is_empty());
        assert_eq!(db.get_broken_remote_entries(root_b).unwrap().len(), 1);

        assert_eq!(db.clear_all_broken_remote_entries().unwrap(), 1);
        assert!(db.get_broken_remote_entries(root_b).unwrap().is_empty());
    }

    #[test]
    fn file_state_round_trips_remote_file_id_and_tombstone_fields() {
        let tempdir = tempfile::tempdir().unwrap();
        let db = Database::open(&tempdir.path().join("state.db")).unwrap();
        let root_id = Uuid::new_v4();
        let remote_file_id = Uuid::new_v4();

        let state = FileState {
            id: None,
            root_id,
            relative_path: PathBuf::from("docs/spec.md"),
            local_hash: Some("local-hash".to_string()),
            remote_hash: Some("remote-hash".to_string()),
            remote_file_id: Some(remote_file_id),
            local_modified_at: Some(100),
            remote_modified_at: Some(101),
            size: Some(42),
            is_directory: Some(false),
            sync_status: Some("tombstone".to_string()),
            tombstone_side: Some("local".to_string()),
            tombstone_at: Some(200),
            last_sync_at: Some(201),
        };

        db.upsert_file_state(&state).unwrap();

        let loaded = db
            .get_file_state(root_id, Path::new("docs/spec.md"))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.remote_file_id, Some(remote_file_id));
        assert_eq!(loaded.sync_status.as_deref(), Some("tombstone"));
        assert_eq!(loaded.tombstone_side.as_deref(), Some("local"));
        assert_eq!(loaded.tombstone_at, Some(200));
    }
}
