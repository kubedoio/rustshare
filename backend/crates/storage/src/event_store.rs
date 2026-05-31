//! Event store implementation.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rustshare_core::events::EventBroadcaster;
use rustshare_core::events::*;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Event store for append-only event log
pub struct EventStore {
    pool: PgPool,
}

impl EventStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Access the underlying database pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Append a new event to the event store
    pub async fn append(&self, event: &Event, broadcaster: &EventBroadcaster) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO events (event_id, event_type, aggregate_id, aggregate_type, payload, user_id, timestamp, version)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            event.id,
            serde_json::to_string(&event.event_type)?,
            event.aggregate_id,
            serde_json::to_string(&event.aggregate_type)?,
            &event.payload,
            event.user_id,
            event.timestamp,
            event.version
        )
        .execute(&self.pool)
        .await?;

        broadcaster.publish(event.clone());

        Ok(())
    }

    /// Get all events for an aggregate
    pub async fn get_events(
        &self,
        aggregate_id: Uuid,
        aggregate_type: AggregateType,
    ) -> Result<Vec<Event>> {
        let aggregate_type_str = serde_json::to_string(&aggregate_type)?;

        let rows = sqlx::query!(
            r#"
            SELECT event_id, event_type, aggregate_id, aggregate_type, payload, user_id, timestamp, version
            FROM events
            WHERE aggregate_id = $1 AND aggregate_type = $2
            ORDER BY timestamp ASC
            "#,
            aggregate_id,
            &aggregate_type_str
        )
        .fetch_all(&self.pool)
        .await?;

        let events = rows
            .into_iter()
            .map(|row| {
                Ok(Event {
                    id: row.event_id,
                    event_type: serde_json::from_str(&row.event_type)?,
                    aggregate_id: row.aggregate_id,
                    aggregate_type: serde_json::from_str(&row.aggregate_type)?,
                    payload: row.payload,
                    user_id: row.user_id,
                    timestamp: row.timestamp,
                    version: row.version,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(events)
    }

    /// Get events since a specific event ID for catch-up synchronization
    ///
    /// Fetches events for a specific user after the given event_id.
    /// Uses (timestamp, id) tuple comparison for deterministic ordering.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The user whose events to fetch
    /// * `last_seen_event_id` - The last event ID the client has seen (None to fetch from beginning)
    /// * `limit` - Maximum number of events to return
    pub async fn get_events_since(
        &self,
        user_id: Uuid,
        last_seen_event_id: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<Event>> {
        let rows = sqlx::query!(
            r#"
            SELECT event_id, event_type, aggregate_id, aggregate_type, payload, user_id, timestamp, version
            FROM events
            WHERE user_id = $1
              AND ($2::uuid IS NULL OR (timestamp, event_id) > (
                SELECT timestamp, event_id FROM events WHERE event_id = $2
              ))
            ORDER BY timestamp ASC, event_id ASC
            LIMIT $3
            "#,
            user_id,
            last_seen_event_id,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        let events = rows
            .into_iter()
            .map(|row| {
                Ok(Event {
                    id: row.event_id,
                    event_type: serde_json::from_str(&row.event_type)?,
                    aggregate_id: row.aggregate_id,
                    aggregate_type: serde_json::from_str(&row.aggregate_type)?,
                    payload: row.payload,
                    user_id: row.user_id,
                    timestamp: row.timestamp,
                    version: row.version,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(events)
    }

    /// Query recent file/module/share mutation events for a tenant.
    ///
    /// Returns events ordered by timestamp descending (newest first).
    /// Supports cursor pagination via `before_timestamp` and `before_id`.
    pub async fn query_recent_events(
        &self,
        tenant_id: Uuid,
        before_timestamp: Option<DateTime<Utc>>,
        before_id: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<Event>> {
        let event_types: Vec<String> = vec![
            EventType::FileUploaded,
            EventType::FileModified,
            EventType::FileRenamed,
            EventType::FileMoved,
            EventType::FileDeleted,
            EventType::FileRestored,
            EventType::FolderCreated,
            EventType::FolderRenamed,
            EventType::FolderMoved,
            EventType::FolderDeleted,
            EventType::ShareCreated,
            EventType::ShareRevoked,
            EventType::ShareUpdated,
            EventType::ShareReceivedByUser,
            EventType::SharePermissionChanged,
            EventType::ShareRevokedFromUser,
            EventType::BrainstormBoardModified,
            EventType::MeetingNoteModified,
            EventType::DecisionModified,
            EventType::StandupModified,
            EventType::KanbanModified,
            EventType::NoteModified,
        ]
        .into_iter()
        .map(|et| serde_json::to_string(&et).unwrap())
        .collect();

        let rows = sqlx::query(
            r#"
            SELECT e.event_id, e.event_type, e.aggregate_id, e.aggregate_type, e.payload, e.user_id, e.timestamp, e.version
            FROM events e
            JOIN users u ON u.id = e.user_id
            WHERE u.tenant_id = $1
              AND e.event_type = ANY($2)
              AND ($3::timestamptz IS NULL OR (e.timestamp, e.event_id) < ($3, $4))
            ORDER BY e.timestamp DESC, e.event_id DESC
            LIMIT $5
            "#,
        )
        .bind(tenant_id)
        .bind(&event_types)
        .bind(before_timestamp)
        .bind(before_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let events = rows
            .into_iter()
            .map(|row| {
                Ok(Event {
                    id: row.try_get("event_id")?,
                    event_type: serde_json::from_str(
                        row.try_get::<String, _>("event_type")?.as_str(),
                    )?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_type: serde_json::from_str(
                        row.try_get::<String, _>("aggregate_type")?.as_str(),
                    )?,
                    payload: row.try_get("payload")?,
                    user_id: row.try_get("user_id")?,
                    timestamp: row.try_get("timestamp")?,
                    version: row.try_get("version")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::events::{AggregateType, Event, EventType};
    use serde_json::json;

    const TEST_DATABASE_URL: &str = "postgres://rustshare:changeme@localhost:5432/rustshare";

    async fn setup_test_db() -> PgPool {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        PgPool::connect(&database_url).await.unwrap()
    }

    #[test]
    fn test_query_recent_events_event_type_serialization_matches_append() {
        // query_recent_events builds its event type filter by serializing
        // EventType variants with serde_json::to_string. append() stores
        // event_type the same way. This test ensures the formats match.
        let event_types = vec![
            EventType::FileUploaded,
            EventType::FileModified,
            EventType::FileRenamed,
            EventType::FileMoved,
            EventType::FileDeleted,
            EventType::FileRestored,
            EventType::FolderCreated,
            EventType::FolderRenamed,
            EventType::FolderMoved,
            EventType::FolderDeleted,
            EventType::ShareCreated,
            EventType::ShareRevoked,
            EventType::ShareUpdated,
            EventType::ShareReceivedByUser,
            EventType::SharePermissionChanged,
            EventType::ShareRevokedFromUser,
            EventType::BrainstormBoardModified,
            EventType::MeetingNoteModified,
            EventType::DecisionModified,
            EventType::StandupModified,
            EventType::KanbanModified,
            EventType::NoteModified,
        ];

        for et in event_types {
            let serialized = serde_json::to_string(&et).unwrap();
            // Verify it's a JSON object with a "type" key, not a plain string.
            assert!(
                serialized.starts_with("{\"type\":\""),
                "EventType {:?} serializes as {} which is not the expected JSON object format",
                et,
                serialized
            );
            // Verify round-trip deserialization works.
            let deserialized: EventType = serde_json::from_str(&serialized).unwrap();
            assert_eq!(et, deserialized);
        }
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_append_and_retrieve_event() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);
        let broadcaster = EventBroadcaster::new(100);

        let user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let payload = json!({
            "file_id": file_id.to_string(),
            "name": "test.txt",
            "size": 1024
        });

        let event = Event::new(
            EventType::FileUploaded,
            file_id,
            AggregateType::File,
            payload,
            user_id,
        );

        // Append event
        store.append(&event, &broadcaster).await.unwrap();

        // Retrieve events
        let events = store
            .get_events(file_id, AggregateType::File)
            .await
            .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FileUploaded);
        assert_eq!(events[0].aggregate_id, file_id);
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_get_events_since_with_last_id() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);
        let broadcaster = EventBroadcaster::new(100);

        let user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        // Create 5 events
        let mut event_ids = Vec::new();
        for i in 0..5 {
            let payload = json!({
                "file_id": file_id.to_string(),
                "name": format!("test{}.txt", i),
                "index": i
            });

            let event = Event::new(
                EventType::FileUploaded,
                file_id,
                AggregateType::File,
                payload,
                user_id,
            );
            event_ids.push(event.id);
            store.append(&event, &broadcaster).await.unwrap();

            // Small delay to ensure different timestamps
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Fetch events after the 2nd event (index 1)
        let events = store
            .get_events_since(user_id, Some(event_ids[1]), 100)
            .await
            .unwrap();

        // Should get 3 events (indices 2, 3, 4)
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].id, event_ids[2]);
        assert_eq!(events[1].id, event_ids[3]);
        assert_eq!(events[2].id, event_ids[4]);
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_get_events_since_respects_limit() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);
        let broadcaster = EventBroadcaster::new(100);

        let user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        // Create 10 events
        let mut event_ids = Vec::new();
        for i in 0..10 {
            let payload = json!({
                "file_id": file_id.to_string(),
                "name": format!("test{}.txt", i),
                "index": i
            });

            let event = Event::new(
                EventType::FileUploaded,
                file_id,
                AggregateType::File,
                payload,
                user_id,
            );
            event_ids.push(event.id);
            store.append(&event, &broadcaster).await.unwrap();

            // Small delay to ensure different timestamps
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Fetch events with limit of 3
        let events = store.get_events_since(user_id, None, 3).await.unwrap();

        // Should get exactly 3 events
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].id, event_ids[0]);
        assert_eq!(events[1].id, event_ids[1]);
        assert_eq!(events[2].id, event_ids[2]);
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_get_events_since_filters_by_user() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);
        let broadcaster = EventBroadcaster::new(100);

        let user1_id = Uuid::new_v4();
        let user2_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        // Create events for user 1
        let mut user1_event_ids = Vec::new();
        for i in 0..3 {
            let payload = json!({
                "file_id": file_id.to_string(),
                "name": format!("user1_test{}.txt", i),
                "index": i
            });

            let event = Event::new(
                EventType::FileUploaded,
                file_id,
                AggregateType::File,
                payload,
                user1_id,
            );
            user1_event_ids.push(event.id);
            store.append(&event, &broadcaster).await.unwrap();

            // Small delay to ensure different timestamps
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Create events for user 2
        let mut user2_event_ids = Vec::new();
        for i in 0..3 {
            let payload = json!({
                "file_id": file_id.to_string(),
                "name": format!("user2_test{}.txt", i),
                "index": i
            });

            let event = Event::new(
                EventType::FileUploaded,
                file_id,
                AggregateType::File,
                payload,
                user2_id,
            );
            user2_event_ids.push(event.id);
            store.append(&event, &broadcaster).await.unwrap();

            // Small delay to ensure different timestamps
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Fetch events for user 1
        let user1_events = store.get_events_since(user1_id, None, 100).await.unwrap();

        // Should only get user 1's events
        assert_eq!(user1_events.len(), 3);
        assert_eq!(user1_events[0].id, user1_event_ids[0]);
        assert_eq!(user1_events[1].id, user1_event_ids[1]);
        assert_eq!(user1_events[2].id, user1_event_ids[2]);

        // Fetch events for user 2
        let user2_events = store.get_events_since(user2_id, None, 100).await.unwrap();

        // Should only get user 2's events
        assert_eq!(user2_events.len(), 3);
        assert_eq!(user2_events[0].id, user2_event_ids[0]);
        assert_eq!(user2_events[1].id, user2_event_ids[1]);
        assert_eq!(user2_events[2].id, user2_event_ids[2]);
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_get_events_since_with_null_id() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);
        let broadcaster = EventBroadcaster::new(100);

        let user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        // Create 5 events
        let mut event_ids = Vec::new();
        for i in 0..5 {
            let payload = json!({
                "file_id": file_id.to_string(),
                "name": format!("test{}.txt", i),
                "index": i
            });

            let event = Event::new(
                EventType::FileUploaded,
                file_id,
                AggregateType::File,
                payload,
                user_id,
            );
            event_ids.push(event.id);
            store.append(&event, &broadcaster).await.unwrap();

            // Small delay to ensure different timestamps
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Fetch events from beginning (NULL last_seen_event_id)
        let events = store.get_events_since(user_id, None, 100).await.unwrap();

        // Should get all 5 events
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].id, event_ids[0]);
        assert_eq!(events[1].id, event_ids[1]);
        assert_eq!(events[2].id, event_ids[2]);
        assert_eq!(events[3].id, event_ids[3]);
        assert_eq!(events[4].id, event_ids[4]);
    }
}
