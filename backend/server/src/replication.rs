use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::{config::Credentials, primitives::ByteStream, Client as S3Client};
use chrono::Utc;
use rustshare_core::{
    domain::{ReplicationJob, ReplicationState, ReplicationTarget},
    events::{AggregateType, Event, EventBroadcaster, EventType, ReplicationStateChangedPayload},
};
use rustshare_storage::{EventStore, MetadataStore, ObjectStore, ReplicationAttemptRecord};
use serde::Deserialize;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ReplicationWorkerConfig {
    pub enabled: bool,
    pub poll_interval: Duration,
    pub batch_size: i64,
    pub lease_timeout_secs: i64,
    pub max_attempts: i32,
}

#[derive(Debug, Clone)]
struct ReplicationEventContext<'a> {
    job: &'a ReplicationJob,
    replication_state: ReplicationState,
    job_status: Option<String>,
    attempt_count: i32,
    next_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
    last_error: Option<String>,
}

impl ReplicationWorkerConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("REPLICATION_WORKER_ENABLED")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(true),
            poll_interval: Duration::from_millis(
                std::env::var("REPLICATION_WORKER_POLL_INTERVAL_MS")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(5_000),
            ),
            batch_size: std::env::var("REPLICATION_WORKER_BATCH_SIZE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8),
            lease_timeout_secs: std::env::var("REPLICATION_WORKER_LEASE_TIMEOUT_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(120),
            max_attempts: std::env::var("REPLICATION_WORKER_MAX_ATTEMPTS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8),
        }
    }
}

#[derive(Debug, Deserialize)]
struct TargetAuthConfig {
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
    session_token: Option<String>,
}

pub fn spawn_replication_worker(
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    event_store: Arc<EventStore>,
    broadcaster: Arc<EventBroadcaster>,
    config: ReplicationWorkerConfig,
) {
    if !config.enabled {
        info!("Replication worker disabled");
        return;
    }

    tokio::spawn(async move {
        info!(
            batch_size = config.batch_size,
            lease_timeout_secs = config.lease_timeout_secs,
            poll_interval_ms = config.poll_interval.as_millis(),
            "Replication worker started"
        );

        loop {
            if let Err(error) = tick_replication_worker(
                &metadata_store,
                &object_store,
                &event_store,
                &broadcaster,
                &config,
            )
            .await
            {
                error!(error = %error, "Replication worker tick failed");
            }

            tokio::time::sleep(config.poll_interval).await;
        }
    });
}

async fn tick_replication_worker(
    metadata_store: &MetadataStore,
    object_store: &ObjectStore,
    event_store: &EventStore,
    broadcaster: &EventBroadcaster,
    config: &ReplicationWorkerConfig,
) -> Result<()> {
    let lease_token = Uuid::new_v4();
    let jobs = metadata_store
        .lease_replication_jobs(config.batch_size, config.lease_timeout_secs, lease_token)
        .await
        .context("failed to lease replication jobs")?;

    if jobs.is_empty() {
        debug!("No replication jobs ready");
        return Ok(());
    }

    let targets = metadata_store
        .list_enabled_replication_targets()
        .await
        .context("failed to load replication targets")?;

    for job in jobs {
        if targets.is_empty() {
            metadata_store
                .mark_replication_job_completed(job.id)
                .await
                .with_context(|| format!("failed to complete job {} with zero targets", job.id))?;
            metadata_store
                .update_file_version_replication_state(
                    job.file_version_id,
                    ReplicationState::FullyReplicated,
                )
                .await
                .with_context(|| {
                    format!(
                        "failed to mark file version {} fully replicated",
                        job.file_version_id
                    )
                })?;
            publish_replication_event(
                metadata_store,
                event_store,
                broadcaster,
                ReplicationEventContext {
                    job: &job,
                    replication_state: ReplicationState::FullyReplicated,
                    job_status: Some("completed".to_string()),
                    attempt_count: job.attempt_count,
                    next_attempt_at: None,
                    last_error: None,
                },
            )
            .await?;
            continue;
        }

        process_replication_job(
            metadata_store,
            object_store,
            event_store,
            broadcaster,
            config,
            &job,
            &targets,
        )
        .await?;
    }

    Ok(())
}

async fn process_replication_job(
    metadata_store: &MetadataStore,
    object_store: &ObjectStore,
    event_store: &EventStore,
    broadcaster: &EventBroadcaster,
    config: &ReplicationWorkerConfig,
    job: &ReplicationJob,
    targets: &[ReplicationTarget],
) -> Result<()> {
    metadata_store
        .update_file_version_replication_state(job.file_version_id, ReplicationState::Syncing)
        .await
        .with_context(|| format!("failed to mark version {} syncing", job.file_version_id))?;
    publish_replication_event(
        metadata_store,
        event_store,
        broadcaster,
        ReplicationEventContext {
            job,
            replication_state: ReplicationState::Syncing,
            job_status: Some("syncing".to_string()),
            attempt_count: job.attempt_count,
            next_attempt_at: None,
            last_error: None,
        },
    )
    .await?;

    let blob = object_store
        .get(&job.storage_key)
        .await
        .with_context(|| format!("failed to read primary blob {}", job.storage_key))?;

    let mut required_failures = Vec::new();

    for target in targets {
        let started_at = Utc::now();
        let attempt_result = replicate_to_target(target, &job.storage_key, blob.clone()).await;
        let completed_at = Utc::now();

        match attempt_result {
            Ok(()) => {
                metadata_store
                    .create_replication_attempt(ReplicationAttemptRecord {
                        job_id: job.id,
                        target_id: target.id,
                        attempt_number: job.attempt_count,
                        status: "completed",
                        error_message: None,
                        started_at,
                        completed_at,
                    })
                    .await
                    .with_context(|| {
                        format!(
                            "failed to record successful attempt for job {} target {}",
                            job.id, target.id
                        )
                    })?;
                metadata_store
                    .update_replication_target_health(
                        target.id,
                        "healthy",
                        None,
                        Some(completed_at),
                    )
                    .await
                    .with_context(|| {
                        format!("failed to update target {} health after success", target.id)
                    })?;
            }
            Err(error) => {
                let error_text = error.to_string();
                metadata_store
                    .create_replication_attempt(ReplicationAttemptRecord {
                        job_id: job.id,
                        target_id: target.id,
                        attempt_number: job.attempt_count,
                        status: "failed",
                        error_message: Some(&error_text),
                        started_at,
                        completed_at,
                    })
                    .await
                    .with_context(|| {
                        format!(
                            "failed to record failed attempt for job {} target {}",
                            job.id, target.id
                        )
                    })?;
                metadata_store
                    .update_replication_target_health(
                        target.id,
                        "unhealthy",
                        Some(&error_text),
                        None,
                    )
                    .await
                    .with_context(|| {
                        format!("failed to update target {} health after error", target.id)
                    })?;

                if target.is_required {
                    required_failures.push(error_text);
                } else {
                    warn!(
                        job_id = %job.id,
                        target_id = %target.id,
                        "Optional replication target failed"
                    );
                }
            }
        }
    }

    if required_failures.is_empty() {
        metadata_store
            .mark_replication_job_completed(job.id)
            .await
            .with_context(|| format!("failed to mark job {} completed", job.id))?;
        metadata_store
            .update_file_version_replication_state(
                job.file_version_id,
                ReplicationState::FullyReplicated,
            )
            .await
            .with_context(|| {
                format!(
                    "failed to mark file version {} fully replicated",
                    job.file_version_id
                )
            })?;
        publish_replication_event(
            metadata_store,
            event_store,
            broadcaster,
            ReplicationEventContext {
                job,
                replication_state: ReplicationState::FullyReplicated,
                job_status: Some("completed".to_string()),
                attempt_count: job.attempt_count,
                next_attempt_at: None,
                last_error: None,
            },
        )
        .await?;

        info!(job_id = %job.id, "Replication job completed");
        return Ok(());
    }

    let error_text = required_failures.join(" | ");
    if is_retryable_error(&error_text) && job.attempt_count < config.max_attempts {
        let backoff = retry_backoff_secs(job.attempt_count);
        let next_attempt_at = Utc::now() + chrono::Duration::seconds(i64::from(backoff));

        metadata_store
            .mark_replication_job_retrying(job.id, &error_text, next_attempt_at)
            .await
            .with_context(|| format!("failed to mark job {} retrying", job.id))?;
        metadata_store
            .update_file_version_replication_state(job.file_version_id, ReplicationState::Degraded)
            .await
            .with_context(|| {
                format!(
                    "failed to mark file version {} degraded",
                    job.file_version_id
                )
            })?;
        publish_replication_event(
            metadata_store,
            event_store,
            broadcaster,
            ReplicationEventContext {
                job,
                replication_state: ReplicationState::Degraded,
                job_status: Some("retrying".to_string()),
                attempt_count: job.attempt_count,
                next_attempt_at: Some(next_attempt_at),
                last_error: Some(error_text.clone()),
            },
        )
        .await?;

        warn!(
            job_id = %job.id,
            attempt = job.attempt_count,
            backoff_secs = backoff,
            "Replication job scheduled for retry"
        );
        return Ok(());
    }

    metadata_store
        .mark_replication_job_failed(job.id, &error_text)
        .await
        .with_context(|| format!("failed to mark job {} failed", job.id))?;
    metadata_store
        .update_file_version_replication_state(job.file_version_id, ReplicationState::Failed)
        .await
        .with_context(|| format!("failed to mark file version {} failed", job.file_version_id))?;
    publish_replication_event(
        metadata_store,
        event_store,
        broadcaster,
        ReplicationEventContext {
            job,
            replication_state: ReplicationState::Failed,
            job_status: Some("failed".to_string()),
            attempt_count: job.attempt_count,
            next_attempt_at: None,
            last_error: Some(error_text),
        },
    )
    .await?;

    error!(job_id = %job.id, "Replication job exhausted retries");
    Ok(())
}

async fn publish_replication_event(
    metadata_store: &MetadataStore,
    event_store: &EventStore,
    broadcaster: &EventBroadcaster,
    context: ReplicationEventContext<'_>,
) -> Result<()> {
    let file = metadata_store
        .find_file_by_id_unchecked(context.job.file_id)
        .await
        .with_context(|| {
            format!(
                "failed to load file {} for replication event",
                context.job.file_id
            )
        })?
        .context("replication job references missing file")?;

    let payload = ReplicationStateChangedPayload {
        file_id: context.job.file_id,
        file_version_id: context.job.file_version_id,
        replication_state: context.replication_state,
        job_status: context.job_status,
        attempt_count: context.attempt_count,
        next_attempt_at: context.next_attempt_at,
        last_error: context.last_error,
        updated_at: chrono::Utc::now(),
    };

    let event = Event::new(
        EventType::ReplicationStateChanged,
        context.job.file_id,
        AggregateType::File,
        serde_json::to_value(payload).context("failed to serialize replication payload")?,
        file.owner_id,
    );

    event_store
        .append(&event, broadcaster)
        .await
        .context("failed to append replication event")?;

    Ok(())
}

async fn replicate_to_target(
    target: &ReplicationTarget,
    storage_key: &str,
    blob: bytes::Bytes,
) -> Result<()> {
    if !matches!(
        target.destination_type.as_str(),
        "rustfs" | "s3" | "s3_compatible"
    ) {
        anyhow::bail!(
            "unsupported replication target type `{}` for target {}",
            target.destination_type,
            target.id
        );
    }

    let bucket = target
        .bucket
        .as_ref()
        .context("replication target bucket is missing")?;
    let region = target
        .region
        .as_ref()
        .context("replication target region is missing")?;

    let client = build_target_client(target, region).await?;
    let target_key = build_target_key(target.base_path.as_deref(), storage_key);

    client
        .put_object()
        .bucket(bucket)
        .key(target_key)
        .body(ByteStream::from(blob))
        .send()
        .await
        .context("failed to upload replica object")?;

    Ok(())
}

async fn build_target_client(target: &ReplicationTarget, region: &str) -> Result<S3Client> {
    let shared_config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(&target.endpoint)
        .region(aws_config::Region::new(region.to_string()))
        .load()
        .await;

    let mut builder = aws_sdk_s3::config::Builder::from(&shared_config).force_path_style(true);

    if let Some(auth_value) = target.auth_config.clone() {
        let auth: TargetAuthConfig =
            serde_json::from_value(auth_value).context("invalid replication target auth_config")?;
        if let (Some(access_key_id), Some(secret_access_key)) =
            (auth.access_key_id, auth.secret_access_key)
        {
            builder = builder.credentials_provider(Credentials::new(
                access_key_id,
                secret_access_key,
                auth.session_token,
                None,
                "replication-target",
            ));
        }
    }

    Ok(S3Client::from_conf(builder.build()))
}

fn build_target_key(base_path: Option<&str>, storage_key: &str) -> String {
    match base_path.map(str::trim).filter(|value| !value.is_empty()) {
        Some(prefix) => format!(
            "{}/{}",
            prefix.trim_end_matches('/'),
            storage_key.trim_start_matches('/')
        ),
        None => storage_key.to_string(),
    }
}

fn retry_backoff_secs(attempt_count: i32) -> i32 {
    let exponent = attempt_count.clamp(1, 6) as u32;
    5 * 2_i32.pow(exponent)
}

fn is_retryable_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();

    !message.contains("accessdenied")
        && !message.contains("invalidaccesskeyid")
        && !message.contains("signaturedoesnotmatch")
        && !message.contains("forbidden")
        && !message.contains("authorization")
        && !message.contains("unsupported replication target type")
        && !message.contains("bucket is missing")
        && !message.contains("region is missing")
        && !message.contains("auth_config")
}
