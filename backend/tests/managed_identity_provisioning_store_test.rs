use rustshare_storage::{
    ManagedIdentityOperationError, ManagedIdentityOperationStatus, ManagedIdentityOperationStore,
    ProvisioningRequest, MANAGED_IDENTITY_OPERATION_ERROR_CODES,
};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

const TABLE: &str = "managed_chat_identity_operations";
const TENANT_POLICY: &str = "managed_chat_identity_operations_tenant_isolation";
const IDENTITY_TABLE: &str = "managed_chat_identities";

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn managed_identity_tables_are_tenant_scoped_and_cannot_hold_plaintext() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point at a disposable PostgreSQL database");
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("../migrations").run(&pool).await.unwrap();

    for (table, tenant_policy, allowed_columns) in [
        (
            TABLE,
            TENANT_POLICY,
            &["admission_event_id", "error_code", "status"][..],
        ),
        (
            IDENTITY_TABLE,
            "managed_chat_identities_tenant_isolation",
            &[
                "ciphertext",
                "kms_key_id",
                "kms_key_version",
                "nonce",
                "public_key",
                "status",
                "wrapped_dek",
            ][..],
        ),
    ] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(exists, "{table} table is missing; run its migration");

        // Only the explicitly listed ciphertext/envelope metadata or bounded
        // catalog values may use plaintext-capable PostgreSQL types.
        let plaintext_capable: Vec<(String, String)> = sqlx::query_as(
            "SELECT column_name, data_type \
             FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = $1 \
               AND (data_type IN ('bytea','text','character varying','character','json','jsonb') \
                    OR (data_type = 'ARRAY' AND udt_name IN ('_text','_varchar','_json','_jsonb'))) \
             ORDER BY column_name",
        )
        .bind(table)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            plaintext_capable
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            allowed_columns,
            "unexpected plaintext-capable columns in {table} (column, type): {plaintext_capable:?}"
        );

        let (rowsecurity, forcerowsecurity): (bool, bool) = sqlx::query_as(
            "SELECT c.relrowsecurity, c.relforcerowsecurity \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = current_schema() AND c.relname = $1",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(rowsecurity, "RLS must be enabled on {table}");
        assert!(forcerowsecurity, "RLS must be forced on {table}");

        let policies: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT policyname, qual, with_check \
             FROM pg_policies \
             WHERE schemaname = current_schema() AND tablename = $1 \
             ORDER BY policyname",
        )
        .bind(table)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            policies.len(),
            1,
            "expected exactly one RLS policy on {table}: {policies:?}"
        );
        let (policy_name, qual, with_check) = &policies[0];
        assert_eq!(policy_name, tenant_policy);
        let qual = qual.as_deref().unwrap_or_default();
        let with_check = with_check.as_deref().unwrap_or_default();
        assert!(
            qual.contains("app.current_tenant_id"),
            "policy qual must reference app.current_tenant_id: {qual}"
        );
        assert!(
            with_check.contains("app.current_tenant_id"),
            "policy with_check must reference app.current_tenant_id: {with_check}"
        );
    }

    // The migration CHECK catalog on error_code must exactly match the store's
    // catalog: parse every quoted literal out of the rendered constraint so a
    // renamed constraint cannot hide a drift.
    let constraintdefs: Vec<(String,)> = sqlx::query_as(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = $1::regclass AND contype = 'c' \
           AND pg_get_constraintdef(oid) LIKE '%error_code%'",
    )
    .bind(TABLE)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        constraintdefs.len(),
        1,
        "expected exactly one error_code CHECK constraint on {TABLE}: {constraintdefs:?}"
    );
    let constraintdef = &constraintdefs[0].0;
    let mut catalog: Vec<String> = Vec::new();
    let mut rest = constraintdef.as_str();
    while let Some(start) = rest.find('\'') {
        let tail = &rest[start + 1..];
        let end = tail
            .find('\'')
            .expect("unbalanced quote in constraint definition: {constraintdef}");
        catalog.push(tail[..end].to_string());
        rest = &tail[end + 1..];
    }
    catalog.sort();
    catalog.dedup();
    let mut expected: Vec<String> = MANAGED_IDENTITY_OPERATION_ERROR_CODES
        .iter()
        .map(|code| (*code).to_string())
        .collect();
    expected.sort();
    assert_eq!(
        catalog, expected,
        "migration error_code CHECK catalog drifted from the store catalog: {constraintdef}"
    );
    pool.close().await;
}

async fn test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point at a disposable PostgreSQL database");
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("../migrations").run(&pool).await.unwrap();
    pool
}

fn provisioning_request(tenant_id: Uuid, principal_id: Uuid) -> ProvisioningRequest {
    ProvisioningRequest {
        tenant_id,
        principal_id,
        workspace_id: Uuid::new_v4(),
        mapping_id: Uuid::new_v4(),
        mapping_generation: 1,
        idempotency_key: Uuid::new_v4(),
    }
}

/// Let the database clock move so `updated_at` comparisons stay strict.
async fn clock_tick() {
    tokio::time::sleep(Duration::from_millis(5)).await;
}

/// Raw-insert a syntactically valid vault row so authorize can observe an
/// existing generation for the subject (the envelope contents are irrelevant
/// to the operation store).
async fn seed_vault_identity(pool: &PgPool, tenant_id: Uuid, principal_id: Uuid, generation: i64) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let public_key = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO managed_chat_identities \
         (identity_id, tenant_id, principal_id, generation, idempotency_key, public_key, \
          envelope_version, nonce, ciphertext, wrapped_dek, kms_key_id, kms_key_version) \
         VALUES ($1,$2,$3,$4,$5,$6,1,$7,$8,$9,$10,$11)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(principal_id)
    .bind(generation)
    .bind(Uuid::new_v4())
    .bind(public_key)
    .bind(vec![0u8; 12])
    .bind(vec![0u8; 48])
    .bind(vec![0u8; 28])
    .bind("test-kms")
    .bind("v1")
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn authorize_is_idempotent_per_key_and_rejects_second_active_operation() {
    let pool = test_pool().await;
    let store = ManagedIdentityOperationStore::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let principal_id = Uuid::new_v4();
    seed_vault_identity(&pool, tenant_id, principal_id, 3).await;

    let request = provisioning_request(tenant_id, principal_id);
    let first = store.authorize(request.clone()).await.unwrap();
    assert_eq!(
        first.status,
        ManagedIdentityOperationStatus::Authorized.as_str()
    );
    assert_eq!(
        first.identity_generation, 4,
        "generation must be max(existing vault generation) + 1"
    );

    let replayed = store.authorize(request.clone()).await.unwrap();
    assert_eq!(replayed.operation_id, first.operation_id);
    assert_eq!(replayed.identity_id, first.identity_id);
    assert_eq!(replayed.identity_generation, first.identity_generation);
    assert_eq!(replayed.workspace_id, first.workspace_id);

    let loaded = store.load(tenant_id, first.operation_id).await.unwrap();
    assert_eq!(loaded.operation_id, first.operation_id);

    let competing = ProvisioningRequest {
        idempotency_key: Uuid::new_v4(),
        ..request.clone()
    };
    assert_eq!(
        store.authorize(competing).await.unwrap_err(),
        ManagedIdentityOperationError::Conflict
    );
    assert_eq!(store.list_retryable(tenant_id, 10).await.unwrap().len(), 1);
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn failed_rows_stay_in_the_one_active_index_until_abandoned() {
    let pool = test_pool().await;
    let store = ManagedIdentityOperationStore::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let principal_id = Uuid::new_v4();
    let request = provisioning_request(tenant_id, principal_id);
    let operation = store.authorize(request.clone()).await.unwrap();

    clock_tick().await;
    let failed = store
        .mark_failed(tenant_id, operation.operation_id, "kms_unavailable")
        .await
        .unwrap();
    assert_eq!(
        failed.status,
        ManagedIdentityOperationStatus::Failed.as_str()
    );
    assert_eq!(failed.attempt_count, 1);
    assert_eq!(failed.error_code.as_deref(), Some("kms_unavailable"));

    // The failed row still occupies the one-active index: a different key
    // cannot race reconciliation into a second operation.
    let competing = ProvisioningRequest {
        idempotency_key: Uuid::new_v4(),
        ..request.clone()
    };
    assert_eq!(
        store.authorize(competing.clone()).await.unwrap_err(),
        ManagedIdentityOperationError::Conflict
    );

    // A failed row is resumable and still blocks re-authorization while resumed.
    let resumed = store
        .attach_identity(tenant_id, operation.operation_id, Uuid::new_v4(), 1)
        .await
        .unwrap();
    assert_eq!(
        resumed.status,
        ManagedIdentityOperationStatus::Provisioning.as_str()
    );
    assert_eq!(
        store.authorize(competing.clone()).await.unwrap_err(),
        ManagedIdentityOperationError::Conflict
    );

    clock_tick().await;
    let failed_again = store
        .mark_failed(tenant_id, operation.operation_id, "transport_unavailable")
        .await
        .unwrap();
    clock_tick().await;
    let abandoning = store
        .begin_abandon(tenant_id, operation.operation_id)
        .await
        .unwrap();
    assert_eq!(
        abandoning.status,
        ManagedIdentityOperationStatus::Abandoning.as_str()
    );
    assert_eq!(
        store.authorize(competing.clone()).await.unwrap_err(),
        ManagedIdentityOperationError::Conflict
    );
    assert!(!store
        .list_retryable(tenant_id, 10)
        .await
        .unwrap()
        .iter()
        .any(|row| row.operation_id == operation.operation_id));
    let abandoned = store
        .abandon(tenant_id, operation.operation_id)
        .await
        .unwrap();
    assert_eq!(
        abandoned.status,
        ManagedIdentityOperationStatus::Abandoned.as_str()
    );
    assert!(
        abandoned.updated_at > failed_again.updated_at,
        "abandon must advance updated_at like every other CAS update"
    );

    // Leaving the index permits a fresh authorize for the same subject.
    let fresh = store.authorize(competing).await.unwrap();
    assert_ne!(fresh.operation_id, operation.operation_id);

    // An abandoned operation never resumes: every transition is rejected.
    assert_eq!(
        store
            .attach_identity(tenant_id, operation.operation_id, Uuid::new_v4(), 1)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    assert_eq!(
        store
            .mark_failed(tenant_id, operation.operation_id, "internal_error")
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    assert_eq!(
        store
            .mark_ready(tenant_id, operation.operation_id)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    assert_eq!(
        store
            .abandon(tenant_id, operation.operation_id)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );

    // The abandonment claim is only valid from failed, never from authorized.
    let authorized = store
        .authorize(provisioning_request(tenant_id, Uuid::new_v4()))
        .await
        .unwrap();
    assert_eq!(
        store
            .abandon(tenant_id, authorized.operation_id)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn store_cannot_abandon_while_attached_identity_is_active() {
    let pool = test_pool().await;
    let store = ManagedIdentityOperationStore::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let principal_id = Uuid::new_v4();
    let request = provisioning_request(tenant_id, principal_id);
    let operation = store.authorize(request.clone()).await.unwrap();
    let public_key = format!(
        "{}{}",
        operation.identity_id.simple(),
        Uuid::new_v4().simple()
    );
    sqlx::query(
        "INSERT INTO managed_chat_identities \
         (identity_id, tenant_id, principal_id, generation, idempotency_key, public_key, \
          envelope_version, nonce, ciphertext, wrapped_dek, kms_key_id, kms_key_version) \
         VALUES ($1,$2,$3,$4,$5,$6,1,$7,$8,$9,'test-kek','v1')",
    )
    .bind(operation.identity_id)
    .bind(tenant_id)
    .bind(principal_id)
    .bind(operation.identity_generation)
    .bind(operation.idempotency_key)
    .bind(public_key)
    .bind(vec![0u8; 12])
    .bind(vec![0u8; 48])
    .bind(vec![0u8; 28])
    .execute(&pool)
    .await
    .unwrap();
    store
        .attach_identity(
            tenant_id,
            operation.operation_id,
            operation.identity_id,
            operation.identity_generation,
        )
        .await
        .unwrap();
    store
        .mark_failed(
            tenant_id,
            operation.operation_id,
            "stale_mapping_generation",
        )
        .await
        .unwrap();
    store
        .begin_abandon(tenant_id, operation.operation_id)
        .await
        .unwrap();

    assert_eq!(
        store
            .abandon(tenant_id, operation.operation_id)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    assert_eq!(
        store
            .load(tenant_id, operation.operation_id)
            .await
            .unwrap()
            .status,
        ManagedIdentityOperationStatus::Abandoning.as_str()
    );
    let competing = ProvisioningRequest {
        idempotency_key: Uuid::new_v4(),
        ..request
    };
    assert_eq!(
        store.authorize(competing.clone()).await.unwrap_err(),
        ManagedIdentityOperationError::Conflict
    );

    sqlx::query(
        "UPDATE managed_chat_identities SET status='disabled' \
         WHERE tenant_id=$1 AND identity_id=$2 AND principal_id=$3 \
           AND generation=$4 AND status='active'",
    )
    .bind(tenant_id)
    .bind(operation.identity_id)
    .bind(principal_id)
    .bind(operation.identity_generation)
    .execute(&pool)
    .await
    .unwrap();
    store
        .abandon(tenant_id, operation.operation_id)
        .await
        .unwrap();
    assert_eq!(
        store.authorize(competing).await.unwrap().status,
        ManagedIdentityOperationStatus::Authorized.as_str()
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn cas_transitions_are_monotonic() {
    let pool = test_pool().await;
    let store = ManagedIdentityOperationStore::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let principal_id = Uuid::new_v4();

    // Authorize preallocates generation 1 when the subject has no vault row.
    let operation = store
        .authorize(provisioning_request(tenant_id, principal_id))
        .await
        .unwrap();
    assert_eq!(operation.identity_generation, 1);
    let preallocated = operation.identity_id;

    // attach_identity moves authorized -> provisioning and records the actual
    // identity values, replacing the preallocation (no phantom generation).
    let actual_identity_id = Uuid::new_v4();
    clock_tick().await;
    let attached = store
        .attach_identity(tenant_id, operation.operation_id, actual_identity_id, 7)
        .await
        .unwrap();
    assert_eq!(
        attached.status,
        ManagedIdentityOperationStatus::Provisioning.as_str()
    );
    assert_ne!(attached.identity_id, preallocated);
    assert_eq!(attached.identity_id, actual_identity_id);
    assert_eq!(attached.identity_generation, 7);
    assert!(attached.updated_at > operation.updated_at);
    assert_eq!(
        store
            .mark_ready(tenant_id, operation.operation_id)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition,
        "an operation cannot become ready before relay admission is recorded"
    );

    // A second attach_identity is not monotonic.
    assert_eq!(
        store
            .attach_identity(tenant_id, operation.operation_id, Uuid::new_v4(), 8)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    assert_eq!(
        store
            .attach_identity(tenant_id, operation.operation_id, Uuid::new_v4(), 0)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidRequest
    );

    // mark_ready only accepts provisioning; mark_failed accepts any nonterminal
    // status, increments attempts, and records a bounded catalog error code.
    let other = store
        .authorize(provisioning_request(tenant_id, Uuid::new_v4()))
        .await
        .unwrap();
    assert_eq!(
        store
            .mark_ready(tenant_id, other.operation_id)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    clock_tick().await;
    let failed = store
        .mark_failed(tenant_id, other.operation_id, "mapping_unavailable")
        .await
        .unwrap();
    assert_eq!(
        failed.status,
        ManagedIdentityOperationStatus::Failed.as_str()
    );
    assert_eq!(failed.attempt_count, 1);
    assert_eq!(failed.error_code.as_deref(), Some("mapping_unavailable"));
    assert!(failed.last_attempt_at.is_some());
    assert!(failed.updated_at > other.updated_at);
    assert_eq!(
        store
            .mark_ready(tenant_id, other.operation_id)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    clock_tick().await;
    let failed_again = store
        .mark_failed(tenant_id, other.operation_id, "database_unavailable")
        .await
        .unwrap();
    assert_eq!(failed_again.attempt_count, 2);
    assert_eq!(
        failed_again.error_code.as_deref(),
        Some("database_unavailable")
    );
    assert!(failed_again.updated_at > failed.updated_at);
    assert_eq!(
        store
            .mark_failed(tenant_id, other.operation_id, "not_a_catalog_code")
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidRequest
    );
    let unchanged = store.load(tenant_id, other.operation_id).await.unwrap();
    assert_eq!(unchanged.attempt_count, 2);

    // record_admission_sent wins exactly once: it stamps admitted_at alongside
    // the event id, and a replayed call preserves both.
    assert_eq!(
        store
            .record_admission_sent(tenant_id, other.operation_id, "not-hex")
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidRequest
    );
    let event_id = format!("{:064x}", 0xC0FFEEu64);
    clock_tick().await;
    assert!(store
        .record_admission_sent(tenant_id, operation.operation_id, &event_id)
        .await
        .unwrap());
    let after_first = store.load(tenant_id, operation.operation_id).await.unwrap();
    assert_eq!(
        after_first.admission_event_id.as_deref(),
        Some(event_id.as_str())
    );
    assert!(after_first.admitted_at.is_some());
    assert!(after_first.updated_at > attached.updated_at);
    clock_tick().await;
    let replay_event_id = format!("{:064x}", 0xBADu64);
    assert_ne!(replay_event_id, event_id);
    assert!(!store
        .record_admission_sent(tenant_id, operation.operation_id, &replay_event_id)
        .await
        .unwrap());
    let admitted = store.load(tenant_id, operation.operation_id).await.unwrap();
    assert_eq!(
        admitted.admission_event_id.as_deref(),
        Some(event_id.as_str())
    );
    assert_eq!(admitted.admitted_at, after_first.admitted_at);
    assert!(admitted.updated_at > attached.updated_at);

    // mark_ready moves provisioning -> ready exactly once and is terminal.
    clock_tick().await;
    let ready = store
        .mark_ready(tenant_id, operation.operation_id)
        .await
        .unwrap();
    assert_eq!(ready.status, ManagedIdentityOperationStatus::Ready.as_str());
    assert!(ready.ready_at.is_some());
    assert!(ready.updated_at > admitted.updated_at);
    assert_eq!(
        store
            .mark_ready(tenant_id, operation.operation_id)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    // Terminal rows reject every remaining transition; mark_failed must not
    // reopen ready (or abandoned) rows.
    assert_eq!(
        store
            .mark_failed(tenant_id, operation.operation_id, "internal_error")
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    assert_eq!(
        store
            .attach_identity(tenant_id, operation.operation_id, Uuid::new_v4(), 9)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    assert_eq!(
        store
            .abandon(tenant_id, operation.operation_id)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn admitted_identity_cannot_be_rebound_after_failure() {
    let pool = test_pool().await;
    let store = ManagedIdentityOperationStore::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let operation = store
        .authorize(provisioning_request(tenant_id, Uuid::new_v4()))
        .await
        .unwrap();
    let admitted_identity = Uuid::new_v4();
    let admission_event_id = "b".repeat(64);
    store
        .attach_identity(tenant_id, operation.operation_id, admitted_identity, 1)
        .await
        .unwrap();
    store
        .record_admission_sent(tenant_id, operation.operation_id, &admission_event_id)
        .await
        .unwrap();
    store
        .mark_failed(tenant_id, operation.operation_id, "authentication_rejected")
        .await
        .unwrap();

    assert_eq!(
        store
            .attach_identity(tenant_id, operation.operation_id, Uuid::new_v4(), 2)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::InvalidTransition
    );
    let unchanged = store.load(tenant_id, operation.operation_id).await.unwrap();
    assert_eq!(unchanged.identity_id, admitted_identity);
    assert_eq!(unchanged.identity_generation, 1);
    assert_eq!(
        unchanged.admission_event_id.as_deref(),
        Some(admission_event_id.as_str())
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn list_retryable_respects_bounds_and_order() {
    let pool = test_pool().await;
    let store = ManagedIdentityOperationStore::new(pool.clone());
    let tenant_id = Uuid::new_v4();

    let authorized = store
        .authorize(provisioning_request(tenant_id, Uuid::new_v4()))
        .await
        .unwrap();
    clock_tick().await;
    let provisioning = store
        .authorize(provisioning_request(tenant_id, Uuid::new_v4()))
        .await
        .unwrap();
    store
        .attach_identity(tenant_id, provisioning.operation_id, Uuid::new_v4(), 1)
        .await
        .unwrap();
    clock_tick().await;
    let failed = store
        .authorize(provisioning_request(tenant_id, Uuid::new_v4()))
        .await
        .unwrap();
    store
        .mark_failed(tenant_id, failed.operation_id, "kms_unavailable")
        .await
        .unwrap();
    clock_tick().await;
    let exhausted = store
        .authorize(provisioning_request(tenant_id, Uuid::new_v4()))
        .await
        .unwrap();
    for _ in 0..5 {
        store
            .mark_failed(tenant_id, exhausted.operation_id, "transport_unavailable")
            .await
            .unwrap();
    }
    let exhausted_row = store.load(tenant_id, exhausted.operation_id).await.unwrap();
    assert_eq!(exhausted_row.attempt_count, exhausted_row.max_attempts);
    clock_tick().await;
    let ready = store
        .authorize(provisioning_request(tenant_id, Uuid::new_v4()))
        .await
        .unwrap();
    store
        .attach_identity(tenant_id, ready.operation_id, Uuid::new_v4(), 1)
        .await
        .unwrap();
    store
        .record_admission_sent(tenant_id, ready.operation_id, &"a".repeat(64))
        .await
        .unwrap();
    store
        .mark_ready(tenant_id, ready.operation_id)
        .await
        .unwrap();
    clock_tick().await;
    let abandoned = store
        .authorize(provisioning_request(tenant_id, Uuid::new_v4()))
        .await
        .unwrap();
    store
        .mark_failed(tenant_id, abandoned.operation_id, "internal_error")
        .await
        .unwrap();
    store
        .begin_abandon(tenant_id, abandoned.operation_id)
        .await
        .unwrap();
    store
        .abandon(tenant_id, abandoned.operation_id)
        .await
        .unwrap();

    let retryable = store.list_retryable(tenant_id, 50).await.unwrap();
    let ids: Vec<Uuid> = retryable.iter().map(|row| row.operation_id).collect();
    assert_eq!(
        ids,
        vec![
            authorized.operation_id,
            provisioning.operation_id,
            failed.operation_id
        ],
        "ready, abandoned, and exhausted rows must not be retryable"
    );

    let limited = store.list_retryable(tenant_id, 2).await.unwrap();
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].operation_id, authorized.operation_id);
    assert_eq!(limited[1].operation_id, provisioning.operation_id);
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn operations_are_tenant_isolated() {
    let pool = test_pool().await;
    let store = ManagedIdentityOperationStore::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let other_tenant = Uuid::new_v4();
    let principal_id = Uuid::new_v4();

    let operation = store
        .authorize(provisioning_request(tenant_id, principal_id))
        .await
        .unwrap();

    // Cross-tenant store calls see nothing: RLS filters before the predicates.
    assert_eq!(
        store
            .load(other_tenant, operation.operation_id)
            .await
            .unwrap_err(),
        ManagedIdentityOperationError::NotFound
    );
    assert!(store
        .list_retryable(other_tenant, 10)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(store.list_retryable(tenant_id, 10).await.unwrap().len(), 1);

    // A real NOSUPERUSER NOBYPASSRLS role cannot bypass the tenant policy.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "CREATE ROLE managed_identity_operations_rls_probe \
         NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "GRANT SELECT ON managed_chat_identity_operations \
         TO managed_identity_operations_rls_probe",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("GRANT managed_identity_operations_rls_probe TO CURRENT_USER")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL ROLE managed_identity_operations_rls_probe")
        .execute(&mut *tx)
        .await
        .unwrap();
    let unscoped: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM managed_chat_identity_operations WHERE tenant_id=$1",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(unscoped, 0);
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(other_tenant.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let cross_tenant: i64 =
        sqlx::query_scalar("SELECT count(*) FROM managed_chat_identity_operations")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(cross_tenant, 0);
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let scoped: i64 = sqlx::query_scalar("SELECT count(*) FROM managed_chat_identity_operations")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(scoped, 1);
    tx.rollback().await.unwrap();
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn authorize_rejects_nonpositive_mapping_generation_without_creating_rows() {
    let pool = test_pool().await;
    let store = ManagedIdentityOperationStore::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let principal_id = Uuid::new_v4();

    for mapping_generation in [0, -1] {
        let request = ProvisioningRequest {
            mapping_generation,
            ..provisioning_request(tenant_id, principal_id)
        };
        assert_eq!(
            store.authorize(request).await.unwrap_err(),
            ManagedIdentityOperationError::InvalidRequest
        );
    }
    assert!(
        store
            .list_retryable(tenant_id, 10)
            .await
            .unwrap()
            .is_empty(),
        "rejected authorize calls must not create operation rows"
    );
    pool.close().await;
}

#[test]
fn operation_status_parses_all_catalog_values() {
    for (text, expected) in [
        ("authorized", ManagedIdentityOperationStatus::Authorized),
        ("provisioning", ManagedIdentityOperationStatus::Provisioning),
        ("ready", ManagedIdentityOperationStatus::Ready),
        ("failed", ManagedIdentityOperationStatus::Failed),
        ("abandoned", ManagedIdentityOperationStatus::Abandoned),
    ] {
        let parsed = ManagedIdentityOperationStatus::parse(text);
        assert_eq!(parsed, Some(expected), "failed to parse {text:?}");
        assert_eq!(parsed.unwrap().as_str(), text);
    }
    for invalid in ["", "ACTIVE", "Ready", "abandoned ", "revoked"] {
        assert_eq!(
            ManagedIdentityOperationStatus::parse(invalid),
            None,
            "unexpectedly parsed {invalid:?}"
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn list_retryable_breaks_created_at_ties_by_operation_id() {
    let pool = test_pool().await;
    let store = ManagedIdentityOperationStore::new(pool.clone());
    let tenant_id = Uuid::new_v4();
    let mut tied = [Uuid::new_v4(), Uuid::new_v4()];
    tied.sort();

    // One transaction stamps both rows with the same created_at (now() is
    // transaction-scoped), so only the operation_id tiebreak can order them.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    for operation_id in tied {
        sqlx::query(
            "INSERT INTO managed_chat_identity_operations \
             (operation_id, tenant_id, principal_id, workspace_id, mapping_id, mapping_generation, \
              identity_id, identity_generation, idempotency_key) \
             VALUES ($1,$2,$3,$4,$5,1,$6,1,$7)",
        )
        .bind(operation_id)
        .bind(tenant_id)
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    let retryable = store.list_retryable(tenant_id, 10).await.unwrap();
    let ids: Vec<Uuid> = retryable.iter().map(|row| row.operation_id).collect();
    assert_eq!(ids, tied.to_vec());
    pool.close().await;
}
