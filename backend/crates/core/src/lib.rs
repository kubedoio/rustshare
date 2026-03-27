//! RustShare core domain models and business logic.
//!
//! This crate contains pure business logic with no I/O dependencies.

pub mod domain;
pub mod events;
pub mod services;
pub mod service_traits;

// Re-export commonly used types
pub use domain::{File, FileVersion, Folder, Share, User};
pub use events::{AggregateType, Event, EventType};
pub use service_traits::{FileServiceTrait, FolderServiceTrait, ShareServiceTrait};
