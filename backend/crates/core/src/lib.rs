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
    ApplicationManifest, ApplicationRegistry, ApplicationRuntimeKind, CausationId,
    ConnectorAuthority, ConnectorCapabilities, ConnectorDescriptor, ConnectorDescriptorError,
    ConnectorHealth, ConnectorInstance, ConnectorInstanceId, ConnectorInstanceStatus,
    ConnectorMetadata, ConnectorMode, ConnectorModes, ConnectorOperations, ConnectorPublications,
    ConnectorSource, ConnectorStatusError, CorrelationId, File, FileVersion, Folder, PrincipalId,
    Share, TenantId, User, Vault, VaultAdapter, VaultDevice, VaultFile, WorkspaceId,
    CONNECTOR_API_VERSION,
};
pub use events::{AggregateType, Event, EventType};
