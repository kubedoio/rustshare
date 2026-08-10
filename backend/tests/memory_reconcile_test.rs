//! Integration tests for the Memory chat reconciliation repair path
//! ([`rustshare_server::handlers::memory_reconcile::reconcile_chat_memory_for_tenant`]):
//! the admin reconcile endpoint re-projects the tenant's `memory_catalog`
//! from the signature-verified observation index (`chat_observed_events`) —
//! no outbox replay, no private Buzz database access, no receipts.
//!
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied, including `20260810000004`) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test --test memory_reconcile_test -- --ignored --test-threads=1
//!
//! The chat tables are tenant-scoped, so every test takes a shared `SERIAL`
//! guard and cleans up exactly the rows it created (same convention as the
//! memory-projection and outbox suites).

use chrono::{DateTime, Timelike, Utc};
use rustshare_core::domain::{PrincipalId, TenantId};
use rustshare_memory::event::ObservedEventType;
use rustshare_server::handlers::memory_reconcile::reconcile_chat_memory_for_tenant;
use rustshare_storage::{
    ChatIdentityStore, ChatObservationStore, MemoryCatalogStore, ReconcileCounts,
};
use sqlx::{PgPool, Row};
use std::sync::LazyLock;
use uuid::Uuid;

/// Serializes the tests within this binary (same convention as the outbox and
/// chat-observation suites).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

const COMMUNITY_ID: &str = "community-reconcile";
const CHANNEL_ID: &str = "channel-1";

/// Shared pool over `DATABASE_URL` with the same fallback the storage-layer
/// tests use.
async fn pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("failed to connect to the dev database")
}

fn hex64(seed: u8) -> String {
    format!("{seed:02x}").repeat(32)
}

/// Remove every row the tests create for the tenant.
async fn cleanup(pool: &PgPool, tenant_id: TenantId) {
    for table in [
        "memory_catalog",
        "chat_observed_events",
        "chat_buzz_admissions",
        "chat_workspace_communities",
        "chat_identity_bindings",
        "application_enablements",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE tenant_id = $1"))
            .bind(tenant_id.0)
            .execute(pool)
            .await
            .unwrap();
    }
}

/// Full tenant setup: active community mapping, active binding, active
/// admission, and the chat Application enabled with `configuration`. Returns
/// the bound principal id (mirrored onto the observation rows).
async fn setup_tenant(
    pool: &PgPool,
    tenant_id: TenantId,
    configuration: serde_json::Value,
) -> PrincipalId {
    let mapping_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_workspace_communities
            (mapping_id, tenant_id, workspace_id, community_id, relay_url, active)
         VALUES ($1, $2, $3, $4, $5, true)",
    )
    .bind(mapping_id)
    .bind(tenant_id.0)
    .bind(tenant_id.0)
    .bind(COMMUNITY_ID)
    .bind("wss://relay.example.test")
    .execute(pool)
    .await
    .unwrap();

    let binding_id = Uuid::new_v4();
    let principal_id = PrincipalId::from(Uuid::new_v4());
    sqlx::query(
        "INSERT INTO chat_identity_bindings
            (binding_id, tenant_id, principal_id, buzz_pubkey, status, verified_at, audit_metadata)
         VALUES ($1, $2, $3, $4, 'active', now(), '{}'::jsonb)",
    )
    .bind(binding_id)
    .bind(tenant_id.0)
    .bind(principal_id.0)
    .bind(hex64(0xbb))
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO chat_buzz_admissions
            (admission_id, tenant_id, mapping_id, binding_id, buzz_pubkey, active)
         VALUES ($1, $2, $3, $4, $5, true)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id.0)
    .bind(mapping_id)
    .bind(binding_id)
    .bind(hex64(0xbb))
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO application_enablements
            (tenant_id, workspace_id, application_id, enabled, configuration)
         VALUES ($1, $2, 'io.elembra.chat', true, $3)",
    )
    .bind(tenant_id.0)
    .bind(tenant_id.0)
    .bind(configuration)
    .execute(pool)
    .await
    .unwrap();

    principal_id
}

/// Insert one observation row directly (workspace == tenant per the platform
/// invariant). The reconcile path reads ONLY this index — no buzz push, no
/// outbox, no receipts.
#[allow(clippy::too_many_arguments)]
async fn insert_observation(
    pool: &PgPool,
    tenant_id: TenantId,
    principal_id: PrincipalId,
    event_id: &str,
    message_id: &str,
    event_type: ObservedEventType,
    created_at: DateTime<Utc>,
    body: Option<&str>,
) {
    let supersedes_event_id =
        (event_type != ObservedEventType::Created).then(|| message_id.to_string());
    let active = event_type != ObservedEventType::Deleted;
    let observed_at = created_at + chrono::Duration::seconds(1);
    sqlx::query(
        "INSERT INTO chat_observed_events
            (tenant_id, workspace_id, event_id, message_id, event_type,
             supersedes_event_id, community_id, channel_id, channel_kind,
             thread_root_id, author_pubkey, author_principal_id,
             event_created_at, observed_at, checksum, signature,
             signature_verified, body, active)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'workspace', NULL, $9, $10,
                 $11, $12, $13, $14, true, $15, $16)",
    )
    .bind(tenant_id.0)
    .bind(tenant_id.0)
    .bind(event_id)
    .bind(message_id)
    .bind(event_type_str(event_type))
    .bind(supersedes_event_id)
    .bind(COMMUNITY_ID)
    .bind(CHANNEL_ID)
    .bind(hex64(0xbb))
    .bind(principal_id.0)
    .bind(created_at)
    .bind(observed_at)
    .bind(format!("sha256:{event_id}"))
    .bind("c".repeat(128))
    .bind(body)
    .bind(active)
    .execute(pool)
    .await
    .unwrap();
}

fn event_type_str(event_type: ObservedEventType) -> &'static str {
    match event_type {
        ObservedEventType::Created => "created",
        ObservedEventType::Edited => "edited",
        ObservedEventType::Deleted => "deleted",
    }
}

/// Run the reconcile orchestration over the shared stores.
async fn reconcile(
    pool: &PgPool,
    tenant_id: TenantId,
    since: Option<DateTime<Utc>>,
) -> ReconcileCounts {
    reconcile_chat_memory_for_tenant(
        &ChatIdentityStore::new(pool.clone()),
        &ChatObservationStore::new(pool.clone()),
        &MemoryCatalogStore::new(pool.clone()),
        tenant_id,
        since,
    )
    .await
    .expect("reconcile must succeed")
}

async fn catalog_count(pool: &PgPool, tenant_id: TenantId) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM memory_catalog WHERE tenant_id = $1")
        .bind(tenant_id.0)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn catalog_row(pool: &PgPool, tenant_id: TenantId) -> Option<sqlx::postgres::PgRow> {
    sqlx::query(
        "SELECT record_id, message_id, latest_event_id, event_type, indexing_status, provenance
         FROM memory_catalog WHERE tenant_id = $1",
    )
    .bind(tenant_id.0)
    .fetch_optional(pool)
    .await
    .expect("catalog lookup must succeed")
}

// ---------------------------------------------------------------------------
// 1. Missing record is rebuilt; re-runs are idempotent (no duplicates)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn reconcile_repairs_missing_record() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let principal = setup_tenant(
        &pool,
        tenant,
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    // Message M: created then edited, bodies NULL (content_indexing off).
    let message_id = hex64(0xaa);
    let created_event = hex64(0xaa); // the created event IS the message id
    let edited_event = hex64(0xee);
    let t0 = Utc::now();
    insert_observation(
        &pool,
        tenant,
        principal,
        &created_event,
        &message_id,
        ObservedEventType::Created,
        t0,
        None,
    )
    .await;
    insert_observation(
        &pool,
        tenant,
        principal,
        &edited_event,
        &message_id,
        ObservedEventType::Edited,
        t0 + chrono::Duration::seconds(10),
        None,
    )
    .await;

    let counts = reconcile(&pool, tenant, None).await;
    assert_eq!(counts.processed, 2, "both observation rows are examined");
    assert_eq!(counts.created, 1, "exactly one record is inserted");
    assert_eq!(counts.updated, 0);
    assert_eq!(catalog_count(&pool, tenant).await, 1);
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("message_id"), message_id);
    assert_eq!(
        row.get::<String, _>("latest_event_id"),
        edited_event,
        "the record mirrors the latest (edited) event"
    );
    assert_eq!(row.get::<String, _>("event_type"), "edited");
    let provenance: serde_json::Value = row.get("provenance");
    assert_eq!(
        provenance.as_array().expect("provenance is an array").len(),
        2,
        "created + edited ⇒ two provenance entries"
    );

    // Simulate corruption: drop the catalog record, reconcile again.
    sqlx::query("DELETE FROM memory_catalog WHERE tenant_id = $1")
        .bind(tenant.0)
        .execute(&pool)
        .await
        .unwrap();
    let counts = reconcile(&pool, tenant, None).await;
    assert_eq!(counts.created, 1, "the deleted record is rebuilt");
    assert_eq!(counts.updated, 0);
    assert_eq!(catalog_count(&pool, tenant).await, 1);

    // Third run: idempotent — still exactly one record, no duplicates.
    let counts = reconcile(&pool, tenant, None).await;
    assert_eq!(counts.processed, 2);
    assert_eq!(
        catalog_count(&pool, tenant).await,
        1,
        "no duplicate records"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 2. `since` watermark limits the rebuild to later observations
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn reconcile_respects_since() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let principal = setup_tenant(
        &pool,
        tenant,
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    let t1 = (Utc::now() - chrono::Duration::seconds(1))
        .with_nanosecond(0)
        .unwrap();
    let t2 = t1 + chrono::Duration::seconds(10);
    insert_observation(
        &pool,
        tenant,
        principal,
        &hex64(0xaa),
        &hex64(0xaa),
        ObservedEventType::Created,
        t1,
        None,
    )
    .await;
    insert_observation(
        &pool,
        tenant,
        principal,
        &hex64(0xbb),
        &hex64(0xbb),
        ObservedEventType::Created,
        t2,
        None,
    )
    .await;

    // Watermark strictly between the two events: only the later message is
    // rebuilt.
    let counts = reconcile(&pool, tenant, Some(t1 + chrono::Duration::microseconds(1))).await;
    assert_eq!(counts.processed, 1, "the earlier observation is excluded");
    assert_eq!(counts.created, 1);
    assert_eq!(catalog_count(&pool, tenant).await, 1);
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("message_id"), hex64(0xbb));

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 3. Projection disabled ⇒ rows examined, nothing written
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn reconcile_skips_when_projection_disabled() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    // `{}`: memory_projection absent ⇒ false (fail closed).
    let principal = setup_tenant(&pool, tenant, serde_json::json!({})).await;

    insert_observation(
        &pool,
        tenant,
        principal,
        &hex64(0xaa),
        &hex64(0xaa),
        ObservedEventType::Created,
        Utc::now(),
        None,
    )
    .await;

    let counts = reconcile(&pool, tenant, None).await;
    assert_eq!(counts.processed, 1, "the rows are still counted");
    assert_eq!(counts.created, 0);
    assert_eq!(counts.updated, 0);
    assert_eq!(
        catalog_count(&pool, tenant).await,
        0,
        "a disabled policy must not project"
    );

    cleanup(&pool, tenant).await;
}

// ---------------------------------------------------------------------------
// 4. Tombstones are re-projected: record marked, provenance survives
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn reconcile_tombstones_marked() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let tenant = TenantId::from(Uuid::new_v4());
    let principal = setup_tenant(
        &pool,
        tenant,
        serde_json::json!({ "memory_projection": true }),
    )
    .await;

    let t0 = Utc::now();
    insert_observation(
        &pool,
        tenant,
        principal,
        &hex64(0xaa),
        &hex64(0xaa),
        ObservedEventType::Created,
        t0,
        None,
    )
    .await;
    insert_observation(
        &pool,
        tenant,
        principal,
        &hex64(0xdd),
        &hex64(0xaa),
        ObservedEventType::Deleted,
        t0 + chrono::Duration::seconds(5),
        None,
    )
    .await;

    let counts = reconcile(&pool, tenant, None).await;
    assert_eq!(counts.processed, 2);
    assert_eq!(counts.created, 1);
    assert_eq!(catalog_count(&pool, tenant).await, 1);
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("event_type"), "deleted");
    assert_eq!(row.get::<String, _>("indexing_status"), "tombstoned");
    assert_eq!(row.get::<String, _>("latest_event_id"), hex64(0xdd));

    cleanup(&pool, tenant).await;
}
