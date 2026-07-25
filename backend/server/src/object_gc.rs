//! Reference-aware garbage collection for global content-addressed blobs.

use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use rand::RngExt;
use rustshare_storage::{BlobReferenceSummary, MetadataStore, ObjectGcCandidate, ObjectStore};
use tracing::{error, info, warn};
use uuid::Uuid;

const MIN_INTERVAL_SECONDS: u64 = 10;
const MAX_BATCH_SIZE: i64 = 1_000;
const MAX_ERROR_BYTES: usize = 1_000;

#[derive(Debug, Clone)]
pub struct ObjectGcConfig {
    pub enabled: bool,
    pub interval: Duration,
    pub batch_size: i64,
    pub grace_period_hours: i64,
    pub max_attempts: i32,
    pub lease_seconds: i64,
    pub max_backoff_seconds: i64,
}

impl ObjectGcConfig {
    pub fn from_env() -> Result<Self> {
        let enabled = env_parse("RUSTSHARE_OBJECT_GC_ENABLED", false)?;
        let interval_seconds = env_parse("RUSTSHARE_OBJECT_GC_INTERVAL_SECONDS", 300u64)?;
        let batch_size = env_parse("RUSTSHARE_OBJECT_GC_BATCH_SIZE", 50i64)?;
        let grace_period_hours = env_parse("RUSTSHARE_OBJECT_GC_GRACE_PERIOD_HOURS", 24i64)?;
        let max_attempts = env_parse("RUSTSHARE_OBJECT_GC_MAX_ATTEMPTS", 10i32)?;
        let lease_seconds = env_parse("RUSTSHARE_OBJECT_GC_LEASE_SECONDS", 900i64)?;
        let max_backoff_seconds = env_parse("RUSTSHARE_OBJECT_GC_MAX_BACKOFF_SECONDS", 86_400i64)?;

        anyhow::ensure!(
            interval_seconds >= MIN_INTERVAL_SECONDS,
            "RUSTSHARE_OBJECT_GC_INTERVAL_SECONDS must be at least {MIN_INTERVAL_SECONDS}"
        );
        anyhow::ensure!(
            (1..=MAX_BATCH_SIZE).contains(&batch_size),
            "RUSTSHARE_OBJECT_GC_BATCH_SIZE must be between 1 and {MAX_BATCH_SIZE}"
        );
        anyhow::ensure!(grace_period_hours >= 1, "GC grace period must be positive");
        anyhow::ensure!(max_attempts >= 1, "GC max attempts must be positive");
        anyhow::ensure!(lease_seconds >= 30, "GC lease must be at least 30 seconds");
        anyhow::ensure!(
            max_backoff_seconds >= MIN_INTERVAL_SECONDS as i64,
            "GC max backoff must be at least {MIN_INTERVAL_SECONDS} seconds"
        );

        Ok(Self {
            enabled,
            interval: Duration::from_secs(interval_seconds),
            batch_size,
            grace_period_hours,
            max_attempts,
            lease_seconds,
            max_backoff_seconds,
        })
    }
}

fn env_parse<T>(name: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|error| anyhow::anyhow!("invalid {name}: {error}")),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error.into()),
    }
}

pub fn spawn_object_gc_worker(
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    config: ObjectGcConfig,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    if !config.enabled {
        info!("Object GC worker disabled; candidate enqueueing remains active");
        return;
    }

    tokio::spawn(async move {
        info!(
            interval_seconds = config.interval.as_secs(),
            batch_size = config.batch_size,
            grace_period_hours = config.grace_period_hours,
            "Object GC worker started"
        );
        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    info!("Object GC worker shutting down");
                    break;
                }
                _ = tokio::time::sleep(config.interval) => {
                    if let Err(error) = tick(&metadata_store, &object_store, &config).await {
                        error!(error = %error, "Object GC worker tick failed");
                    }
                }
            }
        }
    });
}

/// Process one bounded batch. Exposed for infrastructure contract tests.
pub async fn tick(
    metadata_store: &MetadataStore,
    object_store: &ObjectStore,
    config: &ObjectGcConfig,
) -> Result<()> {
    let worker_id = Uuid::new_v4().to_string();
    let candidates = metadata_store
        .lease_object_gc_candidates(
            config.batch_size,
            config.lease_seconds,
            &worker_id,
            config.grace_period_hours,
        )
        .await
        .context("failed to lease object GC candidates")?;

    for candidate in candidates {
        process_candidate(metadata_store, object_store, config, &worker_id, &candidate).await;
    }

    let (pending, oldest_seconds) = metadata_store.object_gc_backlog().await?;
    metrics::gauge!("object_gc_pending_candidates").set(pending as f64);
    metrics::gauge!("object_gc_oldest_pending_seconds")
        .set(oldest_seconds.unwrap_or_default().max(0) as f64);
    Ok(())
}

async fn process_candidate(
    metadata_store: &MetadataStore,
    object_store: &ObjectStore,
    config: &ObjectGcConfig,
    worker_id: &str,
    candidate: &ObjectGcCandidate,
) {
    metrics::counter!("object_gc_candidates_processed_total").increment(1);

    if !is_content_addressed_blob_key(&candidate.object_key) {
        metrics::counter!("object_gc_candidates_invalid_total").increment(1);
        if let Err(error) = metadata_store
            .complete_object_gc_candidate(&candidate.object_key, worker_id, "invalid_key")
            .await
        {
            warn!(candidate_id = %candidate.id, error = %error, "Failed to record invalid GC key");
        }
        warn!(candidate_id = %candidate.id, reason = %candidate.reason, "Rejected invalid object GC key");
        return;
    }

    let result = process_locked_candidate(metadata_store, object_store, worker_id, candidate).await;
    if let Err(error) = result {
        metrics::counter!("object_gc_delete_failures_total").increment(1);
        let error_text = bounded_error(&error);
        let backoff = retry_backoff_seconds(candidate.attempt_count, config.max_backoff_seconds);
        if let Err(record_error) = metadata_store
            .retry_object_gc_candidate(&candidate.object_key, worker_id, &error_text, backoff)
            .await
        {
            error!(candidate_id = %candidate.id, error = %record_error, "Failed to record object GC retry");
        }
        if candidate.attempt_count >= config.max_attempts {
            error!(candidate_id = %candidate.id, attempt = candidate.attempt_count, error = %error, "Object GC candidate exceeded maximum attempts");
        } else {
            warn!(candidate_id = %candidate.id, attempt = candidate.attempt_count, error = %error, "Object GC candidate deferred");
        }
    }
}

async fn process_locked_candidate(
    metadata_store: &MetadataStore,
    object_store: &ObjectStore,
    worker_id: &str,
    candidate: &ObjectGcCandidate,
) -> Result<()> {
    let _blob_lock = object_store
        .acquire_blob_lock(&candidate.object_key)
        .await
        .context("failed to acquire blob GC lock")?;

    let first = metadata_store
        .count_blob_references(&candidate.object_key)
        .await
        .inspect_err(|_| {
            metrics::counter!("object_gc_reference_check_failures_total").increment(1);
        })
        .context("first blob reference check failed")?;
    if first.total() > 0 {
        metrics::counter!("object_gc_candidates_referenced_total").increment(1);
        metadata_store
            .complete_object_gc_candidate(&candidate.object_key, worker_id, "referenced")
            .await?;
        return Ok(());
    }

    if !object_store
        .exists(&candidate.object_key)
        .await
        .context("object existence check failed")?
    {
        metrics::counter!("object_gc_blobs_missing_total").increment(1);
        metadata_store
            .complete_object_gc_candidate(&candidate.object_key, worker_id, "missing")
            .await?;
        return Ok(());
    }

    let second = metadata_store
        .count_blob_references(&candidate.object_key)
        .await
        .inspect_err(|_| {
            metrics::counter!("object_gc_reference_check_failures_total").increment(1);
        })
        .context("second blob reference check failed")?;
    if !deletion_allowed(&first, &second) {
        metrics::counter!("object_gc_candidates_referenced_total").increment(1);
        metadata_store
            .complete_object_gc_candidate(&candidate.object_key, worker_id, "referenced")
            .await?;
        return Ok(());
    }

    object_store
        .delete(&candidate.object_key)
        .await
        .context("object delete failed")?;
    metadata_store
        .complete_object_gc_candidate(&candidate.object_key, worker_id, "deleted")
        .await?;
    metrics::counter!("object_gc_blobs_deleted_total").increment(1);
    info!(candidate_id = %candidate.id, reason = %candidate.reason, "Deleted unreferenced content-addressed blob");
    Ok(())
}

fn deletion_allowed(first: &BlobReferenceSummary, second: &BlobReferenceSummary) -> bool {
    first.total() == 0 && second.total() == 0
}

pub(crate) fn is_content_addressed_blob_key(key: &str) -> bool {
    key.strip_prefix("blobs/").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn retry_backoff_seconds(attempt: i32, maximum: i64) -> i64 {
    let exponent = u32::try_from(attempt.saturating_sub(1).min(20)).unwrap_or_default();
    let base = 30i64.saturating_mul(2i64.saturating_pow(exponent));
    let capped = base.min(maximum).max(1);
    let jitter = rand::rng().random_range(0..=(capped / 4));
    capped.saturating_add(jitter).min(maximum)
}

fn bounded_error(error: &anyhow::Error) -> String {
    let text = error.to_string();
    if text.len() <= MAX_ERROR_BYTES {
        return text;
    }
    let mut end = MAX_ERROR_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_validation_accepts_only_canonical_blob_keys() {
        assert!(is_content_addressed_blob_key(&format!(
            "blobs/{}",
            "a".repeat(64)
        )));
        for key in [
            "",
            "blobs/",
            "blobs/../secret",
            "blobs/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "temp/uploads/00000000-0000-0000-0000-000000000000/0",
            "https://store/blobs/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            assert!(!is_content_addressed_blob_key(key), "accepted {key}");
        }
    }

    #[test]
    fn bounded_error_does_not_split_utf8() {
        let error = anyhow::anyhow!("{}", "é".repeat(600));
        assert!(bounded_error(&error).len() <= MAX_ERROR_BYTES);
    }

    #[test]
    fn reference_created_between_checks_cancels_deletion() {
        let first = BlobReferenceSummary::default();
        let second = BlobReferenceSummary {
            vault_files: 1,
            ..BlobReferenceSummary::default()
        };
        assert!(!deletion_allowed(&first, &second));
    }
}
