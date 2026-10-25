//! Integration tests for Memory Chat reconciliation and Files-backed Notes
//! index recovery. Chat repair re-projects the tenant's `memory_catalog` from
//! verified `chat_observed_events`; Files recovery rebuilds the existing note
//! vector index from current source rows and OKF content.
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
    ChatIdentityStore, ChatObservationStore, MemoryCatalogStore, MetadataStore, ReconcileCounts,
};
use sqlx::{PgPool, Row};
use std::sync::LazyLock;
use uuid::Uuid;

mod support;

/// Serializes the tests within this binary (same convention as the outbox and
/// chat-observation suites).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

#[tokio::test]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn files_memory_reconciliation_pages_authoritative_rows_and_tombstones() {
    use support::calendar_harness::{
        cleanup_tenant, create_test_tenant, create_test_user, setup_test_env,
    };

    let _guard = SERIAL.lock().await;
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let foreign_tenant_id = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "memory_reconcile_files", tenant_id).await;
    let unavailable_owner =
        create_test_user(&state, "memory_reconcile_files_unavailable", tenant_id).await;
    let foreign_owner =
        create_test_user(&state, "memory_reconcile_files_foreign", foreign_tenant_id).await;
    let metadata = MetadataStore::new(state.db_pool.clone());
    let base = Uuid::new_v4().as_u128() & !0xff;
    let mut ids = Vec::new();
    for (index, (user_id, target_tenant)) in [
        (owner.id, tenant_id),
        (owner.id, tenant_id),
        (unavailable_owner.id, tenant_id),
        (foreign_owner.id, foreign_tenant_id),
    ]
    .into_iter()
    .enumerate()
    {
        let id = Uuid::from_u128(base + index as u128 + 1);
        let path = format!("/memory-reconcile-{id}.md");
        let mut file = rustshare_core::domain::File::new(
            format!("memory-reconcile-{id}.md"),
            path,
            format!("hash-{id}"),
            1,
            "text/markdown".to_string(),
            None,
            user_id,
            target_tenant,
        );
        file.id = id;
        metadata.create_file(&file).await.unwrap();
        ids.push(id);
    }
    metadata.delete_file(ids[1], owner.id).await.unwrap();
    sqlx::query("UPDATE users SET disabled_at = NOW() WHERE id = $1")
        .bind(unavailable_owner.id)
        .execute(&state.db_pool)
        .await
        .unwrap();

    let first = metadata
        .list_files_for_memory_reconciliation(tenant_id, None, 2)
        .await
        .unwrap();
    let retried_first = metadata
        .list_files_for_memory_reconciliation(tenant_id, None, 2)
        .await
        .unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(
        first.iter().map(|entry| entry.file.id).collect::<Vec<_>>(),
        retried_first
            .iter()
            .map(|entry| entry.file.id)
            .collect::<Vec<_>>()
    );
    assert!(first.iter().all(|entry| entry.file.tenant_id == tenant_id));
    assert!(first.iter().all(|entry| entry.owner_available));
    let second = metadata
        .list_files_for_memory_reconciliation(tenant_id, Some(first[1].file.id), 2)
        .await
        .unwrap();
    assert_eq!(second.len(), 1);
    assert!(second.iter().all(|entry| entry.file.tenant_id == tenant_id));
    assert!(first
        .iter()
        .chain(second.iter())
        .any(|entry| entry.file.deleted_at.is_some()));
    let (tombstone_lock, tombstone_is_active) = metadata
        .lock_file_for_memory_reconciliation(tenant_id, ids[1])
        .await
        .unwrap()
        .expect("deleted source row should remain lockable for index cleanup");
    assert!(!tombstone_is_active);
    tombstone_lock.commit().await.unwrap();
    assert_eq!(
        first
            .iter()
            .chain(second.iter())
            .filter(|entry| !entry.owner_available)
            .count(),
        1
    );

    let foreign = metadata
        .list_files_for_memory_reconciliation(foreign_tenant_id, None, 10)
        .await
        .unwrap();
    assert_eq!(foreign.len(), 1);
    assert_eq!(foreign[0].file.owner_id, foreign_owner.id);

    cleanup_tenant(&state.db_pool, tenant_id).await;
    cleanup_tenant(&state.db_pool, foreign_tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL, object storage, and migrations applied"]
async fn files_memory_reconciliation_repairs_a_missed_index_and_deleted_source() {
    use bytes::Bytes;
    use rustshare_core::services::ai::{
        ContentIndexer, InMemoryVectorStore, SimpleEmbeddingGenerator, VectorStore,
    };
    use rustshare_server::services::{
        note_index_sink::ContentIndexerNoteSink, note_service::NoteService,
    };
    use std::sync::Arc;
    use support::calendar_harness::{
        cleanup_tenant, create_test_tenant, create_test_user, setup_test_env,
    };

    let _guard = SERIAL.lock().await;
    let mut state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "memory_reconcile_rebuild", tenant_id).await;
    let vector_store: Arc<dyn VectorStore> = Arc::new(InMemoryVectorStore::new());
    let indexer = Arc::new(ContentIndexer::new(
        Arc::new(SimpleEmbeddingGenerator::new()),
        vector_store,
    ));
    let sink = Arc::new(ContentIndexerNoteSink::new(indexer.clone()));
    state.note_service = Arc::new(
        NoteService::new(
            state.file_service.clone(),
            state.folder_service.clone(),
            state.metadata_store.clone(),
            state.object_store.clone(),
            state.permission_resolver.clone(),
            state.db_pool.clone(),
        )
        .with_index_sink(Some(sink)),
    );

    let body = b"# Recovery source\nThis source missed its indexing callback.";
    let file_id = Uuid::new_v4();
    let mut file = rustshare_core::domain::File::new(
        "index.md".to_string(),
        format!("/Workspace/Meetings/2026/10/{file_id}/index.md"),
        format!("memory-reconcile-{file_id}"),
        body.len() as i64,
        "text/markdown".to_string(),
        None,
        owner.id,
        tenant_id,
    );
    file.id = file_id;
    state
        .object_store
        .put(&file.storage_key(), Bytes::from_static(body))
        .await
        .unwrap();
    state.metadata_store.create_file(&file).await.unwrap();

    assert!(state
        .note_service
        .reconcile_index_file(file.id, owner.id, tenant_id)
        .await
        .unwrap());
    assert!(state
        .note_service
        .reconcile_index_file(file.id, owner.id, tenant_id)
        .await
        .unwrap());
    assert_eq!(indexer.document_count(tenant_id).await, 1);

    state
        .metadata_store
        .delete_file(file.id, owner.id)
        .await
        .unwrap();
    let tombstone = state
        .metadata_store
        .list_files_for_memory_reconciliation(tenant_id, None, 10)
        .await
        .unwrap();
    assert_eq!(tombstone.len(), 1);
    assert!(tombstone[0].file.deleted_at.is_some());
    indexer
        .remove_source_file_chunks(tenant_id, file.id)
        .await
        .unwrap();
    assert_eq!(indexer.document_count(tenant_id).await, 0);

    state
        .object_store
        .delete(&file.storage_key())
        .await
        .unwrap();
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

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
        &MemoryCatalogStore::with_observation_store(
            pool.clone(),
            ChatObservationStore::new(pool.clone()),
        ),
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

    // Watermark strictly between the two observations: only the later
    // message is rebuilt.
    let counts = reconcile(&pool, tenant, Some(t1 + chrono::Duration::seconds(2))).await;
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
    let initial = reconcile(&pool, tenant, None).await;
    assert_eq!(initial.processed, 1);
    assert_eq!(initial.created, 1);

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

    // A late-arriving deletion may have an older source event timestamp than
    // the watermark. It must still rebuild the affected message from its
    // complete history; selecting only event_created_at would miss it.
    let counts = reconcile(&pool, tenant, Some(t0 + chrono::Duration::seconds(6))).await;
    assert_eq!(counts.processed, 2);
    assert_eq!(counts.updated, 1);
    assert_eq!(catalog_count(&pool, tenant).await, 1);
    let row = catalog_row(&pool, tenant).await.expect("record exists");
    assert_eq!(row.get::<String, _>("event_type"), "deleted");
    assert_eq!(row.get::<String, _>("indexing_status"), "tombstoned");
    assert_eq!(row.get::<String, _>("latest_event_id"), hex64(0xdd));

    cleanup(&pool, tenant).await;
}
