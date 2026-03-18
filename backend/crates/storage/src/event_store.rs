//! Event store implementation.
//!
//! NOTE: Currently uses runtime queries (`sqlx::query()`) instead of compile-time
//! queries (`sqlx::query!()`) because offline mode setup requires a running database.
//! This will be migrated to compile-time queries after Docker Compose is set up in Task 11.

use anyhow::Result;
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

    /// Append a new event to the event store
    pub async fn append(&self, event: &Event) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            INSERT INTO events (event_id, event_type, aggregate_id, aggregate_type, payload, user_id, timestamp, version)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(event.id)
        .bind(serde_json::to_string(&event.event_type)?)
        .bind(event.aggregate_id)
        .bind(serde_json::to_string(&event.aggregate_type)?)
        .bind(&event.payload)
        .bind(event.user_id)
        .bind(event.timestamp)
        .bind(event.version)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get all events for an aggregate
    pub async fn get_events(
        &self,
        aggregate_id: Uuid,
        aggregate_type: AggregateType,
    ) -> Result<Vec<Event>> {
        let aggregate_type_str = serde_json::to_string(&aggregate_type)?;

        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
            r#"
            SELECT event_id, event_type, aggregate_id, aggregate_type, payload, user_id, timestamp, version
            FROM events
            WHERE aggregate_id = $1 AND aggregate_type = $2
            ORDER BY timestamp ASC
            "#,
        )
        .bind(aggregate_id)
        .bind(&aggregate_type_str)
        .fetch_all(&self.pool)
        .await?;

        let events = rows
            .into_iter()
            .map(|row| {
                Ok(Event {
                    id: row.try_get("event_id")?,
                    event_type: serde_json::from_str(&row.try_get::<String, _>("event_type")?)?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_type: serde_json::from_str(&row.try_get::<String, _>("aggregate_type")?)?,
                    payload: row.try_get("payload")?,
                    user_id: row.try_get("user_id")?,
                    timestamp: row.try_get("timestamp")?,
                    version: row.try_get("version")?,
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
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
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
        )
        .bind(user_id)
        .bind(last_seen_event_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let events = rows
            .into_iter()
            .map(|row| {
                Ok(Event {
                    id: row.try_get("event_id")?,
                    event_type: serde_json::from_str(&row.try_get::<String, _>("event_type")?)?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_type: serde_json::from_str(&row.try_get::<String, _>("aggregate_type")?)?,
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
    use rustshare_core::events::{Event, EventType, AggregateType};
    use serde_json::json;

    async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());

        PgPool::connect(&database_url).await.unwrap()
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_append_and_retrieve_event() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);

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
        store.append(&event).await.unwrap();

        // Retrieve events
        let events = store.get_events(file_id, AggregateType::File).await.unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FileUploaded);
        assert_eq!(events[0].aggregate_id, file_id);
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_get_events_since_with_last_id() {
        let pool = setup_test_db().await;
        let store = EventStore::new(pool);

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
            store.append(&event).await.unwrap();

            // Small delay to ensure different timestamps
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Fetch events after the 2nd event (index 1)
        let events = store.get_events_since(user_id, Some(event_ids[1]), 100).await.unwrap();

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
            store.append(&event).await.unwrap();

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
            store.append(&event).await.unwrap();

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
            store.append(&event).await.unwrap();

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
            store.append(&event).await.unwrap();

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
