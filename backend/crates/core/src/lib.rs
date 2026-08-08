//! RustShare core domain models and business logic.
//!
//! This crate contains pure business logic with no I/O dependencies.

pub mod domain;
pub mod events;
pub mod okf;
pub mod services;
pub mod validation;

// Re-export commonly used types
pub use domain::{
    ActionCapability, ApplicationContribution, ApplicationHealth, ApplicationId,
    ApplicationManifest, ApplicationRegistry, ApplicationRuntimeKind, CausationId, CorrelationId,
    File, FileVersion, Folder, PrincipalId, Share, TenantId, User, Vault, VaultAdapter,
    VaultDevice, VaultFile, WorkspaceId,
};
pub use events::{AggregateType, Event, EventType};
