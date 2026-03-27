//! # ⚠️ DEPRECATED - LEGACY CRATE
//!
//! This crate (`rustshare-infrastructure`) is **DEPRECATED** and will be removed in a future version.
//!
//! ## Migration Notice
//!
//! This crate previously contained SQLx-based repository implementations for PostgreSQL.
//! As part of the clean break from PostgreSQL, all SQLx code has been removed.
//!
//! ## Replacement
//!
//! Use the new storage layer instead:
//! - `rustshare_storage` crate - SQLite-based storage implementations
//!
//! ## What Was Removed
//!
//! - `UserRepository` - User database operations (SQLx)
//! - `FileRepository` - File database operations (SQLx)
//! - `FolderRepository` - Folder database operations (SQLx)
//! - `ShareRepository` - Share database operations (SQLx)
//! - `NotificationRepository` - Notification database operations (SQLx)
//!
//! ## Timeline
//!
//! - Now: This crate exports nothing and serves as a placeholder
//! - Future: This crate will be completely removed

// This module is intentionally empty - all repository implementations have been removed
// as part of the PostgreSQL to SQLite migration.
//
// Use rustshare_storage instead for all storage needs.

// Re-export nothing - the crate is deprecated
