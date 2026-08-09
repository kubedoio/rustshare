//! Durable integration-event dispatcher (ADR-0031 / issue #212).
//!
//! The dispatcher runs in its own task, strictly after the source mutation
//! transaction has committed: `FileService` publishes into the outbox inside
//! its own transaction and never depends on consumer availability — a slow
//! or absent consumer only makes events accumulate in
//! `integration_outbox`, it never blocks or rolls back a mutation.
//!
//! Delivery model: at-least-once with lease fencing (`claim_token`) and
//! consumer-side idempotency (the reference consumer records a durable
//! receipt atomically with its effect). The dispatcher itself is stateless
//! between ticks; `OutboxStatus` exposes last-tick health to readiness.
//!
//! # Metrics (operator contract)
//!
//! All names are prefixed `outbox_` and labels are bounded (consumer id,
//! event type, delivery state — never resource content):
//!
//! * `outbox_dispatched_total{consumer}` — events claimed and handed to a
//!   consumer in a tick.
//! * `outbox_processed_total{consumer,event_type}` — events acknowledged.
//! * `outbox_retry_total{consumer,event_type}` — events failed retryably
//!   (backoff).
//! * `outbox_dead_lettered_total{consumer,event_type}` — poison events or
//!   exhausted retries.
//! * `outbox_duplicate_receipt_total{consumer}` — duplicate deliveries
//!   skipped by the consumer's idempotency receipt (emitted by the
//!   consumer).
//! * `outbox_process_seconds{consumer,event_type}` — per-event processing
//!   latency histogram.
//! * `outbox_pending_count{consumer,state}` — queue depth gauge, once per
//!   tick.
//! * `outbox_dlq_count` — global dead-letter gauge, once per tick.
//! * `outbox_oldest_pending_age_seconds` — age of the oldest pending
//!   delivery, once per tick.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rustshare_integration_events::redact::redact_error;
use rustshare_integration_events::{event_matches_subscription, ConsumerOutcome, OutboxConsumer};
use rustshare_storage::{ClaimedEvent, OutboxConfig, OutboxStore, OutboxStoreError};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::config::OutboxWorkerConfig;

/// Dispatcher liveness shared with the readiness probe.
#[derive(Debug, Default)]
pub struct OutboxStatus {
    /// When the last tick started (`None` until the first tick).
    pub last_tick_at: Mutex<Option<Instant>>,
    /// Whether the last tick ran to completion (stays `false` if a tick
    /// panics; transient per-event errors are tolerated inside a tick).
    pub last_tick_ok: AtomicBool,
}

/// Asynchronous dispatcher that claims outbox deliveries and drives the
/// registered [`OutboxConsumer`]s through the delivery lifecycle
/// (acknowledge / retry with backoff / dead-letter).
pub struct OutboxDispatcher {
    store: Arc<OutboxStore>,
    consumers: Vec<Arc<dyn OutboxConsumer>>,
    config: OutboxWorkerConfig,
    worker_id: String,
    status: Arc<OutboxStatus>,
}

impl OutboxDispatcher {
    pub fn new(
        store: Arc<OutboxStore>,
        consumers: Vec<Arc<dyn OutboxConsumer>>,
        config: OutboxWorkerConfig,
        worker_id: String,
    ) -> Self {
        Self {
            store,
            consumers,
            config,
            worker_id,
            status: Arc::new(OutboxStatus::default()),
        }
    }

    /// Shared liveness status (readiness probe).
    pub fn status(&self) -> &Arc<OutboxStatus> {
        &self.status
    }

    /// The claim/lease/backoff configuration handed to the outbox store.
    pub fn store_config(&self) -> OutboxConfig {
        OutboxConfig {
            claim_batch_size: self.config.claim_batch_size,
            lease_secs: self.config.lease_secs,
            max_attempts: self.config.max_attempts,
            backoff_initial_ms: self.config.backoff_initial_ms,
            backoff_max_ms: self.config.backoff_max_ms,
            retention_hours: self.config.retention_hours,
        }
    }

    /// Run one full dispatch pass: maintenance, one claim+process cycle per
    /// consumer, then queue-depth gauges.
    ///
    /// The tick tolerates transient DB errors (logged, never panicking);
    /// `last_tick_ok` is set to `true` only when the whole pass completes.
    pub async fn tick(&self) {
        *self
            .status
            .last_tick_at
            .lock()
            .expect("outbox status mutex poisoned") = Some(Instant::now());
        self.status.last_tick_ok.store(false, Ordering::Relaxed);

        match self.store.maintenance(self.config.retention_hours).await {
            Ok(deleted) => {
                debug!(deleted, "outbox maintenance compacted delivered rows");
            }
            Err(error) => {
                warn!(error = %error, "outbox maintenance failed");
            }
        }

        for consumer in &self.consumers {
            self.dispatch_consumer(consumer).await;
        }

        self.record_queue_depth().await;

        self.status.last_tick_ok.store(true, Ordering::Relaxed);
    }

    /// Spawn the poll loop. Returns immediately; the loop runs until
    /// `shutdown` is signalled (broadcast channel, same convention as the
    /// other server workers).
    pub fn spawn(
        self: Arc<Self>,
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let poll_interval = self.config.poll_interval;
            info!(
                consumer_count = self.consumers.len(),
                poll_interval_ms = poll_interval.as_millis(),
                claim_batch_size = self.config.claim_batch_size,
                "Outbox dispatcher worker started"
            );
            let mut ticker = tokio::time::interval(poll_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = shutdown.recv() => {
                        info!("Outbox dispatcher worker shutting down");
                        break;
                    }
                    _ = ticker.tick() => {
                        self.tick().await;
                    }
                }
            }
        })
    }

    /// One claim + process cycle for one consumer.
    async fn dispatch_consumer(&self, consumer: &Arc<dyn OutboxConsumer>) {
        let consumer_id = consumer.consumer_id().to_string();
        let subscriptions = match expand_subscriptions(&self.store, &consumer.subscriptions()).await
        {
            Ok(subscriptions) => subscriptions,
            Err(error) => {
                warn!(
                    %consumer_id,
                    error = %error,
                    "outbox subscription expansion failed; skipping consumer"
                );
                return;
            }
        };
        let claimed = match self
            .store
            .claim_batch(
                &consumer_id,
                &subscriptions,
                &self.store_config(),
                &self.worker_id,
            )
            .await
        {
            Ok(claimed) => claimed,
            Err(error) => {
                warn!(
                    %consumer_id,
                    error = %error,
                    "outbox claim failed; skipping consumer"
                );
                return;
            }
        };
        metrics::counter!("outbox_dispatched_total", "consumer" => consumer_id.clone())
            .increment(claimed.len() as u64);
        for claimed_event in claimed {
            self.process_claimed(consumer, &consumer_id, &claimed_event)
                .await;
        }
    }

    /// Run one claimed event through the consumer and persist the outcome.
    async fn process_claimed(
        &self,
        consumer: &Arc<dyn OutboxConsumer>,
        consumer_id: &str,
        claimed: &ClaimedEvent,
    ) {
        let started = Instant::now();
        let outcome = consumer.process(&claimed.event).await;
        metrics::histogram!(
            "outbox_process_seconds",
            "consumer" => consumer_id.to_string(),
            "event_type" => claimed.event.r#type.clone()
        )
        .record(started.elapsed().as_secs_f64());
        let event_type = claimed.event.r#type.clone();
        match outcome {
            ConsumerOutcome::Processed => {
                match self
                    .store
                    .acknowledge(
                        consumer_id,
                        &claimed.source,
                        claimed.event_id,
                        claimed.claim_token,
                    )
                    .await
                {
                    Ok(true) => {
                        metrics::counter!(
                            "outbox_processed_total",
                            "consumer" => consumer_id.to_string(),
                            "event_type" => event_type
                        )
                        .increment(1);
                    }
                    Ok(false) => {
                        warn!(
                            %consumer_id,
                            source = %claimed.source,
                            event_id = %claimed.event_id,
                            "lost lease, another worker reclaimed the delivery"
                        );
                    }
                    Err(error) => {
                        warn!(
                            %consumer_id,
                            source = %claimed.source,
                            event_id = %claimed.event_id,
                            error = %error,
                            "outbox acknowledge failed"
                        );
                    }
                }
            }
            ConsumerOutcome::Retryable { reason } => {
                let reason = redact_error(&reason, 512);
                match self
                    .store
                    .fail_retryable(
                        consumer_id,
                        &claimed.source,
                        claimed.event_id,
                        claimed.claim_token,
                        &reason,
                        &self.store_config(),
                    )
                    .await
                {
                    Ok(true) => {
                        metrics::counter!(
                            "outbox_retry_total",
                            "consumer" => consumer_id.to_string(),
                            "event_type" => event_type
                        )
                        .increment(1);
                    }
                    Ok(false) => {
                        warn!(
                            %consumer_id,
                            source = %claimed.source,
                            event_id = %claimed.event_id,
                            "lost lease, another worker reclaimed the delivery"
                        );
                    }
                    Err(error) => {
                        warn!(
                            %consumer_id,
                            source = %claimed.source,
                            event_id = %claimed.event_id,
                            error = %error,
                            "outbox retry record failed"
                        );
                    }
                }
            }
            ConsumerOutcome::Permanent { reason } => {
                let reason = redact_error(&reason, 512);
                match self
                    .store
                    .dead_letter(
                        consumer_id,
                        &claimed.source,
                        claimed.event_id,
                        claimed.claim_token,
                        &reason,
                    )
                    .await
                {
                    Ok(true) => {
                        metrics::counter!(
                            "outbox_dead_lettered_total",
                            "consumer" => consumer_id.to_string(),
                            "event_type" => event_type
                        )
                        .increment(1);
                    }
                    Ok(false) => {
                        warn!(
                            %consumer_id,
                            source = %claimed.source,
                            event_id = %claimed.event_id,
                            "lost lease, another worker reclaimed the delivery"
                        );
                    }
                    Err(error) => {
                        warn!(
                            %consumer_id,
                            source = %claimed.source,
                            event_id = %claimed.event_id,
                            error = %error,
                            "outbox dead-letter failed"
                        );
                    }
                }
            }
        }
    }

    /// Record queue-depth gauges once per tick. Failures are logged and
    /// ignored — a gauge must never fail a tick.
    async fn record_queue_depth(&self) {
        match self.store.pending_counts().await {
            Ok(counts) => {
                for count in counts {
                    metrics::gauge!(
                        "outbox_pending_count",
                        "consumer" => count.consumer_id,
                        "state" => count.state
                    )
                    .set(count.count as f64);
                }
            }
            Err(error) => {
                warn!(error = %error, "outbox pending-count gauge failed");
            }
        }
        match self.store.dlq_count().await {
            Ok(count) => {
                metrics::gauge!("outbox_dlq_count").set(count as f64);
            }
            Err(error) => {
                warn!(error = %error, "outbox dlq gauge failed");
            }
        }
        match self.store.oldest_pending_age_seconds(None).await {
            Ok(Some(age)) => {
                metrics::gauge!("outbox_oldest_pending_age_seconds").set(age);
            }
            Ok(None) => {}
            Err(error) => {
                warn!(error = %error, "outbox oldest-pending gauge failed");
            }
        }
    }
}

/// Expand a consumer's subscription list into a concrete event-type list for
/// `claim_batch`.
///
/// Exact entries pass through unchanged; entries ending in `.*` are expanded
/// against the distinct event types currently present in the outbox (so
/// prefix expansion is bounded by the outbox's own distinct-type set). An
/// empty list stays empty, which the claim layer interprets as "subscribe to
/// everything".
pub async fn expand_subscriptions(
    store: &OutboxStore,
    subscriptions: &[String],
) -> Result<Vec<String>, OutboxStoreError> {
    if !subscriptions.iter().any(|s| s.ends_with(".*")) {
        return Ok(subscriptions.to_vec());
    }
    let distinct =
        sqlx::query_scalar::<_, String>("SELECT DISTINCT event_type FROM integration_outbox")
            .fetch_all(store.pool())
            .await?;
    Ok(expand_subscription_patterns(subscriptions, &distinct))
}

/// Pure expansion of subscription patterns against a known event-type set
/// (split out for DB-free unit testing).
fn expand_subscription_patterns(subscriptions: &[String], distinct: &[String]) -> Vec<String> {
    let mut expanded: Vec<String> = Vec::new();
    for subscription in subscriptions {
        if subscription.ends_with(".*") {
            for event_type in distinct {
                if event_matches_subscription(event_type, std::slice::from_ref(subscription))
                    && !expanded.contains(event_type)
                {
                    expanded.push(event_type.clone());
                }
            }
        } else if !expanded.contains(subscription) {
            expanded.push(subscription.clone());
        }
    }
    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subs(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn expand_exact_subscriptions_passes_through() {
        let distinct = subs(&["io.elembra.files.file.created.v1"]);
        assert_eq!(
            expand_subscription_patterns(&subs(&["io.elembra.files.file.created.v1"]), &distinct),
            subs(&["io.elembra.files.file.created.v1"])
        );
    }

    #[test]
    fn expand_prefix_subscription_against_distinct_types() {
        let distinct = subs(&[
            "io.elembra.files.file.created.v1",
            "io.elembra.files.file.updated.v1",
            "io.elembra.mail.message.archived.v1",
        ]);
        let expanded = expand_subscription_patterns(&subs(&["io.elembra.files.*"]), &distinct);
        assert_eq!(
            expanded,
            subs(&[
                "io.elembra.files.file.created.v1",
                "io.elembra.files.file.updated.v1",
            ])
        );
    }

    #[test]
    fn expand_empty_subscriptions_stays_empty_meaning_all() {
        assert!(expand_subscription_patterns(&[], &[]).is_empty());
        assert!(
            expand_subscription_patterns(&[], &subs(&["io.elembra.files.file.created.v1"]))
                .is_empty()
        );
    }

    #[test]
    fn expand_mixed_list_dedupes_and_preserves_order() {
        let distinct = subs(&[
            "io.elembra.files.file.created.v1",
            "io.elembra.files.file.updated.v1",
        ]);
        let expanded = expand_subscription_patterns(
            &subs(&[
                "io.elembra.files.file.created.v1",
                "io.elembra.files.*",
                "io.elembra.files.file.created.v1",
            ]),
            &distinct,
        );
        assert_eq!(
            expanded,
            subs(&[
                "io.elembra.files.file.created.v1",
                "io.elembra.files.file.updated.v1",
            ])
        );
    }
}
