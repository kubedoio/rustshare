//! Durable outbox consumer contract (ADR-0031).

use crate::event::IntegrationEvent;

/// Outcome of processing one integration event, as reported by a consumer
/// to the dispatcher.
///
/// `Retryable` and `Permanent` reasons are diagnostics only; the dispatcher
/// redacts them with [`crate::redact::redact_error`] before persisting them
/// to the delivery record (no secrets in the dead-letter queue).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsumerOutcome {
    /// The event was processed durably and idempotently.
    Processed,
    /// Transient failure; the dispatcher will retry with backoff.
    Retryable { reason: String },
    /// Permanent failure (poison event); the dispatcher will dead-letter it.
    Permanent { reason: String },
}

/// A durable consumer of integration events.
///
/// Implementations must:
/// * return a stable [`Self::consumer_id`] across restarts (it identifies
///   the delivery ledger row);
/// * declare the event types they subscribe to in [`Self::subscriptions`]
///   (exact types or `.*` prefix patterns; see
///   [`event_matches_subscription`]);
/// * be idempotent: [`Self::process`] may be invoked more than once for the
///   same event (at-least-once delivery), so durable effects and the
///   idempotency receipt must be recorded atomically in one consumer-local
///   transaction where possible;
/// * classify failures as [`ConsumerOutcome::Retryable`] (transient) or
///   [`ConsumerOutcome::Permanent`] (poison events that must never be
///   retried).
#[async_trait::async_trait]
pub trait OutboxConsumer: Send + Sync {
    /// Stable consumer identifier, e.g. `io.elembra.test.memory-projection`.
    fn consumer_id(&self) -> &str;

    /// Exact event types OR prefix patterns ending with `.*` this consumer
    /// subscribes to. Must be non-empty: durable registration rejects an
    /// empty list (an empty pattern set cannot be discovered at eager
    /// fan-out, so no durable obligation would ever be created). Broad
    /// consumers declare an explicit prefix such as `io.elembra.*`.
    fn subscriptions(&self) -> Vec<String>;

    /// Process one event.
    ///
    /// Must be idempotent (durable effect + receipt in one consumer-local
    /// transaction where possible). Return [`ConsumerOutcome::Retryable`]
    /// for transient failures and [`ConsumerOutcome::Permanent`] for poison
    /// events.
    async fn process(&self, event: &IntegrationEvent) -> ConsumerOutcome;
}

/// Whether `event_type` matches any of the subscription patterns.
///
/// Matching rules:
/// * an empty `subscriptions` list matches nothing (fail closed — a
///   consumer without explicit patterns must never receive events);
/// * a subscription without a trailing `.*` matches exactly;
/// * a subscription ending in `.*` (e.g. `io.elembra.files.*`) matches any
///   event type under that prefix (`io.elembra.files.file.created.v1`).
pub fn event_matches_subscription(event_type: &str, subscriptions: &[String]) -> bool {
    if subscriptions.is_empty() {
        return false;
    }
    subscriptions.iter().any(|subscription| {
        if let Some(prefix) = subscription.strip_suffix(".*") {
            !prefix.is_empty()
                && event_type.len() > prefix.len()
                && event_type.starts_with(prefix)
                && event_type.as_bytes()[prefix.len()] == b'.'
        } else {
            event_type == subscription
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_subscription_matches_only_that_type() {
        let subs = vec!["io.elembra.files.file.created.v1".to_string()];
        assert!(event_matches_subscription(
            "io.elembra.files.file.created.v1",
            &subs
        ));
        assert!(!event_matches_subscription(
            "io.elembra.files.file.updated.v1",
            &subs
        ));
    }

    #[test]
    fn prefix_subscription_matches_under_the_namespace() {
        let subs = vec!["io.elembra.files.*".to_string()];
        assert!(event_matches_subscription(
            "io.elembra.files.file.created.v1",
            &subs
        ));
        assert!(event_matches_subscription(
            "io.elembra.files.share.revoked.v1",
            &subs
        ));
        assert!(!event_matches_subscription(
            "io.elembra.mail.message.archived.v1",
            &subs
        ));
        // The prefix itself is not a valid event and must not match.
        assert!(!event_matches_subscription("io.elembra.files", &subs));
    }

    #[test]
    fn empty_subscriptions_match_nothing() {
        let subs: Vec<String> = vec![];
        assert!(!event_matches_subscription(
            "io.elembra.files.file.created.v1",
            &subs
        ));
        assert!(!event_matches_subscription("anything.at.all.v9", &subs));
    }

    #[test]
    fn multiple_subscriptions_are_union() {
        let subs = vec![
            "io.elembra.files.file.created.v1".to_string(),
            "io.elembra.mail.*".to_string(),
        ];
        assert!(event_matches_subscription(
            "io.elembra.files.file.created.v1",
            &subs
        ));
        assert!(event_matches_subscription(
            "io.elembra.mail.message.archived.v1",
            &subs
        ));
        assert!(!event_matches_subscription(
            "io.elembra.files.file.updated.v1",
            &subs
        ));
    }

    #[test]
    fn dangling_wildcards_and_empty_prefixes_are_rejected() {
        let subs = vec!["io.elembra.files.".to_string()];
        assert!(!event_matches_subscription(
            "io.elembra.files.file.created.v1",
            &subs
        ));
        let subs = vec!["*".to_string()];
        assert!(!event_matches_subscription(
            "io.elembra.files.file.created.v1",
            &subs
        ));
    }

    struct TestConsumer;

    #[async_trait::async_trait]
    impl OutboxConsumer for TestConsumer {
        fn consumer_id(&self) -> &str {
            "io.elembra.test.consumer"
        }
        fn subscriptions(&self) -> Vec<String> {
            vec!["io.elembra.files.*".to_string()]
        }
        async fn process(&self, _event: &IntegrationEvent) -> ConsumerOutcome {
            ConsumerOutcome::Processed
        }
    }

    #[tokio::test]
    async fn consumer_trait_is_object_safe_and_callable() {
        let consumer: Box<dyn OutboxConsumer> = Box::new(TestConsumer);
        assert_eq!(consumer.consumer_id(), "io.elembra.test.consumer");
        let outcome = consumer.process(&dummy_event()).await;
        assert_eq!(outcome, ConsumerOutcome::Processed);
    }

    fn dummy_event() -> IntegrationEvent {
        use rustshare_core::domain::{TenantId, WorkspaceId};
        let tenant = TenantId(uuid::Uuid::new_v4());
        IntegrationEvent::builder()
            .source("elembra://io.elembra.files")
            .r#type("io.elembra.files.file.created.v1")
            .tenant_id(tenant)
            .workspace_id(WorkspaceId(tenant.0))
            .data(serde_json::json!({}))
            .build()
            .unwrap()
    }
}
