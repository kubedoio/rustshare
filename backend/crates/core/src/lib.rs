//! RustShare core domain models and business logic.
//!
//! This crate contains pure business logic with no I/O dependencies.

pub mod domain;

// Re-export commonly used types
pub use domain::{File, FileVersion, Folder, Share, User};
