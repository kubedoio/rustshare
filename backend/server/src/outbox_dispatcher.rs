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
//! # Durable consumer registration (v1alpha1)
//!
//! The store's durable registration (`integration_consumers` /
//! `integration_consumer_subscriptions`) is authoritative for claiming. At
//! the start of every tick the dispatcher re-registers each runtime consumer
//! with its current subscription list (`register_consumer`). Subscription
//! contracts are immutable in v1alpha1: re-registration is an idempotent
//! no-op when the normalized subscription set is identical (preserving
//! `enabled` and `registered_at`), and a changed set is rejected with a
//! `ConsumerRegistrationConflict` — the consumer is logged and skipped, and
//! its durable contract stays as-is. A consumer whose subscription set
//! changes must use a new (versioned) consumer identity. An empty
//! subscription list is rejected outright; every consumer must declare at
//! least one explicit pattern. An operator-disabled consumer
//! (`enabled = false`) keeps its pending obligations but is skipped by
//! `claim_batch` (store-side), so no events are lost while it is turned off.
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
use rustshare_integration_events::{ConsumerOutcome, OutboxConsumer};
use rustshare_storage::{ClaimedEvent, OutboxConfig, OutboxStore};
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

        // Registration sync: mirror the runtime consumer set into the store's
        // durable registration. v1alpha1 subscription contracts are
        // immutable, so re-registration is an idempotent no-op when the
        // consumer's subscription set is unchanged (preserving `enabled` /
        // `registered_at`); a consumer whose set changed is rejected with a
        // conflict and claims nothing this tick (its durable contract and
        // past obligations are untouched). The durable registration is
        // authoritative for claiming, so a consumer that fails to register
        // simply claims nothing this tick; a persistent failure only loses
        // it its claim rights (obligations from past publishes are
        // untouched).
        for consumer in &self.consumers {
            let consumer_id = consumer.consumer_id().to_string();
            match self
                .store
                .register_consumer(&consumer_id, &consumer.subscriptions())
                .await
            {
                Ok(()) => {
                    debug!(%consumer_id, "outbox consumer registered");
                }
                Err(error) => {
                    warn!(%consumer_id, error = %error, "outbox consumer registration failed");
                }
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
    ///
    /// The durable store registration (kept in sync at the start of each
    /// tick) is authoritative: `claim_batch` filters by the durable
    /// subscriptions and returns nothing for an unregistered or disabled
    /// consumer.
    async fn dispatch_consumer(&self, consumer: &Arc<dyn OutboxConsumer>) {
        let consumer_id = consumer.consumer_id().to_string();
        let claimed = match self
            .store
            .claim_batch(&consumer_id, &self.store_config(), &self.worker_id)
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
