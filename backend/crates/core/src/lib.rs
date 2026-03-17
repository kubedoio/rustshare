//! RustShare core domain models and business logic.
//!
//! This crate contains pure business logic with no I/O dependencies.

pub mod domain;
pub mod events;

// Re-export commonly used types
pub use domain::{File, FileVersion, Folder, Share, User};
pub use events::{AggregateType, Event, EventType};
