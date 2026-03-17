//! Storage layer for RustShare.
//!
//! Handles persistence to PostgreSQL and RustFS.

pub mod event_store;
pub mod metadata;
pub mod object_store;

pub use event_store::EventStore;
