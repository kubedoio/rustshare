//! Background worker claiming due external calendar sources and running
//! their read-only sync under a DB lease (issue #315).
//!
//! Lease model: `calendar_sync_states.locked_by/locked_at`. A source is
//! claimable when due (`next_sync_at <= now`) and unclaimed or stale. The
//! holder heartbeats during the run and releases the lease (recording the
//! next due time, cursor, and last-error) on completion or failure — the
//! same claim/stale-reset shape as the mail import worker.

use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::FutureExt;
use rustshare_storage::MetadataStore;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::services::google_calendar;
use crate::services::google_calendar::{
    CalendarSyncConfig, GoogleCalendarClient, SyncOutcome, DEFAULT_SYNC_INTERVAL,
};

pub struct CalendarSyncWorkerConfig {
    pub poll_interval: Duration,
    pub max_concurrent_jobs: usize,
    pub stale_threshold: Duration,
    pub sync: CalendarSyncConfig,
}

impl CalendarSyncWorkerConfig {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            poll_interval: Duration::from_secs(config.calendar_sync_worker_poll_secs),
            max_concurrent_jobs: config.calendar_sync_worker_max_concurrent,
            stale_threshold: Duration::from_secs(
                config.calendar_sync_worker_stale_secs.max(0) as u64
            ),
            sync: CalendarSyncConfig::from_config(config),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_calendar_sync_worker(
    metadata_store: Arc<MetadataStore>,
    secret_key: Arc<rustshare_crypto::SecretEncryptionKey>,
    google: Option<Arc<GoogleCalendarClient>>,
    mut shutdown: broadcast::Receiver<()>,
    config: CalendarSyncWorkerConfig,
) {
    let worker_id = format!("calendar-sync-{}", Uuid::new_v4());
    tokio::spawn(async move {
        let mut join_set = tokio::task::JoinSet::new();
        let mut in_flight_ids: HashSet<Uuid> = HashSet::new();

        loop {
            // Best-effort reaping of expired single-use OAuth states; the
            // connect flow deletes rows on consume, so only abandoned
            // (never-completed) consents accumulate here.
            match metadata_store.delete_expired_calendar_oauth_states().await {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!("Reaped {count} expired calendar OAuth states");
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to reap expired calendar OAuth states: {e}");
                }
            }

            while in_flight_ids.len() < config.max_concurrent_jobs {
                let source = match metadata_store
                    .claim_due_calendar_source(&worker_id, config.stale_threshold)
                    .await
                {
                    Ok(Some(source)) => source,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!("Failed to claim due calendar source: {e}");
                        break;
                    }
                };

                let source_id = source.id;
                in_flight_ids.insert(source_id);
                let store = Arc::clone(&metadata_store);
                let key = Arc::clone(&secret_key);
                let client = google.clone();
                let sync_config = CalendarSyncConfig {
                    past_days: config.sync.past_days,
                    future_days: config.sync.future_days,
                };
                let holder = worker_id.clone();
                join_set.spawn(async move {
                    let result =
                        AssertUnwindSafe(run_sync(store, key, client, source, sync_config, holder))
                            .catch_unwind()
                            .await;
                    match result {
                        Ok(()) => source_id,
                        Err(e) => {
                            tracing::error!("Calendar sync task {source_id} panicked: {e:?}");
                            source_id
                        }
                    }
                });
            }

            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("Calendar sync worker shutting down");
                    break;
                }
                _ = tokio::time::sleep(config.poll_interval) => {}
                res = join_set.join_next(), if !join_set.is_empty() => {
                    match res {
                        Some(Ok(source_id)) => {
                            in_flight_ids.remove(&source_id);
                        }
                        Some(Err(e)) => {
                            tracing::error!("Calendar sync task panicked or was aborted: {e}");
                        }
                        None => {}
                    }
                }
            }
        }

        if !join_set.is_empty() {
            tracing::info!(
                "Waiting for {} calendar sync tasks to finish",
                in_flight_ids.len()
            );
            let shutdown_timeout = Duration::from_secs(30);
            let _ = tokio::time::timeout(shutdown_timeout, async {
                while let Some(res) = join_set.join_next().await {
                    if let Err(e) = res {
                        tracing::error!("Calendar sync task panicked or was aborted: {e}");
                    }
                }
            })
            .await;
        }

        tracing::info!("Calendar sync worker stopped");
    });
}

/// Run one sync pass for a claimed source and release its lease, mapping the
/// outcome onto scheduling, cursor, and health bookkeeping. Public so the
/// integration suite can exercise the full worker path (claim → sync →
/// lease release) without spawning the polling loop.
#[allow(clippy::too_many_arguments)]
pub async fn run_sync(
    store: Arc<MetadataStore>,
    secret_key: Arc<rustshare_crypto::SecretEncryptionKey>,
    google: Option<Arc<GoogleCalendarClient>>,
    source: rustshare_core::domain::CalendarSource,
    sync_config: CalendarSyncConfig,
    worker_id: String,
) {
    // The pre-run cursor, preserved across a rate-limit backoff so the next
    // run stays incremental.
    let source_id = source.id;
    let prior_cursor = store
        .get_calendar_sync_state(source_id)
        .await
        .ok()
        .flatten()
        .and_then(|state| state.cursor_value);
    tracing::info!(source_id = %source_id, kind = %source.kind, "Syncing calendar source");
    let outcome = match source.kind.as_str() {
        "google" => match google {
            Some(client) => {
                google_calendar::sync_source(
                    &store,
                    &client,
                    &secret_key,
                    &source,
                    &sync_config,
                    &worker_id,
                )
                .await
            }
            None => SyncOutcome::Failed("google OAuth is not configured".to_string()),
        },
        // Outlook arrives with issue #315 Task 5; claim only what we can run.
        other => {
            tracing::debug!(source_id = %source_id, kind = %other, "no sync provider registered; skipping");
            SyncOutcome::Completed {
                upserted: 0,
                soft_deleted: 0,
                next_sync_token: None,
            }
        }
    };

    struct SyncPlan {
        next_sync_at: DateTime<Utc>,
        cursor: Option<String>,
        cursor_kind: Option<&'static str>,
        status: Option<&'static str>,
        last_error: Option<String>,
    }

    // Only successful runs advance `calendar_sources.last_synced_at`;
    // failures keep the previous watermark so dashboards do not report a
    // "sync" that changed nothing.
    let synced = matches!(outcome, SyncOutcome::Completed { .. });
    let now = Utc::now();
    let plan = match outcome {
        SyncOutcome::Completed {
            upserted,
            soft_deleted,
            next_sync_token,
        } => {
            tracing::info!(source_id = %source_id, upserted, soft_deleted, "calendar source synced");
            // A source parked in `auth_required` reaches this arm as a
            // deliberate no-op (sync_source early-returns); it must stay
            // parked — writing `healthy` here would oscillate the status
            // every poll cycle.
            let parked = source.status == "auth_required";
            SyncPlan {
                next_sync_at: now
                    + chrono::Duration::from_std(DEFAULT_SYNC_INTERVAL)
                        .unwrap_or(chrono::Duration::seconds(900)),
                cursor: next_sync_token,
                cursor_kind: Some("google_sync_token"),
                status: if parked { None } else { Some("healthy") },
                last_error: None,
            }
        }
        SyncOutcome::RateLimited { retry_after } => {
            let backoff =
                chrono::Duration::from_std(retry_after).unwrap_or(chrono::Duration::seconds(60));
            tracing::warn!(source_id = %source_id, "calendar sync rate limited; backing off");
            SyncPlan {
                next_sync_at: now + backoff,
                cursor: prior_cursor.clone(),
                cursor_kind: prior_cursor.is_some().then_some("google_sync_token"),
                status: Some("rate_limited"),
                last_error: Some("rate limited by provider".to_string()),
            }
        }
        SyncOutcome::AuthRequired => {
            tracing::warn!(source_id = %source_id, "calendar sync grant revoked; auth_required");
            SyncPlan {
                next_sync_at: now
                    + chrono::Duration::from_std(DEFAULT_SYNC_INTERVAL)
                        .unwrap_or(chrono::Duration::seconds(900)),
                cursor: None,
                cursor_kind: None,
                status: Some("auth_required"),
                last_error: Some("provider rejected the grant; reconnect required".to_string()),
            }
        }
        SyncOutcome::Failed(message) => {
            tracing::warn!(source_id = %source_id, "calendar sync failed: {message}");
            // Keep the pre-run cursor: a transient failure must not force
            // the next run into a full window sync (full syncs do not
            // propagate provider deletions, so a forced full resync can
            // silently resurrect deleted events).
            SyncPlan {
                next_sync_at: now
                    + chrono::Duration::from_std(DEFAULT_SYNC_INTERVAL)
                        .unwrap_or(chrono::Duration::seconds(900)),
                cursor: prior_cursor.clone(),
                cursor_kind: prior_cursor.is_some().then_some("google_sync_token"),
                status: Some("failed"),
                last_error: Some(message.chars().take(500).collect()),
            }
        }
    };

    let _ = store
        .heartbeat_calendar_source_lease(source_id, &worker_id)
        .await;
    if let Err(e) = store
        .finish_calendar_source_sync(
            source_id,
            &worker_id,
            plan.next_sync_at,
            plan.cursor_kind,
            plan.cursor.as_deref(),
            plan.last_error.as_deref(),
            synced,
        )
        .await
    {
        tracing::error!(source_id = %source_id, "failed to release calendar sync lease: {e}");
    }
    if let Some(status) = plan.status {
        if let Err(e) = store
            .update_calendar_source_status(source_id, status, plan.last_error.as_deref())
            .await
        {
            tracing::error!(source_id = %source_id, "failed to update source status: {e}");
        }
    }
}
