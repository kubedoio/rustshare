//! Canonical first-party integration-event type constants (v1alpha1).
//!
//! Event types are namespaced, versioned strings owned by the publishing
//! Application (ADR-0031). These constants are the canonical spellings for
//! first-party events: the Files events, and the Chat observed-event constant
//! declared in the Chat manifest; the Application registry
//! (`rustshare-core::domain::application`) declares the same strings in the
//! respective manifests' `integrationEvents.publishes`.

/// A file (and its first content version) was created.
pub const FILES_FILE_CREATED_V1: &str = "io.elembra.files.file.created.v1";
/// An existing file gained a new content version.
pub const FILES_FILE_UPDATED_V1: &str = "io.elembra.files.file.updated.v1";
/// Durable observation of a signed Buzz chat event by the Elembra Chat Bridge
/// (Buzz → Elembra Memory projection, ADR-0033/ADR-0034). Published by the
/// Chat Application; consumed by Memory.
pub const CHAT_BUZZ_EVENT_OBSERVED_V1: &str = "io.elembra.chat.buzz.event.observed.v1";
/// An internal Calendar Application event was created.
pub const CALENDAR_EVENT_CREATED_V1: &str = "io.elembra.calendar.event.created.v1";
/// An internal Calendar Application event was updated.
pub const CALENDAR_EVENT_UPDATED_V1: &str = "io.elembra.calendar.event.updated.v1";
/// An internal Calendar Application event was soft-deleted.
pub const CALENDAR_EVENT_DELETED_V1: &str = "io.elembra.calendar.event.deleted.v1";
/// One calendar import or provider-sync run finished (counts + source
/// ResourceRef only — never titles/descriptions, per minimum-safe-data).
pub const CALENDAR_EVENT_IMPORTED_V1: &str = "io.elembra.calendar.event.imported.v1";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_event_type;

    #[test]
    fn chat_buzz_event_observed_v1_is_canonical() {
        assert_eq!(
            CHAT_BUZZ_EVENT_OBSERVED_V1,
            "io.elembra.chat.buzz.event.observed.v1"
        );
        assert!(validate_event_type(CHAT_BUZZ_EVENT_OBSERVED_V1).is_ok());
    }

    #[test]
    fn calendar_event_types_are_canonical() {
        for event_type in [
            CALENDAR_EVENT_CREATED_V1,
            CALENDAR_EVENT_UPDATED_V1,
            CALENDAR_EVENT_DELETED_V1,
            CALENDAR_EVENT_IMPORTED_V1,
        ] {
            assert!(validate_event_type(event_type).is_ok(), "{event_type}");
        }
        assert_eq!(
            CALENDAR_EVENT_CREATED_V1,
            "io.elembra.calendar.event.created.v1"
        );
        assert_eq!(
            CALENDAR_EVENT_IMPORTED_V1,
            "io.elembra.calendar.event.imported.v1"
        );
    }
}
