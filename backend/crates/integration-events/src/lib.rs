//! Durable cross-Application Integration Events (ADR-0031).
//!
//! This crate defines the transport-neutral contracts for durable
//! communication between Elembra Applications and Connectors:
//!
//! * [`event::IntegrationEvent`] — a CloudEvents 1.0-compatible envelope
//!   carrying routing/provenance context (tenant, workspace, actor,
//!   correlation, resource reference) plus application-owned `data`.
//! * [`consumer::OutboxConsumer`] — the durable consumer contract with
//!   stable identity, subscription list and idempotent `process` semantics.
//! * [`redact::redact_error`] — redaction of secret-looking substrings in
//!   persisted failure diagnostics (dead-letter reasons).
//! * [`event_types`] — canonical first-party event-type constants.
//!
//! The initial transport is the transactional PostgreSQL outbox
//! (`rustshare-storage::outbox_store`); the envelope itself is
//! transport-neutral so a later broker (NATS/Kafka) can reuse it unchanged.
//!
//! See `docs/adr/0031-durable-integration-events.md` and
//! `docs/specs/integration-event-v1alpha1.md`.

pub mod consumer;
pub mod event;
pub mod event_types;
pub mod redact;

pub use consumer::{event_matches_subscription, ConsumerOutcome, OutboxConsumer};
pub use event::{
    validate_event_type, validate_source_uri, ActorRef, ActorRefError, EventValidationError,
    IntegrationEvent, IntegrationEventBuilder, MAX_DATASCHEMA_LEN, MAX_EVENT_BYTES,
    MAX_EVENT_DATA_BYTES, MAX_EVENT_TYPE_LEN, MAX_EXTENSION_STRING_LEN, MAX_SOURCE_LEN,
    MAX_SUBJECT_LEN,
};
