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
}
