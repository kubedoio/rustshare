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
//! Consumer code runs inside a spawned task bounded by a per-event timeout
//! (`RUSTSHARE_OUTBOX_PROCESS_TIMEOUT_SECS`), so a panicking or wedged
//! consumer is contained (dead-letter / retryable) and can never kill or
//! stall the dispatch loop.
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
//! * `outbox_claim_seconds{consumer}` — claim → hand-off latency histogram
//!   for one batch.
//! * `outbox_pending_count{consumer,state}` — queue depth gauge, once per
//!   tick (drained groups are reset to 0).
//! * `outbox_dlq_count` — global dead-letter gauge, once per tick.
//! * `outbox_oldest_pending_age_seconds` — age of the oldest pending
//!   delivery, once per tick.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use rustshare_integration_events::redact::redact_error;
use rustshare_integration_events::{ConsumerOutcome, OutboxConsumer};
use rustshare_storage::{ClaimedEvent, OutboxConfig, OutboxStore};
use tokio::task::{JoinError, JoinHandle};
use tracing::{debug, info, warn};

use crate::config::OutboxWorkerConfig;

/// Delivery states surfaced by the `outbox_pending_count` gauge (mirrors the
/// `integration_deliveries.state` CHECK constraint).
const DELIVERY_STATES: [&str; 4] = ["pending", "claimed", "processed", "dead_lettered"];

/// A spawned task handle that aborts the task when dropped without the task
/// having completed.
///
/// Used for the consumer `process()` tasks: when the worker loop is torn down
/// (shutdown or runtime drop) the in-flight `process_claimed` future is
/// dropped and the spawned consumer task must not survive that cancellation —
/// a wedged `process()` that keeps running could overlap the delivery's next
/// redelivery, and a zombie per redelivery would grow unboundedly. `abort()`
/// on an already-completed task is a no-op, so the normal paths are
/// unaffected.
struct AbortOnDrop<T>(JoinHandle<T>);

impl<T> AbortOnDrop<T> {
    fn abort(&self) {
        self.0.abort();
    }
}

impl<T> Future for AbortOnDrop<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // `JoinHandle` is `Unpin` (a pointer wrapper), so re-pinning the
        // field is sound.
        Pin::new(&mut self.0).poll(cx)
    }
}

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

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
    ///
    /// The whole batch is bounded by `process_timeout * batch_len`: with a
    /// per-event timeout only, a wedged batch of `claim_batch_size` ×
    /// `process_timeout` could stall the tick for ~50 minutes, starving every
    /// other consumer and flipping the outbox readiness component unhealthy.
    /// The budget is enforced between events (never mid-event), so a started
    /// event always completes its full per-event cycle — timeout, abort+await,
    /// persist — deterministically, and the batch can exceed the budget by at
    /// most one per-event tail. Events that never started stay `claimed` and
    /// are reclaimed after lease expiry (at-least-once; the attempt count
    /// increments on re-claim, bounded by `max_attempts`).
    async fn dispatch_consumer(&self, consumer: &Arc<dyn OutboxConsumer>) {
        // Consumer-provided identity runs unprotected otherwise: a panic here
        // must skip the consumer, not kill the tick.
        let consumer_id = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            consumer.consumer_id().to_string()
        })) {
            Ok(consumer_id) => consumer_id,
            Err(_) => {
                warn!("outbox consumer identity query panicked; skipping consumer");
                return;
            }
        };
        let claim_started = Instant::now();
        let claimed = self
            .store
            .claim_batch(&consumer_id, &self.store_config(), &self.worker_id)
            .await;
        // Claim → hand-off latency (DB + lease time), regardless of outcome.
        metrics::histogram!("outbox_claim_seconds", "consumer" => consumer_id.clone())
            .record(claim_started.elapsed().as_secs_f64());
        let batch = match claimed {
            Ok(batch) => batch,
            Err(error) => {
                warn!(
                    %consumer_id,
                    error = %error,
                    "outbox claim failed; skipping consumer"
                );
                return;
            }
        };
        // Rows dead-lettered store-side during the claim (poison envelopes,
        // out-of-subscription rows) are part of the documented dead-letter
        // contract. Their event type is unrecoverable (that is why they are
        // poison), so they are counted under the fixed `unknown` label.
        if batch.poison_dead_lettered > 0 {
            metrics::counter!(
                "outbox_dead_lettered_total",
                "consumer" => consumer_id.clone(),
                "event_type" => "unknown"
            )
            .increment(batch.poison_dead_lettered as u64);
        }
        metrics::counter!("outbox_dispatched_total", "consumer" => consumer_id.clone())
            .increment(batch.len() as u64);
        let batch_len = batch.deliveries.len();
        // Saturating budget: `Duration::saturating_mul` cannot panic on
        // overflow, and an absurd batch length saturates to `Duration::MAX`.
        // A zero budget (empty batch) is harmless: the loop body never runs.
        let batch_budget = self
            .config
            .process_timeout
            .saturating_mul(u32::try_from(batch_len).unwrap_or(u32::MAX));
        let batch_deadline = Instant::now()
            .checked_add(batch_budget)
            .unwrap_or(Instant::now());
        for claimed_event in batch.deliveries {
            // Enforce the budget at the event boundary, never mid-event: the
            // in-flight per-event cycle must not be cancelled before its
            // persist lands.
            let remaining = batch_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                warn!(
                    %consumer_id,
                    batch_len,
                    process_timeout_secs = self.config.process_timeout.as_secs(),
                    "outbox consumer batch budget exhausted; unprocessed claimed rows remain claimed until lease expiry"
                );
                break;
            }
            self.process_claimed(
                consumer,
                &consumer_id,
                &claimed_event,
                remaining.min(self.config.process_timeout),
            )
            .await;
        }
    }

    /// Run one claimed event through the consumer and persist the outcome.
    ///
    /// The consumer runs in a spawned task bounded by `timeout` (the
    /// configured per-event `process_timeout`, or the remaining batch budget
    /// in [`Self::dispatch_consumer`] when it is smaller), so consumer code
    /// can never kill or stall the dispatch loop:
    ///
    /// * a panicking consumer surfaces as a `JoinError` — the delivery is
    ///   dead-lettered (reason `consumer panicked`);
    /// * a wedged consumer trips the timeout — the task is aborted and joined
    ///   BEFORE the delivery is failed retryable, so the redelivery can never
    ///   overlap the previous invocation (the contract requires idempotency,
    ///   not concurrency safety), and a wedged task cannot linger as a zombie
    ///   holding a 128 KiB event clone; the delivery re-enters the bounded
    ///   backoff, dead-lettering only when attempts are exhausted.
    ///
    /// The handle is additionally wrapped in [`AbortOnDrop`], so if a
    /// higher-level deadline drops this future mid-flight (the worker loop
    /// being torn down), the spawned task is aborted rather than detached.
    ///
    /// Both reasons are fixed strings (the store redacts them anyway); the
    /// panic payload is deliberately not persisted.
    async fn process_claimed(
        &self,
        consumer: &Arc<dyn OutboxConsumer>,
        consumer_id: &str,
        claimed: &ClaimedEvent,
        timeout: Duration,
    ) {
        let started = Instant::now();
        let event = claimed.event.clone();
        let consumer = consumer.clone();
        let task = AbortOnDrop(tokio::task::spawn(
            async move { consumer.process(&event).await },
        ));
        tokio::pin!(task);
        let outcome = match tokio::time::timeout(timeout, task.as_mut()).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_join_error)) => {
                warn!(
                    %consumer_id,
                    source = %claimed.source,
                    event_id = %claimed.event_id,
                    "outbox consumer panicked; dead-lettering the delivery"
                );
                ConsumerOutcome::Permanent {
                    reason: "consumer panicked".to_string(),
                }
            }
            Err(_elapsed) => {
                warn!(
                    %consumer_id,
                    source = %claimed.source,
                    event_id = %claimed.event_id,
                    process_timeout_secs = self.config.process_timeout.as_secs(),
                    "outbox consumer timed out; aborting the task and failing the delivery retryable"
                );
                task.abort();
                let _ = task.await;
                ConsumerOutcome::Retryable {
                    reason: "processing timed out".to_string(),
                }
            }
        };
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
    ///
    /// `outbox_pending_count` is a gauge, so Prometheus keeps re-rendering
    /// the last value of a (consumer, state) group once it exists; groups
    /// that drained are therefore reset to 0 over the expected label set
    /// (registered consumers × delivery states) so queue depth never shows a
    /// stale count.
    async fn record_queue_depth(&self) {
        match self.store.pending_counts().await {
            Ok(counts) => {
                let mut present = HashSet::with_capacity(counts.len());
                for count in counts {
                    present.insert((count.consumer_id.clone(), count.state.clone()));
                    metrics::gauge!(
                        "outbox_pending_count",
                        "consumer" => count.consumer_id,
                        "state" => count.state
                    )
                    .set(count.count as f64);
                }
                for consumer in &self.consumers {
                    let consumer_id = consumer.consumer_id().to_string();
                    for state in DELIVERY_STATES {
                        if !present.contains(&(consumer_id.clone(), state.to_string())) {
                            metrics::gauge!(
                                "outbox_pending_count",
                                "consumer" => consumer_id.clone(),
                                "state" => state.to_string()
                            )
                            .set(0.0);
                        }
                    }
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
            Ok(None) => {
                // Nothing pending: the gauge must not re-render a stale age.
                metrics::gauge!("outbox_oldest_pending_age_seconds").set(0.0);
            }
            Err(error) => {
                warn!(error = %error, "outbox oldest-pending gauge failed");
            }
        }
    }
}
