//! Sync engine for the desktop client
//!
//! Provides:
//! - Main sync engine that coordinates local and remote changes
//! - Cursor management for incremental sync
//! - Delta processing from server
//! - Conflict resolution

pub mod conflict;
pub mod cursor;
pub mod delta;
pub mod engine;

pub use conflict::{ConflictResolver, ConflictResolution, ConflictInfo};
pub use cursor::CursorManager;
pub use delta::DeltaProcessor;
pub use engine::{SyncEngine, SyncStatus, SyncEngineConfig};
