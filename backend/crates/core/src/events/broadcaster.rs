//! Event broadcasting for real-time synchronization.
//!
//! This module provides an in-memory pub/sub mechanism using tokio's broadcast
//! channel to distribute events from the EventStore to all connected WebSocket clients.

use std::sync::Arc;
use tokio::sync::broadcast;

use super::Event;

/// Event broadcaster for distributing events to multiple subscribers.
///
/// Uses tokio's broadcast channel to implement a pub/sub pattern where
/// each subscriber receives an independent copy of all published events.
/// Events are wrapped in Arc to avoid cloning for each subscriber.
#[derive(Clone)]
pub struct EventBroadcaster {
    tx: broadcast::Sender<Arc<Event>>,
}

impl EventBroadcaster {
    /// Create a new event broadcaster with the specified channel capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - The number of events that can be buffered in the channel
    ///   before subscribers start lagging
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Publish an event to all subscribers.
    ///
    /// The event is wrapped in Arc and sent to all active subscribers.
    /// If there are no subscribers, the event is silently dropped (no panic).
    ///
    /// # Arguments
    ///
    /// * `event` - The event to publish
    pub fn publish(&self, event: Event) {
        let arc_event = Arc::new(event);
        // Send errors only occur when there are no receivers, which is expected
        // during shutdown or if no components are subscribed. This is not an error condition.
        let _ = self.tx.send(arc_event);
    }

    /// Subscribe to receive events.
    ///
    /// Returns a new receiver that will receive all events published after
    /// this subscription. Each subscriber gets an independent receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Event>> {
        self.tx.subscribe()
    }

    /// Returns true if the broadcaster channel is open and can accept publishes.
    pub fn is_healthy(&self) -> bool {
        // Broadcast senders are always usable; send only fails when there are no
        // receivers, which is a normal operational state, not a health failure.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::types::{AggregateType, EventType};
    use serde_json::json;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_multiple_subscribers_receive_event() {
        // Create broadcaster with capacity for multiple events
        let broadcaster = EventBroadcaster::new(10);

        // Subscribe 3 receivers
        let mut receiver1 = broadcaster.subscribe();
        let mut receiver2 = broadcaster.subscribe();
        let mut receiver3 = broadcaster.subscribe();

        // Create and publish one event
        let user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let event = Event::new(
            EventType::FileUploaded,
            file_id,
            AggregateType::File,
            json!({"file_id": file_id.to_string(), "name": "test.txt"}),
            user_id,
        );
        let event_id = event.id;

        broadcaster.publish(event);

        // Verify all 3 receivers get the same event
        let received1 = receiver1.recv().await.unwrap();
        let received2 = receiver2.recv().await.unwrap();
        let received3 = receiver3.recv().await.unwrap();

        assert_eq!(received1.id, event_id);
        assert_eq!(received2.id, event_id);
        assert_eq!(received3.id, event_id);

        // Verify they all point to the same Arc
        assert!(Arc::ptr_eq(&received1, &received2));
        assert!(Arc::ptr_eq(&received2, &received3));
    }

    #[tokio::test]
    async fn test_lagged_subscriber() {
        // Create broadcaster with small capacity
        let broadcaster = EventBroadcaster::new(2);

        // Subscribe a receiver but don't consume events
        let mut receiver = broadcaster.subscribe();

        // Publish 5 events without consuming (exceeds capacity of 2)
        let user_id = Uuid::new_v4();
        for i in 0..5 {
            let file_id = Uuid::new_v4();
            let event = Event::new(
                EventType::FileUploaded,
                file_id,
                AggregateType::File,
                json!({"file_id": file_id.to_string(), "name": format!("test{}.txt", i)}),
                user_id,
            );
            broadcaster.publish(event);
        }

        // Receiver should get Lagged error
        let result = receiver.recv().await;
        assert!(result.is_err());
        match result {
            Err(broadcast::error::RecvError::Lagged(count)) => {
                // Should have lagged by at least 3 events (5 sent - 2 capacity)
                assert!(count >= 3, "Expected lagged count >= 3, got {}", count);
            }
            _ => panic!("Expected Lagged error, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_no_subscribers() {
        // Create broadcaster
        let broadcaster = EventBroadcaster::new(10);

        // Publish event without any subscribers
        let user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let event = Event::new(
            EventType::FileUploaded,
            file_id,
            AggregateType::File,
            json!({"file_id": file_id.to_string(), "name": "test.txt"}),
            user_id,
        );

        // Should not panic
        broadcaster.publish(event);

        // Verify we can still subscribe and receive future events
        let mut receiver = broadcaster.subscribe();

        let file_id2 = Uuid::new_v4();
        let event2 = Event::new(
            EventType::FileUploaded,
            file_id2,
            AggregateType::File,
            json!({"file_id": file_id2.to_string(), "name": "test2.txt"}),
            user_id,
        );
        let event2_id = event2.id;

        broadcaster.publish(event2);

        let received = receiver.recv().await.unwrap();
        assert_eq!(received.id, event2_id);
    }
}
