//! Filesystem operations for the sync client
//!
//! Provides:
//! - File system watching for real-time change detection
//! - Local file scanning for comparison with server state
//! - File indexing for efficient lookups

pub mod indexer;
pub mod scanner;
pub mod watcher;

pub use indexer::{FileIndex, FileIndexEntry};
pub use scanner::FileScanner;
pub use watcher::{FsWatcher, WatchEvent};
