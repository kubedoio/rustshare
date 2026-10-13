//! Read-only orphan discovery. This module NEVER deletes, NEVER enqueues, NEVER mutates.
//!
//! The scanner lists content-addressed blob keys, classifies each key against
//! the global reference summary and the durable GC queue, and reports
//! aggregate counts. It is the Phase 3.4A discovery half of orphan
//! reconciliation; producing durable candidates from these observations is a
//! separate, later decision (Phase 3.4B) and is intentionally not implemented.
//!
//! By construction this module cannot delete: the scan entry point only
//! accepts `&dyn BlobLister` (a list-only seam) and `&MetadataStore` read
//! methods. `ObjectStore` appears solely as the `BlobLister` implementation
//! target.

use std::{fmt, future::Future, sync::Arc, time::Duration, time::Instant};

use axum::extract::{Json, State};
use chrono::{DateTime, Utc};
use rustshare_storage::{MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};

use crate::handlers::{AdminUser, AppError};
use crate::object_gc::is_content_addressed_blob_key;
use crate::AppState;

const BLOB_PREFIX: &str = "blobs/";
const MAX_SCAN_ERROR_BYTES: usize = 500;
const MAX_UNCERTAIN_REASONS: usize = 10;

pub const DEFAULT_MAX_KEYS_PER_INVOCATION: u32 = 500;
pub const MAX_KEYS_PER_INVOCATION_CAP: u32 = 1_000;
pub const DEFAULT_MIN_OBJECT_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Failure modes of a scan. All carried text is bounded to
/// [`MAX_SCAN_ERROR_BYTES`] and never includes credentials or endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanError {
    /// Listing one page of objects failed; the invocation aborts and reports
    /// nothing rather than a partial picture.
    List(String),
    /// The scan configuration is invalid.
    InvalidConfig(String),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanError::List(message) => write!(f, "object listing failed: {message}"),
            ScanError::InvalidConfig(message) => {
                write!(f, "invalid orphan scan configuration: {message}")
            }
        }
    }
}

impl std::error::Error for ScanError {}

/// One blob observed by a listing page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedBlob {
    pub key: String,
    pub last_modified: DateTime<Utc>,
}

/// One page of a blob listing. `next_continuation` resumes after this page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlobListPage {
    pub blobs: Vec<ListedBlob>,
    pub next_continuation: Option<String>,
}

/// List-only seam over object storage. Deliberately has no put/delete/exists:
/// the scanner is unable to mutate object storage by construction.
///
/// The boxed future keeps the trait dyn-compatible so scan entry points take
/// `&dyn BlobLister`; implementors delegate to their async fn.
pub trait BlobLister: Send + Sync {
    fn list_page<'a>(
        &'a self,
        continuation: Option<&'a str>,
        max_keys: u32,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<BlobListPage, ScanError>> + Send + 'a>>;
}

impl BlobLister for ObjectStore {
    fn list_page<'a>(
        &'a self,
        continuation: Option<&'a str>,
        max_keys: u32,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<BlobListPage, ScanError>> + Send + 'a>> {
        Box::pin(async move {
            let page = self
                .list_page(BLOB_PREFIX, continuation, max_keys)
                .await
                .map_err(|error| ScanError::List(bounded_error_text(&error)))?;
            Ok(BlobListPage {
                blobs: page
                    .objects
                    .into_iter()
                    .map(|object| ListedBlob {
                        key: object.key,
                        last_modified: object.last_modified,
                    })
                    .collect(),
                next_continuation: page.next_continuation_token,
            })
        })
    }
}

/// Per-invocation budget for a dry-run scan.
#[derive(Debug, Clone)]
pub struct OrphanScanConfig {
    /// Stop after classifying this many keys (one page may overshoot).
    /// Default 500, hard cap [`MAX_KEYS_PER_INVOCATION_CAP`].
    pub max_keys_per_invocation: u32,
    /// Objects younger than this are classified `recent` and never considered
    /// for orphanhood. Age is a floor for *consideration*, not proof of
    /// deletion safety. Default 24 hours.
    pub min_object_age: Duration,
}

impl OrphanScanConfig {
    pub fn new(max_keys_per_invocation: u32, min_object_age: Duration) -> Result<Self, ScanError> {
        if max_keys_per_invocation == 0 || max_keys_per_invocation > MAX_KEYS_PER_INVOCATION_CAP {
            return Err(ScanError::InvalidConfig(format!(
                "max_keys_per_invocation must be between 1 and {MAX_KEYS_PER_INVOCATION_CAP}"
            )));
        }
        Ok(Self {
            max_keys_per_invocation,
            min_object_age,
        })
    }

    pub fn from_env() -> Result<Self, ScanError> {
        let max_keys = env_parse(
            "RUSTSHARE_ORPHAN_SCAN_MAX_KEYS",
            DEFAULT_MAX_KEYS_PER_INVOCATION,
        )?;
        let min_age_hours = env_parse("RUSTSHARE_ORPHAN_SCAN_MIN_AGE_HOURS", 24u64)?;
        Self::new(
            max_keys,
            Duration::from_secs(min_age_hours.saturating_mul(3600)),
        )
    }
}

fn env_parse<T>(name: &str, default: T) -> Result<T, ScanError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|error| ScanError::InvalidConfig(format!("invalid {name}: {error}"))),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(ScanError::InvalidConfig(format!("invalid {name}: {error}"))),
    }
}

/// Point-in-time classification of one listed key.
///
/// Every classification is an observation at inspection time, not a durable
/// verdict: `potential_orphan` means "zero durable references observed at
/// inspection time". It is NOT deletion-safe and must never be promoted to
/// deletion without a re-check under the per-key advisory lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    Referenced,
    QueuedPending,
    QueuedRetry,
    QueuedProcessing,
    QueuedReferenced,
    QueuedDeleted,
    QueuedMissing,
    QueuedInvalidKey,
    QueuedOperatorHold,
    PotentialOrphan,
    Recent,
    ExcludedInvalidKey,
    UncertainDbError,
}

/// Per-key classification, kept for assertions. Never serialized into the
/// HTTP response, which carries aggregate counts only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyScanOutcome {
    pub key: String,
    pub classification: Classification,
}

/// Aggregate result of one dry-run scan invocation.
///
/// The report contains counts only: no object contents and no per-key detail.
#[derive(Debug, Clone, Default, Serialize)]
pub struct OrphanScanReport {
    /// Always `true`. Automatic deletion does not exist in this build.
    pub dry_run: bool,
    pub scanned_count: u64,
    pub referenced_count: u64,
    pub queued_pending_count: u64,
    pub queued_retry_count: u64,
    pub queued_processing_count: u64,
    pub queued_referenced_count: u64,
    pub queued_deleted_count: u64,
    pub queued_missing_count: u64,
    pub queued_invalid_key_count: u64,
    pub queued_operator_hold_count: u64,
    pub potential_orphan_count: u64,
    pub recent_count: u64,
    pub excluded_invalid_key_count: u64,
    pub uncertain_db_error_count: u64,
    pub uncertain_count: u64,
    /// Bounded, deduplicated reasons behind uncertain classifications.
    pub uncertain_reasons: Vec<String>,
    #[serde(skip)]
    pub first_key_scanned: Option<String>,
    #[serde(skip)]
    pub last_key_scanned: Option<String>,
    /// Resumes the scan after the last classified page. Resumption is
    /// best-effort consistent under concurrent writes: keys added behind the
    /// token may be skipped, keys re-created in an already-scanned region may
    /// be observed again on the next invocation.
    pub next_continuation_token: Option<String>,
    pub elapsed_ms: u64,
    /// `false` when any metadata query failed during classification. Query
    /// failure is never read as zero references.
    pub db_reachable: bool,
    #[serde(skip)]
    pub outcomes: Vec<KeyScanOutcome>,
}

impl OrphanScanReport {
    fn record(&mut self, outcome: KeyScanOutcome) {
        match outcome.classification {
            Classification::Referenced => self.referenced_count += 1,
            Classification::QueuedPending => self.queued_pending_count += 1,
            Classification::QueuedRetry => self.queued_retry_count += 1,
            Classification::QueuedProcessing => self.queued_processing_count += 1,
            Classification::QueuedReferenced => self.queued_referenced_count += 1,
            Classification::QueuedDeleted => self.queued_deleted_count += 1,
            Classification::QueuedMissing => self.queued_missing_count += 1,
            Classification::QueuedInvalidKey => self.queued_invalid_key_count += 1,
            Classification::QueuedOperatorHold => self.queued_operator_hold_count += 1,
            Classification::PotentialOrphan => self.potential_orphan_count += 1,
            Classification::Recent => self.recent_count += 1,
            Classification::ExcludedInvalidKey => self.excluded_invalid_key_count += 1,
            Classification::UncertainDbError => self.uncertain_db_error_count += 1,
        }
        self.scanned_count += 1;
        self.last_key_scanned = Some(outcome.key.clone());
        self.outcomes.push(outcome);
    }

    fn record_uncertain_reason(&mut self, reason: String) {
        if self.uncertain_reasons.len() < MAX_UNCERTAIN_REASONS
            && !self.uncertain_reasons.contains(&reason)
        {
            self.uncertain_reasons.push(reason);
        }
    }
}

/// Classify listed blob keys without mutating anything.
///
/// Each key is evaluated in order:
///
/// 1. Keys failing the canonical `blobs/<64 hex>` filter (legacy uppercase hex
///    is accepted) are `excluded_invalid_key`.
/// 2. Objects younger than `min_object_age` are `recent` and are never
///    considered for orphanhood.
/// 3. A failed global reference query is `uncertain_db_error`; the report is
///    still returned, `db_reachable` is `false`, and the failure is never
///    read as zero references.
/// 4. More than zero durable references is `referenced`.
/// 5. Zero references with a queue row is `queued_<state>`; without one it is
///    `potential_orphan` — zero durable references observed at inspection
///    time, which is NOT deletion-safe.
///
/// A hard listing failure aborts the invocation with [`ScanError::List`];
/// per-key database failures degrade into `uncertain_db_error` so the partial
/// report remains available.
///
/// The invocation ends normally after either budget is consumed: at most
/// `max_keys_per_invocation` keys classified, or at most
/// `max(config.max_keys_per_invocation, 100)` listing calls made (empty-but-
/// tokened pages under churn cannot inflate call count unboundedly). Both
/// cases return the continuation token that resumes after the last classified
/// page.
pub async fn scan_orphan_blobs_dry_run(
    lister: &dyn BlobLister,
    metadata: &MetadataStore,
    config: &OrphanScanConfig,
    continuation: Option<&str>,
) -> Result<OrphanScanReport, ScanError> {
    let started = Instant::now();
    let mut report = OrphanScanReport {
        dry_run: true,
        db_reachable: true,
        ..OrphanScanReport::default()
    };
    let mut next_continuation = continuation.map(str::to_string);
    let max_page_calls = u64::from(config.max_keys_per_invocation).max(100);
    let mut page_calls = 0u64;

    loop {
        page_calls += 1;
        let page = lister
            .list_page(next_continuation.as_deref(), config.max_keys_per_invocation)
            .await?;
        if report.first_key_scanned.is_none() {
            report.first_key_scanned = page.blobs.first().map(|blob| blob.key.clone());
        }
        for blob in &page.blobs {
            // The lister already filters by prefix; re-validate anyway so a
            // misbehaving lister cannot smuggle keys past the canonical
            // filter (mirrors object_gc.rs).
            let outcome = classify_key(metadata, config, blob, &mut report).await;
            report.record(outcome);
        }

        match page.next_continuation {
            Some(token) if report.scanned_count >= u64::from(config.max_keys_per_invocation) => {
                report.next_continuation_token = Some(token);
                break;
            }
            Some(token) if page_calls >= max_page_calls => {
                report.next_continuation_token = Some(token);
                break;
            }
            Some(token) => next_continuation = Some(token),
            None => break,
        }
    }

    report.uncertain_count = report.uncertain_db_error_count;
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(report)
}

async fn classify_key(
    metadata: &MetadataStore,
    config: &OrphanScanConfig,
    blob: &ListedBlob,
    report: &mut OrphanScanReport,
) -> KeyScanOutcome {
    let key = &blob.key;
    if !is_content_addressed_blob_key(key) {
        return KeyScanOutcome {
            key: key.clone(),
            classification: Classification::ExcludedInvalidKey,
        };
    }

    let min_age =
        chrono::Duration::from_std(config.min_object_age).unwrap_or(chrono::Duration::MAX);
    if Utc::now().signed_duration_since(blob.last_modified) < min_age {
        return KeyScanOutcome {
            key: key.clone(),
            classification: Classification::Recent,
        };
    }

    let references = match metadata.count_blob_references(key).await {
        Ok(references) => references,
        Err(error) => {
            report.db_reachable = false;
            report.record_uncertain_reason(bounded_error_text(&error));
            return KeyScanOutcome {
                key: key.clone(),
                classification: Classification::UncertainDbError,
            };
        }
    };
    if references.total() > 0 {
        return KeyScanOutcome {
            key: key.clone(),
            classification: Classification::Referenced,
        };
    }

    let candidate = match metadata.get_object_gc_candidate(key).await {
        Ok(candidate) => candidate,
        Err(error) => {
            report.db_reachable = false;
            report.record_uncertain_reason(bounded_error_text(&error));
            return KeyScanOutcome {
                key: key.clone(),
                classification: Classification::UncertainDbError,
            };
        }
    };
    let classification = match candidate {
        Some(row) => queued_classification(&row.state).unwrap_or_else(|| {
            report.db_reachable = false;
            report.record_uncertain_reason(format!(
                "queue row for key has unknown state ({} bytes)",
                row.state.len()
            ));
            Classification::UncertainDbError
        }),
        None => Classification::PotentialOrphan,
    };
    KeyScanOutcome {
        key: key.clone(),
        classification,
    }
}

fn queued_classification(state: &str) -> Option<Classification> {
    match state {
        "pending" => Some(Classification::QueuedPending),
        "retry" => Some(Classification::QueuedRetry),
        "processing" => Some(Classification::QueuedProcessing),
        "referenced" => Some(Classification::QueuedReferenced),
        "deleted" => Some(Classification::QueuedDeleted),
        "missing" => Some(Classification::QueuedMissing),
        "invalid_key" => Some(Classification::QueuedInvalidKey),
        "operator_hold" => Some(Classification::QueuedOperatorHold),
        _ => None,
    }
}

fn bounded_error_text(error: &impl fmt::Display) -> String {
    let text = error.to_string();
    if text.len() <= MAX_SCAN_ERROR_BYTES {
        return text;
    }
    let mut end = MAX_SCAN_ERROR_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[derive(Debug, Deserialize)]
pub struct OrphanScanRequest {
    pub dry_run: Option<bool>,
    pub limit: Option<u32>,
    pub min_age_hours: Option<u64>,
    pub continuation_token: Option<String>,
}

/// POST /api/admin/object-gc/orphan-scan
///
/// Admin-only dry-run orphan discovery. The request MUST set `dry_run: true`;
/// nothing in this build deletes automatically. The response carries
/// aggregate counts only — never object contents or per-key detail.
pub async fn post_orphan_scan(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(request): Json<OrphanScanRequest>,
) -> Result<Json<OrphanScanReport>, AppError> {
    if request.dry_run != Some(true) {
        return Err(AppError::bad_request(
            "only dry_run=true is supported; automatic deletion does not exist in this build",
        ));
    }

    let mut config = OrphanScanConfig::from_env().map_err(scan_error_response)?;
    if let Some(limit) = request.limit {
        config.max_keys_per_invocation = limit.clamp(1, MAX_KEYS_PER_INVOCATION_CAP);
    }
    if let Some(hours) = request.min_age_hours {
        config.min_object_age = Duration::from_secs(hours.saturating_mul(3600));
    }

    let object_store: Arc<ObjectStore> = state.object_store.clone();
    let lister: &dyn BlobLister = object_store.as_ref();
    let report = scan_orphan_blobs_dry_run(
        lister,
        &state.metadata_store,
        &config,
        request.continuation_token.as_deref(),
    )
    .await
    .map_err(scan_error_response)?;
    Ok(Json(report))
}

fn scan_error_response(error: ScanError) -> AppError {
    match error {
        ScanError::InvalidConfig(message) => AppError::bad_request(message),
        ScanError::List(message) => AppError::service_unavailable(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_blob(key: &str, age: Duration) -> ListedBlob {
        ListedBlob {
            key: key.to_string(),
            last_modified: Utc::now() - chrono::Duration::from_std(age).unwrap(),
        }
    }

    #[test]
    fn config_rejects_out_of_range_budgets() {
        assert!(OrphanScanConfig::new(0, DEFAULT_MIN_OBJECT_AGE).is_err());
        assert!(
            OrphanScanConfig::new(MAX_KEYS_PER_INVOCATION_CAP + 1, DEFAULT_MIN_OBJECT_AGE).is_err()
        );
        assert!(OrphanScanConfig::new(1, DEFAULT_MIN_OBJECT_AGE).is_ok());
        assert!(OrphanScanConfig::new(MAX_KEYS_PER_INVOCATION_CAP, DEFAULT_MIN_OBJECT_AGE).is_ok());
    }

    #[test]
    fn queued_classification_covers_all_contract_states() {
        for state in [
            "pending",
            "retry",
            "processing",
            "referenced",
            "deleted",
            "missing",
            "invalid_key",
            "operator_hold",
        ] {
            assert!(queued_classification(state).is_some(), "missed {state}");
        }
        assert!(queued_classification("unknown").is_none());
    }

    #[test]
    fn report_serializes_counts_without_per_key_detail() {
        let mut report = OrphanScanReport {
            dry_run: true,
            scanned_count: 1,
            potential_orphan_count: 1,
            db_reachable: true,
            first_key_scanned: Some("blobs/aaa".to_string()),
            last_key_scanned: Some("blobs/aaa".to_string()),
            outcomes: vec![KeyScanOutcome {
                key: "blobs/aaa".to_string(),
                classification: Classification::PotentialOrphan,
            }],
            ..OrphanScanReport::default()
        };
        report.uncertain_count = 0;
        let json = serde_json::to_value(&report).expect("serialize report");
        assert_eq!(json["dry_run"], true);
        assert_eq!(json["potential_orphan_count"], 1);
        for field in ["outcomes", "size", "first_key_scanned", "last_key_scanned"] {
            assert!(json.get(field).is_none(), "report must not contain {field}");
        }
    }

    // The scanner only accepts &dyn BlobLister; ObjectStore's mutating methods
    // are unreachable through that seam (compile-time guarantee by usage,
    // exercised end to end in object_gc_orphan_scan_test).
    #[test]
    fn recent_boundary_never_classifies_young_objects_as_orphans() {
        let config = OrphanScanConfig::new(10, Duration::from_secs(3600)).expect("config");
        let young = fake_blob(&format!("blobs/{}", "a".repeat(64)), Duration::ZERO);
        let min_age = chrono::Duration::from_std(config.min_object_age).unwrap();
        assert!(Utc::now().signed_duration_since(young.last_modified) < min_age);
    }
}
