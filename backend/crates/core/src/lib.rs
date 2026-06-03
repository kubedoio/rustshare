//! RustShare core domain models and business logic.
//!
//! This crate contains pure business logic with no I/O dependencies.

pub mod domain;
pub mod events;
pub mod services;
pub mod validation;

// Re-export commonly used types
pub use domain::{
    File, FileVersion, Folder, Share, User, Vault, VaultAdapter, VaultDevice, VaultFile,
};
pub use events::{AggregateType, Event, EventType};
