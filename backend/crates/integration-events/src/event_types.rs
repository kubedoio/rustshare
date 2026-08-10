//! Canonical first-party integration-event type constants (v1alpha1).
//!
//! Event types are namespaced, versioned strings owned by the publishing
//! Application (ADR-0031). These constants are the canonical spellings for
//! first-party Files events; the Application registry
//! (`rustshare-core::domain::application`) declares the same strings in the
//! Files manifest's `integrationEvents.publishes`.

/// A file (and its first content version) was created.
pub const FILES_FILE_CREATED_V1: &str = "io.elembra.files.file.created.v1";
/// An existing file gained a new content version.
pub const FILES_FILE_UPDATED_V1: &str = "io.elembra.files.file.updated.v1";
