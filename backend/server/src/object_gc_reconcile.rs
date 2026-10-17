//! Safe orphan-blob reconciliation producer (Phase 3.4B).
//!
//! This module is the producer half of orphan reconciliation: it turns the
//! Phase 3.4A read-only discovery into durable GC queue candidates. A listed,
//! aged, unreferenced blob in an *unversioned* bucket is enqueued insert-only
//! (`MetadataStore::enqueue_object_gc_candidate_if_absent`); everything else
//! is skipped or counted as uncertain. Deletion is never performed here and
//! cannot be reached here: by construction the only object-storage seam is
//! [`BlobInspector`] (list + HeadObject + versioning probe) and the only
//! mutation is the insert-only queue enqueue. `ObjectStore::delete` is
//! deliberately not referenced anywhere in this module.

use std::{fmt, future::Future, pin::Pin, time::Instant};

use axum::extract::{Json, State};
use chrono::{DateTime, Utc};
use rustshare_storage::{BucketVersioningState, MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::handlers::{AdminUser, AppError};
use crate::object_gc::is_content_addressed_blob_key;
use crate::object_gc_scan::{
    BlobLister, ListedBlob, OrphanScanConfig, ScanError, DEFAULT_MAX_KEYS_PER_INVOCATION,
    MAX_KEYS_PER_INVOCATION_CAP,
};
use crate::AppState;

/// Durable queue reason recorded for producer-enqueued orphan candidates.
pub const ORPHAN_RECONCILE_REASON: &str = "orphan_reconciliation";

/// Default maximum number of fresh queue rows one invocation may insert.
pub const DEFAULT_MAX_ENQUEUE_PER_RUN: u32 = 100;
/// Hard cap for [`OrphanReconciliationConfig::max_enqueue_per_run`]. Must not
/// exceed the scan key budget cap: an enqueue cap above the number of keys an
/// invocation can process is meaningless and hides misconfiguration.
pub const MAX_ENQUEUE_PER_RUN_CAP: u32 = MAX_KEYS_PER_INVOCATION_CAP;

const MAX_RECONCILE_ERROR_BYTES: usize = 500;
const MIN_GRACE_PERIOD_HOURS: i64 = 1;

/// Failure modes of a reconciliation invocation.
///
/// Per-key probe failures (HeadObject, advisory lock, metadata queries) do
/// not appear here: they degrade into uncertainty counters on the report so
/// a partial picture is never mistaken for a complete one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileError {
    /// `RUSTSHARE_ORPHAN_RECONCILIATION_ENABLED` is not set: the invocation
    /// refuses to run and nothing is mutated.
    Disabled,
    /// The bucket's versioning probe succeeded and reported a state other
    /// than `Unversioned`; previously versioned objects may still exist, so
    /// reconciliation is unsafe.
    VersioningUnsafe(BucketVersioningState),
    /// The bucket's versioning probe failed; the bucket's history is
    /// unproven, so reconciliation fails closed.
    VersioningUnknown,
    /// The reconciliation configuration is invalid.
    InvalidConfig(String),
    /// Object-store/lock probe transport failure at the seam boundary.
    /// Callers of [`reconcile_orphan_blobs`] never observe this variant: it
    /// only exists so the dyn seams can report failure and is degraded into
    /// `uncertain_s3`/`uncertain_db` counters per key.
    Probe(String),
}

impl fmt::Display for ReconcileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReconcileError::Disabled => write!(f, "orphan reconciliation is disabled"),
            ReconcileError::VersioningUnsafe(state) => write!(
                f,
                "bucket versioning state {} is unsafe for orphan reconciliation",
                versioning_label(*state)
            ),
            ReconcileError::VersioningUnknown => write!(
                f,
                "bucket versioning state could not be determined; failing closed"
            ),
            ReconcileError::InvalidConfig(message) => {
                write!(f, "invalid orphan reconciliation configuration: {message}")
            }
            ReconcileError::Probe(message) => write!(f, "object-store probe failed: {message}"),
        }
    }
}

impl std::error::Error for ReconcileError {}

/// Per-invocation configuration for the orphan reconciliation producer.
#[derive(Debug, Clone)]
pub struct OrphanReconciliationConfig {
    /// Master switch (`RUSTSHARE_ORPHAN_RECONCILIATION_ENABLED`, default off).
    pub enabled: bool,
    /// Stop inserting once this many fresh rows were enqueued this run.
    /// Default [`DEFAULT_MAX_ENQUEUE_PER_RUN`], hard cap
    /// [`MAX_ENQUEUE_PER_RUN_CAP`], and never above the scan key budget.
    pub max_enqueue_per_run: u32,
    /// Grace period recorded on freshly enqueued candidates
    /// (`RUSTSHARE_OBJECT_GC_GRACE_PERIOD_HOURS`, default 24, minimum 1).
    pub grace_period_hours: i64,
    /// Listing budget reused from the Phase 3.4A scanner (max keys, min age).
    pub scan: OrphanScanConfig,
}

impl OrphanReconciliationConfig {
    pub fn new(
        enabled: bool,
        max_enqueue_per_run: u32,
        grace_period_hours: i64,
        scan: OrphanScanConfig,
    ) -> Result<Self, ReconcileError> {
        // Validate only when enabled: `from_env` is evaluated per request
        // (not at server startup), and a disabled invocation must fail with
        // 403 Disabled rather than 400 from leftover bogus values
        // (mirrors ObjectGcConfig::from_env).
        if enabled {
            if max_enqueue_per_run == 0 || max_enqueue_per_run > MAX_ENQUEUE_PER_RUN_CAP {
                return Err(ReconcileError::InvalidConfig(format!(
                    "max_enqueue_per_run must be between 1 and {MAX_ENQUEUE_PER_RUN_CAP}"
                )));
            }
            if max_enqueue_per_run > scan.max_keys_per_invocation {
                return Err(ReconcileError::InvalidConfig(
                    "max_enqueue_per_run must not exceed the scan key budget".to_string(),
                ));
            }
            if grace_period_hours < MIN_GRACE_PERIOD_HOURS {
                return Err(ReconcileError::InvalidConfig(format!(
                    "grace_period_hours must be at least {MIN_GRACE_PERIOD_HOURS}"
                )));
            }
        }
        Ok(Self {
            enabled,
            max_enqueue_per_run,
            grace_period_hours,
            scan,
        })
    }

    pub fn from_env() -> Result<Self, ReconcileError> {
        let enabled = env_parse("RUSTSHARE_ORPHAN_RECONCILIATION_ENABLED", false)?;
        // These knobs are parsed only when enabled, exactly like the scan
        // sub-config below: with reconciliation off, a bogus leftover env var
        // must yield 403 Disabled, not 400. Fail-closed either way; the
        // simpler operator mental model wins.
        let (max_enqueue_per_run, grace_period_hours) = if enabled {
            (
                env_parse(
                    "RUSTSHARE_ORPHAN_RECONCILE_MAX_ENQUEUE",
                    DEFAULT_MAX_ENQUEUE_PER_RUN,
                )?,
                env_parse("RUSTSHARE_OBJECT_GC_GRACE_PERIOD_HOURS", 24i64)?,
            )
        } else {
            (DEFAULT_MAX_ENQUEUE_PER_RUN, 24)
        };
        let scan = if enabled {
            OrphanScanConfig::from_env()
                .map_err(|error| ReconcileError::InvalidConfig(error.to_string()))?
        } else {
            OrphanScanConfig::new(
                DEFAULT_MAX_KEYS_PER_INVOCATION,
                std::time::Duration::from_secs(24 * 3600),
            )
            .map_err(|error| ReconcileError::InvalidConfig(error.to_string()))?
        };
        Self::new(enabled, max_enqueue_per_run, grace_period_hours, scan)
    }
}

fn env_parse<T>(name: &str, default: T) -> Result<T, ReconcileError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|error| ReconcileError::InvalidConfig(format!("invalid {name}: {error}"))),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(ReconcileError::InvalidConfig(format!(
            "invalid {name}: {error}"
        ))),
    }
}

/// Boxed future returned by the reconciliation seams; keeps the traits
/// dyn-compatible (mirrors the `BlobLister` signature style).
pub type BoxReconcileFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ReconcileError>> + Send + 'a>>;

/// Read-only object-storage seam for reconciliation. Extends the scanner's
/// list-only [`BlobLister`] with a HeadObject probe and a bucket versioning
/// probe — still strictly no put/delete. The boxed futures keep the trait
/// dyn-compatible so entry points take `&dyn BlobInspector`.
pub trait BlobInspector: BlobLister {
    fn head_last_modified<'a>(
        &'a self,
        key: &'a str,
    ) -> BoxReconcileFuture<'a, Option<DateTime<Utc>>>;

    fn bucket_versioning<'a>(&'a self) -> BoxReconcileFuture<'a, BucketVersioningState>;
}

/// Guard for a held per-key blob lock; dropping releases it.
pub trait HeldLock: Send {}

/// Boxed [`HeldLock`] returned by [`BlobLockProvider::acquire`].
pub type HeldLockBox = Box<dyn HeldLock>;

/// Per-key advisory lock seam. `Ok(None)` marks a non-canonical key; `Err`
/// marks a lock-acquisition failure (degraded to `uncertain_db`).
pub trait BlobLockProvider: Send + Sync {
    fn acquire<'a>(&'a self, key: &'a str) -> BoxReconcileFuture<'a, Option<HeldLockBox>>;
}

/// Advisory-lock guard: the open transaction holds `pg_advisory_xact_lock`
/// for the key; dropping it rolls back the transaction and releases the lock.
struct PgBlobLockGuard {
    _transaction: Transaction<'static, Postgres>,
}

impl HeldLock for PgBlobLockGuard {}

impl BlobInspector for ObjectStore {
    fn head_last_modified<'a>(
        &'a self,
        key: &'a str,
    ) -> BoxReconcileFuture<'a, Option<DateTime<Utc>>> {
        Box::pin(async move {
            self.head_object_last_modified(key)
                .await
                .map_err(|error| ReconcileError::Probe(bounded_error_text(&error)))
        })
    }

    fn bucket_versioning<'a>(&'a self) -> BoxReconcileFuture<'a, BucketVersioningState> {
        Box::pin(async move {
            // Transport failures surface as Probe here; the reconciliation
            // entry point converts any probe error into VersioningUnknown.
            ObjectStore::bucket_versioning(self)
                .await
                .map_err(|error| ReconcileError::Probe(bounded_error_text(&error)))
        })
    }
}

impl BlobLockProvider for ObjectStore {
    fn acquire<'a>(&'a self, key: &'a str) -> BoxReconcileFuture<'a, Option<HeldLockBox>> {
        Box::pin(async move {
            let guard = self
                .acquire_blob_lock(key)
                .await
                .map_err(|error| ReconcileError::Probe(bounded_error_text(&error)))?;
            Ok(guard.map(|transaction| {
                Box::new(PgBlobLockGuard {
                    _transaction: transaction,
                }) as HeldLockBox
            }))
        })
    }
}

/// Overall outcome of one reconciliation invocation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileStatus {
    /// The invocation ended normally: either the listing was fully covered,
    /// or a budget (key budget / listing-call cap) was consumed and
    /// `next_continuation_token` resumes after the last processed page.
    #[default]
    Completed,
    /// A listing call failed mid-run. Counters reflect only what was
    /// processed before the failure; `next_continuation_token` retries the
    /// failed page.
    PartialListingError,
    /// `max_enqueue_per_run` fresh rows were inserted, so processing stopped
    /// mid-page. `next_continuation_token` is the token that yielded the
    /// current page: resuming re-scans that page, which is safe because
    /// enqueueing is insert-only and idempotent.
    EnqueueCapReached,
}

impl ReconcileStatus {
    fn as_str(self) -> &'static str {
        match self {
            ReconcileStatus::Completed => "completed",
            ReconcileStatus::PartialListingError => "partial_listing_error",
            ReconcileStatus::EnqueueCapReached => "enqueue_cap_reached",
        }
    }
}

/// Aggregate result of one reconciliation invocation. Counts only: no object
/// contents, no per-key detail, no first/last key strings.
#[derive(Debug, Clone, Default, Serialize)]
pub struct OrphanReconcileReport {
    pub status: ReconcileStatus,
    /// Keys returned by listing (every listed key, before classification).
    pub objects_listed: u64,
    /// Keys failing the canonical `blobs/<64 hex>` filter, plus canonical
    /// keys the lock provider refused as non-canonical.
    pub keys_invalid: u64,
    /// Objects younger than `min_object_age` at listing or HeadObject time.
    pub objects_too_recent: u64,
    /// Objects confirmed absent by HeadObject under the per-key lock.
    pub skipped_missing: u64,
    /// Objects with at least one durable reference at inspection time.
    pub skipped_referenced: u64,
    /// Objects whose queue row already existed (any non-held state).
    pub skipped_existing_queue_rows: u64,
    /// Objects whose queue row is operator-held and was left untouched.
    pub skipped_operator_holds: u64,
    /// Fresh `pending` queue rows inserted this run.
    pub enqueued: u64,
    /// Per-key metadata/lock failures; never read as zero references.
    pub uncertain_db: u64,
    /// Per-key HeadObject failures and post-listing object replacements.
    pub uncertain_s3: u64,
    /// `false` once any metadata query or lock acquisition failed.
    pub db_reachable: bool,
    /// `true` only when the listing ended without a continuation token.
    pub listing_complete: bool,
    /// Resumes the invocation. Semantics depend on `status` (see
    /// [`ReconcileStatus`]).
    pub next_continuation_token: Option<String>,
    /// Observed bucket versioning state; `"unversioned"` on every successful
    /// run (any other state aborts before a report exists).
    pub versioning: String,
    pub elapsed_ms: u64,
}

/// Produce durable GC candidates for orphaned blobs.
///
/// Execution order:
///
/// 1. Disabled config → [`ReconcileError::Disabled`]; nothing is mutated.
/// 2. Versioning gate: only `Ok(BucketVersioningState::Unversioned)` proceeds;
///    any other observed state → [`ReconcileError::VersioningUnsafe`]; a probe
///    failure → [`ReconcileError::VersioningUnknown`]. Both fail closed.
/// 3. Page through the listing under the scanner's budgets (key budget,
///    listing-call cap of `max(key budget, 100)`).
/// 4. Per listed key, in order: canonical-key check, listed-age check,
///    per-key advisory lock, HeadObject freshness/replacement re-check under
///    the lock, durable reference count, then insert-only enqueue. Each
///    failure mode degrades into its counter instead of aborting the run.
/// 5. A listing failure ends the run with `status = PartialListingError` and
///    partial counters intact — a partial failure never looks like a complete
///    scan.
///
/// When `enqueued` reaches `max_enqueue_per_run`, processing stops mid-run
/// with `status = EnqueueCapReached`, `listing_complete = false`, and
/// `next_continuation_token` set to the token that yielded the *current*
/// page. Resuming from that token re-scans the current page; that is safe
/// because enqueueing is insert-only and idempotent, so already-enqueued keys
/// classify as `skipped_existing_queue_rows` instead of being duplicated.
pub async fn reconcile_orphan_blobs(
    config: &OrphanReconciliationConfig,
    inspector: &dyn BlobInspector,
    locks: &dyn BlobLockProvider,
    metadata: &MetadataStore,
    continuation_token: Option<String>,
) -> Result<OrphanReconcileReport, ReconcileError> {
    if !config.enabled {
        return Err(ReconcileError::Disabled);
    }

    // Versioning gate, fail closed: only a proven-never-versioned bucket is
    // safe to reclaim objects from automatically.
    let versioning = match inspector.bucket_versioning().await {
        Ok(BucketVersioningState::Unversioned) => BucketVersioningState::Unversioned,
        Ok(state) => return Err(ReconcileError::VersioningUnsafe(state)),
        Err(_) => return Err(ReconcileError::VersioningUnknown),
    };

    let started = Instant::now();
    let mut report = OrphanReconcileReport {
        db_reachable: true,
        versioning: versioning_label(versioning).to_string(),
        ..OrphanReconcileReport::default()
    };
    let mut next_continuation = continuation_token;
    let max_page_calls = u64::from(config.scan.max_keys_per_invocation).max(100);
    let mut page_calls = 0u64;

    loop {
        page_calls += 1;
        // The token that yielded the current page; on an enqueue-cap stop the
        // resume token rewinds to it so the page is re-scanned (safe because
        // enqueueing is insert-only and idempotent).
        let current_page_token = next_continuation.clone();
        let page = match inspector
            .list_page(
                next_continuation.as_deref(),
                config.scan.max_keys_per_invocation,
            )
            .await
        {
            Ok(page) => page,
            Err(ScanError::List(_message)) => {
                report.status = ReconcileStatus::PartialListingError;
                report.next_continuation_token = current_page_token;
                break;
            }
            Err(ScanError::InvalidConfig(message)) => {
                return Err(ReconcileError::InvalidConfig(message))
            }
        };

        for blob in &page.blobs {
            report.objects_listed += 1;
            reconcile_key(config, inspector, locks, metadata, blob, &mut report).await;
            if report.enqueued >= u64::from(config.max_enqueue_per_run) {
                report.status = ReconcileStatus::EnqueueCapReached;
                report.next_continuation_token = current_page_token;
                break;
            }
        }
        if report.status == ReconcileStatus::EnqueueCapReached {
            break;
        }

        match page.next_continuation {
            Some(token)
                if report.objects_listed >= u64::from(config.scan.max_keys_per_invocation) =>
            {
                report.next_continuation_token = Some(token);
                break;
            }
            Some(token) if page_calls >= max_page_calls => {
                report.next_continuation_token = Some(token);
                break;
            }
            Some(token) => next_continuation = Some(token),
            None => {
                report.listing_complete = true;
                break;
            }
        }
    }

    report.elapsed_ms = started.elapsed().as_millis() as u64;
    metrics::counter!(
        "object_gc_orphan_reconcile_runs_total",
        "status" => report.status.as_str(),
        // Bounded label so dashboards can distinguish a budget-exhausted
        // (enqueue-cap / key-budget) run from one that covered the listing.
        "listing_complete" => if report.listing_complete { "true" } else { "false" },
    )
    .increment(1);
    metrics::counter!("object_gc_orphan_reconcile_enqueued_total").increment(report.enqueued);
    Ok(report)
}

/// Classify one listed key and, when it is a proven-aged unreferenced orphan,
/// enqueue a durable candidate insert-only. Every failure mode degrades into
/// a counter on the report; the per-key advisory lock is held from step (c)
/// through the enqueue and released when this function returns.
async fn reconcile_key(
    config: &OrphanReconciliationConfig,
    inspector: &dyn BlobInspector,
    locks: &dyn BlobLockProvider,
    metadata: &MetadataStore,
    blob: &ListedBlob,
    report: &mut OrphanReconcileReport,
) {
    let key = &blob.key;
    if !is_content_addressed_blob_key(key) {
        report.keys_invalid += 1;
        return;
    }
    if younger_than_min_age(blob.last_modified, config) {
        report.objects_too_recent += 1;
        return;
    }

    let lock = match locks.acquire(key).await {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            report.keys_invalid += 1;
            return;
        }
        Err(_) => {
            report.uncertain_db += 1;
            report.db_reachable = false;
            return;
        }
    };
    // Held until the end of this function: every check below happens under
    // the per-key advisory lock.
    let _lock = lock;

    // HeadObject re-check under the lock: the listed timestamp is only a
    // point-in-time observation, so a missing object or a changed
    // last_modified (the object was replaced after listing) is uncertainty,
    // never an enqueue decision. Comparison is at whole-second resolution,
    // the coarsest precision S3-compatible stores report via HeadObject
    // (RustFS lists with millisecond precision but heads with second
    // precision). A replacement is detected whenever its HeadObject
    // timestamp lands in a later wall-clock second than the listing; one
    // that lands within the SAME second as the listing is invisible here.
    // That residual window is negligible for content-addressed keys and is
    // backstopped by the GC worker, which re-validates references under the
    // same per-key lock before any delete.
    let head_last_modified = match inspector.head_last_modified(key).await {
        Ok(Some(last_modified)) => last_modified,
        Ok(None) => {
            report.skipped_missing += 1;
            return;
        }
        Err(_) => {
            report.uncertain_s3 += 1;
            return;
        }
    };
    if head_last_modified.timestamp() != blob.last_modified.timestamp() {
        report.uncertain_s3 += 1;
        return;
    }
    if younger_than_min_age(head_last_modified, config) {
        report.objects_too_recent += 1;
        return;
    }

    let references = match metadata.count_blob_references(key).await {
        Ok(references) => references,
        Err(_) => {
            report.uncertain_db += 1;
            report.db_reachable = false;
            return;
        }
    };
    if references.total() > 0 {
        report.skipped_referenced += 1;
        return;
    }

    match metadata
        .enqueue_object_gc_candidate_if_absent(
            key,
            ORPHAN_RECONCILE_REASON,
            config.grace_period_hours,
        )
        .await
    {
        Ok(true) => report.enqueued += 1,
        Ok(false) => match metadata.get_object_gc_candidate(key).await {
            Ok(Some(row)) if row.operator_hold || row.state == "operator_hold" => {
                report.skipped_operator_holds += 1
            }
            Ok(_) => report.skipped_existing_queue_rows += 1,
            Err(_) => {
                report.uncertain_db += 1;
                report.db_reachable = false;
            }
        },
        Err(_) => {
            report.uncertain_db += 1;
            report.db_reachable = false;
        }
    }
}

fn younger_than_min_age(last_modified: DateTime<Utc>, config: &OrphanReconciliationConfig) -> bool {
    let min_age =
        chrono::Duration::from_std(config.scan.min_object_age).unwrap_or(chrono::Duration::MAX);
    Utc::now().signed_duration_since(last_modified) < min_age
}

#[derive(Debug, Deserialize)]
pub struct OrphanReconcileRequest {
    pub continuation_token: Option<String>,
    pub limit: Option<u32>,
}

/// POST /api/admin/object-gc/orphan-reconcile
///
/// Admin-only producer for orphan GC candidates. Partial results are returned
/// with HTTP 200 and are visible through `status` / `listing_complete`; only
/// the fail-closed gates (disabled, versioning unsafe/unknown, invalid
/// configuration) produce error statuses. The response carries aggregate
/// counts only — never object contents or per-key detail.
pub async fn post_orphan_reconcile(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(request): Json<OrphanReconcileRequest>,
) -> Result<Json<OrphanReconcileReport>, AppError> {
    let mut config = OrphanReconciliationConfig::from_env().map_err(reconcile_error_response)?;
    if let Some(limit) = request.limit {
        // The request limit lowers the scan key budget, possibly below
        // `max_enqueue_per_run`; the enqueue cap then simply never trips
        // (harmless — the key budget is the binding limit this run).
        config.scan.max_keys_per_invocation = limit.clamp(1, MAX_KEYS_PER_INVOCATION_CAP);
    }

    let inspector: &dyn BlobInspector = state.object_store.as_ref();
    let locks: &dyn BlobLockProvider = state.object_store.as_ref();
    let report = reconcile_orphan_blobs(
        &config,
        inspector,
        locks,
        &state.metadata_store,
        request.continuation_token,
    )
    .await
    .map_err(reconcile_error_response)?;
    Ok(Json(report))
}

fn reconcile_error_response(error: ReconcileError) -> AppError {
    match error {
        ReconcileError::Disabled => AppError::forbidden("orphan reconciliation is disabled"),
        ReconcileError::VersioningUnsafe(state) => AppError::conflict(format!(
            "bucket versioning state {} is unsafe for orphan reconciliation",
            versioning_label(state)
        )),
        ReconcileError::VersioningUnknown => AppError::conflict(
            "bucket versioning state could not be determined; failing closed".to_string(),
        ),
        ReconcileError::InvalidConfig(message) => AppError::bad_request(message),
        ReconcileError::Probe(message) => AppError::service_unavailable(message),
    }
}

fn versioning_label(state: BucketVersioningState) -> &'static str {
    match state {
        BucketVersioningState::Unversioned => "unversioned",
        BucketVersioningState::Enabled => "enabled",
        BucketVersioningState::Suspended => "suspended",
        BucketVersioningState::Unknown => "unknown",
    }
}

fn bounded_error_text(error: &impl fmt::Display) -> String {
    let text = error.to_string();
    if text.len() <= MAX_RECONCILE_ERROR_BYTES {
        return text;
    }
    let mut end = MAX_RECONCILE_ERROR_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn scan(max_keys: u32, min_age: Duration) -> OrphanScanConfig {
        OrphanScanConfig::new(max_keys, min_age).expect("valid scan config")
    }

    #[test]
    fn config_validates_ranges_only_when_enabled() {
        // Enabled: out-of-range values are rejected.
        assert!(OrphanReconciliationConfig::new(true, 0, 24, scan(1000, Duration::ZERO)).is_err());
        assert!(OrphanReconciliationConfig::new(
            true,
            MAX_ENQUEUE_PER_RUN_CAP + 1,
            24,
            scan(1000, Duration::ZERO)
        )
        .is_err());
        assert!(OrphanReconciliationConfig::new(true, 100, 0, scan(1000, Duration::ZERO)).is_err());
        // The enqueue cap must not exceed the scan key budget.
        assert!(OrphanReconciliationConfig::new(true, 500, 24, scan(100, Duration::ZERO)).is_err());
        assert!(OrphanReconciliationConfig::new(true, 100, 24, scan(100, Duration::ZERO)).is_ok());
        assert!(OrphanReconciliationConfig::new(
            true,
            MAX_ENQUEUE_PER_RUN_CAP,
            1,
            scan(1000, Duration::ZERO)
        )
        .is_ok());
        // Disabled: leftover bogus values must not prevent startup.
        assert!(OrphanReconciliationConfig::new(false, 0, 0, scan(1000, Duration::ZERO)).is_ok());
    }

    #[test]
    fn from_env_parses_and_validates_only_when_enabled() {
        std::env::set_var("RUSTSHARE_ORPHAN_RECONCILIATION_ENABLED", "true");
        std::env::set_var("RUSTSHARE_ORPHAN_RECONCILE_MAX_ENQUEUE", "7");
        std::env::set_var("RUSTSHARE_OBJECT_GC_GRACE_PERIOD_HOURS", "3");
        std::env::set_var("RUSTSHARE_ORPHAN_SCAN_MAX_KEYS", "50");
        let config = OrphanReconciliationConfig::from_env().expect("env config");
        assert!(config.enabled);
        assert_eq!(config.max_enqueue_per_run, 7);
        assert_eq!(config.grace_period_hours, 3);
        assert_eq!(config.scan.max_keys_per_invocation, 50);

        std::env::set_var("RUSTSHARE_ORPHAN_RECONCILE_MAX_ENQUEUE", "0");
        assert!(
            OrphanReconciliationConfig::from_env().is_err(),
            "enabled + zero enqueue cap must be rejected"
        );
        std::env::set_var("RUSTSHARE_ORPHAN_RECONCILE_MAX_ENQUEUE", "7");
        std::env::set_var("RUSTSHARE_ORPHAN_RECONCILIATION_ENABLED", "false");
        std::env::set_var("RUSTSHARE_ORPHAN_SCAN_MAX_KEYS", "bogus");
        std::env::set_var("RUSTSHARE_ORPHAN_RECONCILE_MAX_ENQUEUE", "bogus");
        std::env::set_var("RUSTSHARE_OBJECT_GC_GRACE_PERIOD_HOURS", "bogus");
        let config = OrphanReconciliationConfig::from_env()
            .expect("disabled config must ignore leftover env");
        assert!(!config.enabled);
        assert_eq!(
            config.scan.max_keys_per_invocation, DEFAULT_MAX_KEYS_PER_INVOCATION,
            "a disabled reconciler must not parse (or validate) the scan sub-config"
        );
        assert_eq!(
            config.max_enqueue_per_run, DEFAULT_MAX_ENQUEUE_PER_RUN,
            "a disabled reconciler must not parse the enqueue cap"
        );
        assert_eq!(
            config.grace_period_hours, 24,
            "a disabled reconciler must not parse the grace period"
        );

        std::env::remove_var("RUSTSHARE_ORPHAN_RECONCILIATION_ENABLED");
        std::env::remove_var("RUSTSHARE_ORPHAN_RECONCILE_MAX_ENQUEUE");
        std::env::remove_var("RUSTSHARE_OBJECT_GC_GRACE_PERIOD_HOURS");
        std::env::remove_var("RUSTSHARE_ORPHAN_SCAN_MAX_KEYS");
    }

    #[test]
    fn report_serializes_counts_only() {
        let report = OrphanReconcileReport {
            status: ReconcileStatus::Completed,
            objects_listed: 1,
            enqueued: 1,
            db_reachable: true,
            listing_complete: true,
            versioning: "unversioned".to_string(),
            ..OrphanReconcileReport::default()
        };
        let json = serde_json::to_value(&report).expect("serialize report");
        assert_eq!(json["status"], "completed");
        assert_eq!(json["enqueued"], 1);
        assert_eq!(json["versioning"], "unversioned");
        for field in [
            "outcomes",
            "keys",
            "first_key",
            "last_key",
            "last_modified",
            "content",
        ] {
            assert!(json.get(field).is_none(), "report must not contain {field}");
        }
    }

    #[test]
    fn versioning_labels_cover_all_states() {
        assert_eq!(
            versioning_label(BucketVersioningState::Unversioned),
            "unversioned"
        );
        assert_eq!(versioning_label(BucketVersioningState::Enabled), "enabled");
        assert_eq!(
            versioning_label(BucketVersioningState::Suspended),
            "suspended"
        );
        assert_eq!(versioning_label(BucketVersioningState::Unknown), "unknown");
    }

    #[test]
    fn reconcile_error_maps_to_http_status() {
        use axum::http::StatusCode;
        use axum::response::IntoResponse;

        let status =
            |error: ReconcileError| reconcile_error_response(error).into_response().status();

        assert_eq!(status(ReconcileError::Disabled), StatusCode::FORBIDDEN);
        for state in [
            BucketVersioningState::Enabled,
            BucketVersioningState::Suspended,
            BucketVersioningState::Unknown,
        ] {
            assert_eq!(
                status(ReconcileError::VersioningUnsafe(state)),
                StatusCode::CONFLICT,
                "an unsafe versioning state must fail closed with 409"
            );
        }
        assert_eq!(
            status(ReconcileError::VersioningUnknown),
            StatusCode::CONFLICT
        );
        assert_eq!(
            status(ReconcileError::InvalidConfig("bogus".to_string())),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status(ReconcileError::Probe("transport".to_string())),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn bounded_error_text_does_not_split_utf8() {
        let error = anyhow::anyhow!("{}", "é".repeat(600));
        assert!(bounded_error_text(&error).len() <= MAX_RECONCILE_ERROR_BYTES);
    }
}
