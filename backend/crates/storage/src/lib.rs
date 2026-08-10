//! Storage layer for RustShare.
//!
//! Handles persistence to RustFS with in-memory coordination.
//!
//! Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with,
//! endorsed by, or sponsored by Obsidian.

pub mod chat_identity;
pub mod chat_integration_impl;
pub mod event_store;
pub mod metadata;
pub mod object_store;
pub mod outbox_store;
pub mod repos;
pub mod upload_doc_store;
pub mod upload_impl;

pub use chat_identity::ChatIdentityStore;
pub use event_store::EventStore;
pub use metadata::{
    BlobReferenceSummary, MetadataStore, ObjectGcCandidate, PublicShareAccessLogEntry,
    ReplicationAttemptRecord, SecurityConfig, ShareAccessLogEntry, UserSecurityEvent,
    UserSecurityEventRecord,
};
pub use object_store::{is_missing_object_error, ObjectStore, ObjectStoreOptions};
pub use outbox_store::{
    ClaimedEvent, ConsumerCount, DeadLetterEntry, OutboxConfig, OutboxStore, OutboxStoreError,
};

// Service-layer trait bridges for EventStore, MetadataStore, ObjectStore and
// ShareNotificationRepoImpl now live next to their concrete types (see
// event_store.rs, metadata.rs, object_store.rs and repos/share_notification.rs).
// This keeps the storage crate root small while preserving the existing
// core/service generic architecture.
