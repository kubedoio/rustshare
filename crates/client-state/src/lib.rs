use rusqlite::{params, Connection, Transaction};
use sync_domain::{LocalEntry, SyncRoot, EntryType, HydrationState};
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use chrono::{DateTime, Utc, TimeZone};

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
        conn.execute("PRAGMA journal_mode = WAL", [])?;

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

        Ok(())
    }

    pub fn transaction(&mut self) -> Result<Transaction> {
        Ok(self.conn.transaction()?)
    }

    pub fn save_sync_root(&self, root: &SyncRoot) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sync_roots (id, remote_path, local_path) VALUES (?, ?, ?)",
            params![root.id.as_bytes(), root.remote_path, root.local_path.to_str().unwrap()],
        )?;
        Ok(())
    }

    pub fn get_sync_roots(&self) -> Result<Vec<SyncRoot>> {
        let mut stmt = self.conn.prepare("SELECT id, remote_path, local_path FROM sync_roots")?;
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
        
        let entry = stmt.query_row(params![path.to_str().unwrap()], |row| {
            let entry_type_str: String = row.get(1)?;
            let entry_type = if entry_type_str == "file" { EntryType::File } else { EntryType::Directory };
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
        }).ok();

        Ok(entry)
    }

    pub fn get_sync_cursor(&self) -> Result<Option<String>> {
        let res: Result<String, _> = self.conn.query_row(
            "SELECT cursor FROM sync_cursor WHERE id = 1",
            [],
            |row| row.get(0),
        );
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
        let mut stmt = self.conn.prepare("SELECT pattern FROM sync_filters WHERE root_id = ? AND filter_type = 'exclude'")?;
        let rows = stmt.query_map(params![root_id.as_bytes()], |row| {
            row.get::<_, String>(0)
        })?;

        let mut patterns = Vec::new();
        for row in rows {
            patterns.push(row?);
        }
        Ok(patterns)
    }
}
