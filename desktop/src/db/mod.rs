//! Local database for sync journal
//!
//! The sync journal tracks:
//! - Which folders are synced locally
//! - File states (local paths with server file IDs, timestamps, hashes)
//! - Sync queue (pending uploads/downloads)
//! - Cursors (last sync cursor per folder)

pub mod sqlite;

pub use sqlite::{Database, FileState, SyncQueueItem, SyncQueueItemType, SyncCursor};
