//! Storage layer for RustShare.
//!
//! Handles persistence to RustFS with in-memory coordination.
//!
//! Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with,
//! endorsed by, or sponsored by Obsidian.

pub mod chat_identity;
pub mod chat_integration_impl;
pub mod chat_observation;
pub mod event_store;
pub mod managed_identity_vault;
pub mod memory_catalog;
pub mod metadata;
pub mod object_store;
pub mod outbox_store;
pub mod repos;
pub mod upload_doc_store;
pub mod upload_impl;
#[cfg(feature = "upstream-buzz")]
pub mod upstream_relay_mapping_store;

pub use chat_identity::{
    ChatIdentityStore, CommunityMappingError, ReplacementAuditEntry, ReplacementLease,
    ReplacementStart, ReplacementStatus, ResolvedChatAuthor,
};
pub use chat_observation::{ChatObservationStore, UpsertOutcome};
pub use event_store::EventStore;
pub use managed_identity_vault::{
    ManagedIdentityRecord, ManagedIdentityStatus, ManagedIdentityStoreError,
    ManagedIdentityVaultStore,
};
pub use memory_catalog::{content_match_terms, MemoryCatalogStore, ReconcileCounts};
pub use metadata::{
    BlobReferenceSummary, MemoryReconcileFile, MetadataStore, ObjectGcCandidate,
    PublicShareAccessLogEntry, ReplicationAttemptRecord, SecurityConfig, ShareAccessLogEntry,
    UserSecurityEvent, UserSecurityEventRecord,
};
pub use object_store::{
    is_missing_object_error, BucketVersioningState, ObjectStore, ObjectStoreOptions,
};
pub use outbox_store::{
    ClaimedEvent, ConsumerCount, DeadLetterEntry, OutboxConfig, OutboxStore, OutboxStoreError,
};
#[cfg(feature = "upstream-buzz")]
pub use upstream_relay_mapping_store::{UpstreamRelayMappingStore, UpstreamRelayMappingStoreError};

// Service-layer trait bridges for EventStore, MetadataStore, ObjectStore and
// ShareNotificationRepoImpl now live next to their concrete types (see
// event_store.rs, metadata.rs, object_store.rs and repos/share_notification.rs).
// This keeps the storage crate root small while preserving the existing
// core/service generic architecture.
