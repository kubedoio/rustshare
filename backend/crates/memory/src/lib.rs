//! Elembra Memory: the durable catalog projection of observed Buzz chat events.
//!
//! Buzz remains authoritative for messages, channels and membership. This crate
//! owns the *reference* model Memory projects from signed Buzz events that have
//! been observed by the bridge and published as durable Integration Events
//! (`io.elembra.chat.buzz.event.observed.v1`):
//!
//! * [`event`] — the parsed payload of an observed-event integration event,
//!   with fail-closed validation (`[event::ObservedChatEventData::validate]`).
//! * [`record`] — the Memory catalog record: exactly one per Buzz message per
//!   tenant (`memory_catalog`), mirroring the latest signed event.
//! * [`policy`] — per-tenant projection policy read from the Chat
//!   Application's `configuration` (memory_projection / content_indexing).
//! * [`project`] — pure, deterministic projection functions building catalog
//!   records from validated events (`project_record`, `apply_event`,
//!   `apply_tombstone`).
//!
//! Security posture: an observed event with `signature_verified == false` must
//! never be projected; the projection applies out-of-order guards and never
//! un-tombstones an existing record.

pub mod event;
pub mod policy;
pub mod project;
pub mod record;

pub use event::{
    BuzzEventMeta, ChatChannelKind, ChatContext, MemoryValidationError, ObservedChatEventData,
    ObservedEventType, PrincipalMeta,
};
pub use policy::{ProjectionDecision, ProjectionPolicy, SkipReason};
pub use project::{apply_event, apply_tombstone, project_record};
pub use record::{IndexingStatus, MemoryCatalogRecord, ProvenanceEntry};
