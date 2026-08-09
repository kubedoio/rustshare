//! Integration tests: event-cursor correctness.
//!
//! Regression: an unknown `last_seen_event_id` cursor used to make the tuple
//! comparison `(timestamp, event_id) > NULL` filter out every row, so sync
//! clients got an empty page that looked like a completed catch-up. The cursor
//! must now be resolved explicitly: a bad cursor is an error, not a silent
//! "you are up to date".
//!
//! Run with: cargo test --test event_cursor_test -- --ignored
//! (requires DATABASE_URL, as CI provides).

use rustshare_core::events::{AggregateType, Event, EventBroadcaster, EventType};
use rustshare_storage::EventStore;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

async fn test_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    sqlx::PgPool::connect(&url)
        .await
        .expect("DB connect failed")
}

async fn cleanup(pool: &PgPool, user_id: Uuid) {
    sqlx::query("DELETE FROM events WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn unknown_event_cursor_is_an_explicit_error() {
    let pool = test_pool().await;
    let store = EventStore::new(pool.clone());
    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let user_id = Uuid::new_v4();

    let first = Event::new(
        EventType::FileUploaded,
        Uuid::new_v4(),
        AggregateType::File,
        serde_json::json!({"file_id": Uuid::new_v4()}),
        user_id,
    );
    let second = Event::new(
        EventType::FileModified,
        Uuid::new_v4(),
        AggregateType::File,
        serde_json::json!({"file_id": Uuid::new_v4()}),
        user_id,
    );
    store.append(&first, &broadcaster).await.expect("append e1");
    store
        .append(&second, &broadcaster)
        .await
        .expect("append e2");

    // A cursor that does not exist (or belongs to another user) must be an
    // explicit error, not an empty "caught up" page.
    let unknown = store
        .get_events_since(user_id, Some(Uuid::new_v4()), 10)
        .await;
    assert!(
        unknown.is_err(),
        "unknown cursor must be an error, got: {unknown:?}"
    );
    let message = unknown.unwrap_err().to_string();
    assert!(
        message.contains("Unknown event cursor"),
        "unexpected error message: {message}"
    );

    // A known cursor returns only the events after it.
    let after_first = store
        .get_events_since(user_id, Some(first.id), 10)
        .await
        .expect("known cursor must succeed");
    assert_eq!(after_first.len(), 1);
    assert_eq!(after_first[0].id, second.id);

    // No cursor returns the full history.
    let all = store
        .get_events_since(user_id, None, 10)
        .await
        .expect("full history must succeed");
    assert_eq!(all.len(), 2);

    cleanup(&pool, user_id).await;
}
