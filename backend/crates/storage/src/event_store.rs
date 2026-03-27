//! DEPRECATED: PostgreSQL-based EventStore
//!
//! This module is deprecated as part of the PostgreSQL removal migration.
//! The storage crate no longer supports PostgreSQL as an event store backend.
//!
//! Use `metadata_v2` module instead, which provides:
//! - `EventLogStore` trait for append-only event storage
//! - `RustFsEventStore` for RustFS-backed event storage
//! - Event documents stored in object storage
//!
//! Migration guide:
//! - Replace `EventStore` with `Arc<dyn EventLogStore>` from `metadata_v2`
//! - Events are now stored as documents in object storage

/// DEPRECATED: EventStore has been removed.
///
/// This struct no longer functions. Use `metadata_v2::EventLogStore` implementations instead.
#[deprecated(
    since = "0.2.0",
    note = "PostgreSQL-based EventStore has been removed. Use metadata_v2::EventLogStore implementations instead."
)]
pub struct EventStore {
    _private: (),
}

#[allow(deprecated)]
impl EventStore {
    /// DEPRECATED: Creates a stub that will return errors if used.
    #[deprecated(
        since = "0.2.0",
        note = "EventStore::new is no longer available. Use metadata_v2 instead."
    )]
    pub fn new(_pool: ()) -> Self {
        Self { _private: () }
    }

    /// DEPRECATED: Always returns an error.
    ///
    /// Use `metadata_v2::EventLogStore::append()` instead.
    pub async fn append(
        &self,
        _event: &rustshare_core::events::Event,
        _broadcaster: &rustshare_core::events::EventBroadcaster,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!(
            "EventStore has been deprecated. Use metadata_v2::EventLogStore instead."
        ))
    }

    /// DEPRECATED: Always returns an error.
    ///
    /// Use `metadata_v2::EventLogStore::read_for_resource()` instead.
    pub async fn get_events(
        &self,
        _aggregate_id: uuid::Uuid,
        _aggregate_type: rustshare_core::events::AggregateType,
    ) -> anyhow::Result<Vec<rustshare_core::events::Event>> {
        Err(anyhow::anyhow!(
            "EventStore has been deprecated. Use metadata_v2::EventLogStore instead."
        ))
    }

    /// DEPRECATED: Always returns an error.
    ///
    /// Event catch-up synchronization is now handled differently in metadata_v2.
    pub async fn get_events_since(
        &self,
        _user_id: uuid::Uuid,
        _last_seen_event_id: Option<uuid::Uuid>,
        _limit: i64,
    ) -> anyhow::Result<Vec<rustshare_core::events::Event>> {
        Err(anyhow::anyhow!(
            "EventStore has been deprecated. Use metadata_v2::EventLogStore instead."
        ))
    }
}

#[cfg(test)]
mod tests {
    // Tests removed - EventStore is deprecated and no longer functional.
    // Use metadata_v2 tests instead.
}
