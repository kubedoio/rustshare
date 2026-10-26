//! Integration tests for Memory Chat reconciliation and Files-backed Notes
//! index recovery. Chat repair re-projects the tenant's `memory_catalog` from
//! verified `chat_observed_events`; Files recovery rebuilds the existing note
//! content index from current source rows and OKF content, with or without AI.
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
        format!("/Workspace/Meetings/General/2026/2026-10-25-recovery-{file_id}/index.md"),
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

#[tokio::test]
#[ignore = "requires DATABASE_URL, object storage, and migrations applied"]
async fn keyword_only_notes_recover_without_ai_and_reject_an_unavailable_index() {
    use axum::{body::Body, http::Request, routing::post, Router};
    use rustshare_core::services::KeywordIndexer;
    use rustshare_infrastructure::PgVectorStore;
    use rustshare_server::{
        handlers::memory_reconcile::reconcile_files_memory,
        services::note_index_sink::{KeywordIndexerNoteSink, NoteIndexSink},
    };
    use std::sync::Arc;
    use support::calendar_harness::{
        cleanup_tenant, create_auth_token, create_test_tenant, create_test_user, database_url,
        setup_test_env,
    };
    use tower::ServiceExt;

    let _guard = SERIAL.lock().await;
    let mut state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let owner = create_test_user(&state, "keyword_recovery", tenant_id).await;
    sqlx::query("UPDATE users SET is_admin = true WHERE id = $1")
        .bind(owner.id)
        .execute(&state.db_pool)
        .await
        .unwrap();
    let token = create_auth_token(&state, owner.id, tenant_id);
    let request = || {
        Request::builder()
            .method("POST")
            .uri("/reconcile")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "tenant_id": tenant_id }).to_string(),
            ))
            .unwrap()
    };
    let unavailable = Router::new()
        .route("/reconcile", post(reconcile_files_memory))
        .with_state(state.clone())
        .oneshot(request())
        .await
        .unwrap();
    assert_eq!(
        unavailable.status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );

    let sink: Arc<dyn NoteIndexSink> = Arc::new(KeywordIndexerNoteSink::new(Arc::new(
        KeywordIndexer::new(Arc::new(PgVectorStore::new(state.db_pool.clone()))),
    )));
    state.note_service = Arc::new((*state.note_service).clone().with_index_sink(Some(sink)));
    assert!(state.ai_service.is_none());
    let note = state
        .note_service
        .create_note(
            owner.id,
            tenant_id,
            Some("Recovery source".into()),
            None,
            Some("A papaya keyword body written without AI.".into()),
        )
        .await
        .unwrap();
    let (content, no_embedding): (String, bool) = sqlx::query_as(
        "SELECT content, embedding IS NULL FROM note_index_chunks WHERE tenant_id = $1 AND source_file_id = $2",
    ).bind(tenant_id).bind(note.id).fetch_one(&state.db_pool).await.unwrap();
    assert!(content.contains("papaya"));
    assert!(no_embedding);

    // Raw Files edits leave the sidecar stale; source policy must still win.
    let mut version = note.current_version;
    for (policy, crlf) in [
        ("denied", false),
        ("invalid-value", false),
        ("denied", true),
    ] {
        let (mut fm, body) =
            rustshare_core::okf::frontmatter::parse_frontmatter(&note.content).unwrap();
        fm.rustshare.as_mut().unwrap().embedding_policy = Some(policy.into());
        let source = rustshare_core::okf::frontmatter::to_document(&fm, &body).unwrap();
        let source = if crlf {
            source.replace('\n', "\r\n")
        } else {
            source
        };
        let updated = state
            .file_service
            .update_file(note.id, owner.id, version, bytes::Bytes::from(source))
            .await
            .unwrap();
        version = updated.current_version;
        assert!(!state
            .note_service
            .reconcile_index_file(note.id, owner.id, tenant_id)
            .await
            .unwrap());
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM note_index_chunks WHERE tenant_id = $1 AND source_file_id = $2",
        )
        .bind(tenant_id)
        .bind(note.id)
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        assert_eq!(
            count, 0,
            "source policy must block indexing despite an allowed sidecar"
        );
    }

    // A failed derived-index write must not roll back authoritative Notes writes.
    let failed_pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy(&database_url())
        .unwrap();
    failed_pool.close().await;
    let failing_sink = Arc::new(KeywordIndexerNoteSink::new(Arc::new(KeywordIndexer::new(
        Arc::new(PgVectorStore::new(failed_pool)),
    ))));
    let failing_notes = (*state.note_service)
        .clone()
        .with_index_sink(Some(failing_sink));
    let unindexed = failing_notes
        .create_note(
            owner.id,
            tenant_id,
            Some("Interrupted index".into()),
            None,
            Some("Original body".into()),
        )
        .await
        .expect("index failure must not fail Note creation");
    let saved = failing_notes
        .save_note(
            unindexed.id,
            owner.id,
            tenant_id,
            "Updated authoritative body".into(),
            None,
            None,
        )
        .await
        .expect("index failure must not fail Note updates");
    assert!(saved.content.contains("Updated authoritative body"));
    let missing: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM note_index_chunks WHERE tenant_id = $1 AND source_file_id = $2",
    )
    .bind(tenant_id)
    .bind(unindexed.id)
    .fetch_one(&state.db_pool)
    .await
    .unwrap();
    assert_eq!(missing, 0);

    let app = Router::new()
        .route("/reconcile", post(reconcile_files_memory))
        .with_state(state.clone());
    for _ in 0..2 {
        let response = app.clone().oneshot(request()).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM note_index_chunks WHERE tenant_id = $1 AND source_file_id = $2 AND embedding IS NULL AND content LIKE '%Updated authoritative body%'",
        ).bind(tenant_id).bind(unindexed.id).fetch_one(&state.db_pool).await.unwrap();
        assert_eq!(
            count, 1,
            "recovery is idempotent and uses current source content"
        );
    }

    // Leave a stale projection behind to exercise the endpoint's tombstone repair.
    state
        .file_service
        .delete_file(unindexed.id, owner.id)
        .await
        .unwrap();
    for _ in 0..2 {
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            axum::http::StatusCode::OK
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM note_index_chunks WHERE tenant_id = $1 AND source_file_id = $2",
        )
        .bind(tenant_id)
        .bind(unindexed.id)
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        assert_eq!(count, 0, "deleted Notes must not reappear during recovery");
    }

    for entry in state
        .metadata_store
        .list_files_for_memory_reconciliation(tenant_id, None, 100)
        .await
        .unwrap()
    {
        state
            .object_store
            .delete(&entry.file.storage_key())
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM note_index_chunks WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&state.db_pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM file_versions WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&state.db_pool)
        .await
        .unwrap();
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[sqlx::test(migrations = "../migrations")]
#[ignore = "requires DATABASE_URL and migrations applied"]
async fn files_reconcile_worker_leases_fence_checkpoints_and_resume_cursors(db: PgPool) {
    use rustshare_resource_auth::ResourceRef;
    use rustshare_server::{
        handlers::memory_reconcile::FilesReconcileResponse,
        memory_files_reconcile_worker::{
            claim, discover_tenant_page, fail_page, finish_page, heartbeat,
        },
    };
    use std::time::Duration;
    use support::calendar_harness::{cleanup_tenant, create_test_tenant};

    let _guard = SERIAL.lock().await;
    sqlx::query("INSERT INTO tenants (id,name,created_at,updated_at) SELECT ('00000000-0000-0000-0000-'||lpad(to_hex(i),12,'0'))::uuid, 'reconcile discovery '||i, NOW(), NOW() FROM generate_series(1,201) i")
        .execute(&db).await.unwrap();
    for expected in [100_u128, 200] {
        discover_tenant_page(&MetadataStore::new(db.clone()))
            .await
            .unwrap();
        let cursor: Option<Uuid> = sqlx::query_scalar(
            "SELECT cursor_tenant_id FROM memory_files_reconcile_discovery WHERE singleton=true",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(cursor, Some(Uuid::from_u128(expected)));
    }
    discover_tenant_page(&MetadataStore::new(db.clone()))
        .await
        .unwrap();
    let discovery_cursor: Option<Uuid> = sqlx::query_scalar(
        "SELECT cursor_tenant_id FROM memory_files_reconcile_discovery WHERE singleton=true",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(discovery_cursor, None);
    let tenant = create_test_tenant(&db).await;
    let metadata = MetadataStore::new(db.clone());
    let initial = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memory_files_reconcile_jobs (tenant_id, cursor_file_id) VALUES ($1, $2)",
    )
    .bind(tenant)
    .bind(initial)
    .execute(&db)
    .await
    .unwrap();

    let worker_one = Uuid::new_v4();
    let worker_two = Uuid::new_v4();
    let (one, two) = tokio::join!(
        claim(&metadata, worker_one, Duration::from_secs(30)),
        claim(&metadata, worker_two, Duration::from_secs(30)),
    );
    let (first, first_owner, successor) = match (one.unwrap(), two.unwrap()) {
        (Some(lease), None) => (lease, worker_one, worker_two),
        (None, Some(lease)) => (lease, worker_two, worker_one),
        _ => panic!("exactly one worker must own the tenant lease"),
    };
    assert_eq!(first.cursor, Some(initial));
    assert!(claim(&metadata, successor, Duration::from_secs(30))
        .await
        .unwrap()
        .is_none());

    sqlx::query("UPDATE memory_files_reconcile_jobs SET lease_until=NOW()-INTERVAL '1 second' WHERE tenant_id=$1")
        .bind(tenant).execute(&db).await.unwrap();
    assert!(
        heartbeat(&metadata, &first, first_owner, Duration::from_secs(30))
            .await
            .is_err()
    );
    let current = claim(&metadata, successor, Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    assert!(current.generation > first.generation);
    assert!(
        heartbeat(&metadata, &first, first_owner, Duration::from_secs(30))
            .await
            .is_err()
    );
    let ref_for = |id: Uuid| {
        ResourceRef::new(
            rustshare_core::domain::ApplicationId::new("io.elembra.files"),
            "file",
            id.to_string(),
        )
        .to_uri()
    };

    assert!(finish_page(
        &metadata,
        &first,
        first_owner,
        FilesReconcileResponse {
            processed: 1,
            indexed: 1,
            removed: 0,
            unavailable: 0,
            next_cursor: Some(ref_for(Uuid::new_v4())),
        },
        Duration::from_secs(3600)
    )
    .await
    .is_err());
    assert!(fail_page(&metadata, &first, first_owner).await.is_err());
    let saved: Option<Uuid> = sqlx::query_scalar(
        "SELECT cursor_file_id FROM memory_files_reconcile_jobs WHERE tenant_id=$1",
    )
    .bind(tenant)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(saved, Some(initial));

    fail_page(&metadata, &current, successor).await.unwrap();
    let retries: i32 = sqlx::query_scalar(
        "SELECT retry_count FROM memory_files_reconcile_jobs WHERE tenant_id=$1",
    )
    .bind(tenant)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(retries, 1);
    sqlx::query("UPDATE memory_files_reconcile_jobs SET next_run_at=NOW() WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    let replay_owner = Uuid::new_v4();
    let replay = claim(&metadata, replay_owner, Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(replay.cursor, Some(initial));

    let second_cursor = Uuid::new_v4();
    finish_page(
        &metadata,
        &replay,
        replay_owner,
        FilesReconcileResponse {
            processed: 100,
            indexed: 98,
            removed: 2,
            unavailable: 0,
            next_cursor: Some(ref_for(second_cursor)),
        },
        Duration::from_secs(3600),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE memory_files_reconcile_jobs SET next_run_at=NOW(), lease_until=NOW()-INTERVAL '1 second' WHERE tenant_id=$1")
        .bind(tenant).execute(&db).await.unwrap();
    let restarted_owner = Uuid::new_v4();
    let restarted = claim(&metadata, restarted_owner, Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(restarted.cursor, Some(second_cursor));
    finish_page(
        &metadata,
        &restarted,
        restarted_owner,
        FilesReconcileResponse {
            processed: 1,
            indexed: 1,
            removed: 0,
            unavailable: 0,
            next_cursor: None,
        },
        Duration::from_secs(3600),
    )
    .await
    .unwrap();
    let row = sqlx::query("SELECT cursor_file_id, processed, last_success_at FROM memory_files_reconcile_jobs WHERE tenant_id=$1")
        .bind(tenant).fetch_one(&db).await.unwrap();
    assert_eq!(row.get::<Option<Uuid>, _>("cursor_file_id"), None);
    assert_eq!(row.get::<i64, _>("processed"), 101);
    assert!(row
        .get::<Option<chrono::DateTime<Utc>>, _>("last_success_at")
        .is_some());

    cleanup_tenant(&db, tenant).await;
}

async fn wait_for_worker_job(
    db: &PgPool,
    tenant: Uuid,
    ready: impl Fn(&sqlx::postgres::PgRow) -> bool,
) -> sqlx::postgres::PgRow {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            if let Some(row) =
                sqlx::query("SELECT * FROM memory_files_reconcile_jobs WHERE tenant_id=$1")
                    .bind(tenant)
                    .fetch_optional(db)
                    .await
                    .unwrap()
            {
                if ready(&row) {
                    break row;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("worker did not reach the expected durable state")
}

#[sqlx::test(migrations = "../migrations")]
#[ignore = "requires DATABASE_URL and object storage"]
async fn files_reconcile_worker_automates_recovery_and_bounds_failures(db: PgPool) {
    use rustshare_core::services::KeywordIndexer;
    use rustshare_infrastructure::PgVectorStore;
    use rustshare_server::{
        memory_files_reconcile_worker::{
            spawn_memory_files_reconcile_worker, MemoryFilesReconcileConfig,
        },
        services::note_index_sink::KeywordIndexerNoteSink,
    };
    use std::{sync::Arc, time::Duration};
    use support::calendar_harness::{
        create_test_tenant, create_test_user, setup_test_env_with_pool,
    };

    let state = setup_test_env_with_pool(db.clone()).await;
    let tenant = create_test_tenant(&db).await;
    let owner = create_test_user(&state, "worker_recovery", tenant).await;
    let other_tenant = create_test_tenant(&db).await;
    let other_owner = create_test_user(&state, "worker_other_tenant", other_tenant).await;
    let other_note = state
        .note_service
        .create_note(
            other_owner.id,
            other_tenant,
            Some("Other tenant source".into()),
            None,
            Some("Isolated quince body.".into()),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_files_reconcile_jobs (tenant_id, status) VALUES ($1,'paused')")
        .bind(other_tenant)
        .execute(&db)
        .await
        .unwrap();
    // With no worker and no sink, authoritative Notes writes still succeed.
    assert!(state.memory_files_reconcile_worker.is_none());
    let note = state
        .note_service
        .create_note(
            owner.id,
            tenant,
            Some("Worker source".into()),
            None,
            Some("A durable persimmon body.".into()),
        )
        .await
        .unwrap();
    let orphan = rustshare_core::domain::File::new(
        "orphan.md".into(),
        "/orphan.md".into(),
        "orphan-hash".into(),
        1,
        "text/markdown".into(),
        None,
        owner.id,
        Uuid::new_v4(),
    );
    state.metadata_store.create_file(&orphan).await.unwrap();
    let notes = Arc::new((*state.note_service).clone().with_index_sink(Some(Arc::new(
        KeywordIndexerNoteSink::new(Arc::new(KeywordIndexer::new(Arc::new(PgVectorStore::new(
            db.clone(),
        ))))),
    ))));
    assert!(state.ai_service.is_none());
    let count = || {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM note_index_chunks WHERE tenant_id=$1 AND source_file_id=$2",
        )
        .bind(tenant)
        .bind(note.id)
        .fetch_one(&db)
    };
    assert_eq!(count().await.unwrap(), 0);
    // A newly created source may sort before the cursor saved by an earlier page.
    let saved_cursor = Uuid::from_u128(u128::MAX);
    assert!(note.id < saved_cursor);
    sqlx::query(
        "INSERT INTO memory_files_reconcile_jobs (tenant_id, cursor_file_id) VALUES ($1,$2)",
    )
    .bind(tenant)
    .bind(saved_cursor)
    .execute(&db)
    .await
    .unwrap();
    let config = MemoryFilesReconcileConfig {
        interval: Duration::from_secs(60),
        poll_interval: Duration::from_millis(20),
        lease: Duration::from_secs(3),
        max_concurrent: 1,
        page_timeout: Duration::from_secs(2),
    };
    let (shutdown, _) = tokio::sync::broadcast::channel(1);
    let worker = spawn_memory_files_reconcile_worker(
        state.metadata_store.clone(),
        notes.clone(),
        config,
        shutdown.subscribe(),
    );
    let completed = wait_for_worker_job(&db, tenant, |r| {
        r.get::<Option<DateTime<Utc>>, _>("last_success_at")
            .is_some()
    })
    .await;
    assert_eq!(completed.get::<Option<Uuid>, _>("cursor_file_id"), None);
    assert_eq!(count().await.unwrap(), 0);

    let (source_lock, _) = state
        .metadata_store
        .lock_file_for_memory_reconciliation(tenant, note.id)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE memory_files_reconcile_jobs SET next_run_at=NOW() WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    let claimed =
        wait_for_worker_job(&db, tenant, |r| r.get::<&str, _>("status") == "running").await;
    let first_heartbeat = claimed.get::<Option<DateTime<Utc>>, _>("heartbeat_at");
    sqlx::query("UPDATE memory_files_reconcile_jobs SET status='idle' WHERE tenant_id=$1")
        .bind(other_tenant)
        .execute(&db)
        .await
        .unwrap();
    let running: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory_files_reconcile_jobs WHERE status='running'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        running, 1,
        "one worker slot must process only one tenant at a time"
    );
    wait_for_worker_job(&db, tenant, |r| {
        r.get::<Option<DateTime<Utc>>, _>("heartbeat_at") > first_heartbeat
    })
    .await;
    let failed = wait_for_worker_job(&db, tenant, |r| r.get::<i32, _>("retry_count") == 1).await;
    assert_eq!(failed.get::<Option<Uuid>, _>("cursor_file_id"), None);
    assert_eq!(
        failed.get::<Option<DateTime<Utc>>, _>("last_success_at"),
        completed.get::<Option<DateTime<Utc>>, _>("last_success_at")
    );
    assert!(failed.get::<DateTime<Utc>, _>("next_run_at") > Utc::now());
    wait_for_worker_job(&db, other_tenant, |r| {
        r.get::<Option<DateTime<Utc>>, _>("last_success_at")
            .is_some()
    })
    .await;
    let other_chunks: i64 = sqlx::query_scalar("SELECT count(*) FROM note_index_chunks WHERE tenant_id=$1 AND source_file_id=$2 AND content LIKE '%quince%'")
        .bind(other_tenant).bind(other_note.id).fetch_one(&db).await.unwrap();
    assert_eq!(
        other_chunks, 1,
        "an overdue tenant runs while the failed tenant is backing off"
    );
    assert_eq!(count().await.unwrap(), 0);

    // Shutdown cancels a blocked replay and leaves its cursor for lease recovery.
    sqlx::query("UPDATE memory_files_reconcile_jobs SET next_run_at=NOW() WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    wait_for_worker_job(&db, tenant, |r| r.get::<&str, _>("status") == "running").await;
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), worker)
        .await
        .unwrap()
        .unwrap();
    source_lock.rollback().await.unwrap();
    sqlx::query("UPDATE memory_files_reconcile_jobs SET lease_until=NOW()-INTERVAL '1 second' WHERE tenant_id=$1")
        .bind(tenant).execute(&db).await.unwrap();
    let worker = spawn_memory_files_reconcile_worker(
        state.metadata_store.clone(),
        notes.clone(),
        config,
        shutdown.subscribe(),
    );
    wait_for_worker_job(&db, tenant, |r| {
        r.get::<Option<DateTime<Utc>>, _>("last_success_at")
            > completed.get::<Option<DateTime<Utc>>, _>("last_success_at")
    })
    .await;
    assert_eq!(
        count().await.unwrap(),
        1,
        "a full sweep repairs the source before the old UUID cursor"
    );
    shutdown.send(()).unwrap();
    worker.await.unwrap();

    // An object-store outage cannot mark the saved cursor complete or erase
    // the previous projection; once content is restored, the page can replay.
    let source_file = state
        .metadata_store
        .find_file_by_id_unchecked(note.id)
        .await
        .unwrap()
        .unwrap();
    let source_key = source_file.storage_key();
    let source_body = state.object_store.get(&source_key).await.unwrap();
    state.object_store.delete(&source_key).await.unwrap();
    sqlx::query("UPDATE memory_files_reconcile_jobs SET next_run_at=NOW() WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    let worker = spawn_memory_files_reconcile_worker(
        state.metadata_store.clone(),
        notes.clone(),
        config,
        shutdown.subscribe(),
    );
    let outage = wait_for_worker_job(&db, tenant, |r| r.get::<i32, _>("retry_count") == 1).await;
    assert_eq!(outage.get::<Option<Uuid>, _>("cursor_file_id"), None);
    assert_eq!(count().await.unwrap(), 1);
    shutdown.send(()).unwrap();
    worker.await.unwrap();
    state
        .object_store
        .put(&source_key, source_body)
        .await
        .unwrap();

    // An index statement that outlives a page timeout must roll back before a
    // concurrent source deletion completes, so it cannot recreate stale chunks.
    sqlx::query("CREATE FUNCTION m33_block_note_index() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(418); RETURN NEW; END $$")
        .execute(&db).await.unwrap();
    sqlx::query("CREATE TRIGGER m33_block_note_index BEFORE INSERT ON note_index_chunks FOR EACH ROW EXECUTE FUNCTION m33_block_note_index()")
        .execute(&db).await.unwrap();
    let mut advisory = db.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(418)")
        .execute(&mut *advisory)
        .await
        .unwrap();
    sqlx::query("UPDATE memory_files_reconcile_jobs SET next_run_at=NOW() WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    let worker = spawn_memory_files_reconcile_worker(
        state.metadata_store.clone(),
        notes.clone(),
        config,
        shutdown.subscribe(),
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype='advisory' AND database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND NOT granted)")
                .fetch_one(&db).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("worker did not reach the blocked index write");
    wait_for_worker_job(&db, tenant, |r| r.get::<i32, _>("retry_count") == 1).await;
    shutdown.send(()).unwrap();
    worker.await.unwrap();
    state
        .file_service
        .delete_file(note.id, owner.id)
        .await
        .unwrap();
    notes
        .remove_source_file_from_index(note.id, tenant)
        .await
        .unwrap();
    assert_eq!(count().await.unwrap(), 0);
    advisory.rollback().await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype='advisory' AND database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND NOT granted)")
                .fetch_one(&db).await.unwrap();
            if !waiting { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("timed-out index write did not finish");
    assert_eq!(
        count().await.unwrap(),
        0,
        "a canceled index write must not outlive its source lock"
    );
    sqlx::query("DROP TRIGGER m33_block_note_index ON note_index_chunks")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION m33_block_note_index()")
        .execute(&db)
        .await
        .unwrap();
    state
        .metadata_store
        .restore_file(note.id, owner.id, tenant)
        .await
        .unwrap();
    sqlx::query("UPDATE memory_files_reconcile_jobs SET next_run_at=NOW() WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    let worker = spawn_memory_files_reconcile_worker(
        state.metadata_store.clone(),
        notes.clone(),
        config,
        shutdown.subscribe(),
    );
    wait_for_worker_job(&db, tenant, |r| {
        r.get::<i64, _>("processed") > 0
            && r.get::<i32, _>("retry_count") == 0
            && r.get::<Option<Uuid>, _>("cursor_file_id").is_none()
    })
    .await;
    assert_eq!(count().await.unwrap(), 1);
    shutdown.send(()).unwrap();
    worker.await.unwrap();

    // A real index outage retries the same page and eventually stops, without losing source data.
    let failed_pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy_with(db.connect_options().as_ref().clone());
    failed_pool.close().await;
    let failing = Arc::new((*state.note_service).clone().with_index_sink(Some(Arc::new(
        KeywordIndexerNoteSink::new(Arc::new(KeywordIndexer::new(Arc::new(PgVectorStore::new(
            failed_pool,
        ))))),
    ))));
    let worker = spawn_memory_files_reconcile_worker(
        state.metadata_store.clone(),
        failing,
        config,
        shutdown.subscribe(),
    );
    for retry in 1..=8 {
        sqlx::query("UPDATE memory_files_reconcile_jobs SET next_run_at=NOW() WHERE tenant_id=$1")
            .bind(tenant)
            .execute(&db)
            .await
            .unwrap();
        let row =
            wait_for_worker_job(&db, tenant, |r| r.get::<i32, _>("retry_count") == retry).await;
        assert_eq!(row.get::<Option<Uuid>, _>("cursor_file_id"), None);
    }
    let failed = wait_for_worker_job(&db, tenant, |r| r.get::<&str, _>("status") == "failed").await;
    assert_eq!(failed.get::<i32, _>("retry_count"), 8);
    assert_eq!(
        failed.get::<&str, _>("last_error"),
        "reconciliation page failed"
    );
    assert_eq!(count().await.unwrap(), 1);
    assert!(state
        .note_service
        .get_note(note.id, owner.id, tenant)
        .await
        .unwrap()
        .content
        .contains("persimmon"));
    shutdown.send(()).unwrap();
    worker.await.unwrap();
    for source_tenant in [tenant, other_tenant] {
        for entry in state
            .metadata_store
            .list_files_for_memory_reconciliation(source_tenant, None, 100)
            .await
            .unwrap()
        {
            state
                .object_store
                .delete(&entry.file.storage_key())
                .await
                .unwrap();
        }
    }
}

#[tokio::test]
#[ignore = "requires DATABASE_URL, object storage, and migrations applied"]
async fn memory_reconcile_status_is_scoped_to_admin_tenant() {
    use axum::{
        body::Body,
        http::Request,
        routing::{get, post},
        Router,
    };
    use rustshare_server::memory_files_reconcile_worker::{get_status, retry};
    use support::calendar_harness::{
        cleanup_tenant, create_auth_token, create_test_tenant, create_test_user, setup_test_env,
    };
    use tower::ServiceExt;

    let _guard = SERIAL.lock().await;
    let mut state = setup_test_env().await;
    let tenant = create_test_tenant(&state.db_pool).await;
    let other_tenant = create_test_tenant(&state.db_pool).await;
    let admin = create_test_user(&state, "memory_reconcile_status", tenant).await;
    let foreign_user = create_test_user(&state, "memory_reconcile_foreign", other_tenant).await;
    sqlx::query("UPDATE users SET is_admin = true WHERE id = $1")
        .bind(admin.id)
        .execute(&state.db_pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO memory_files_reconcile_jobs (tenant_id, processed) VALUES ($1, 11), ($2, 99)",
    )
    .bind(tenant)
    .bind(other_tenant)
    .execute(&state.db_pool)
    .await
    .unwrap();
    let token = create_auth_token(&state, admin.id, tenant);
    let foreign_token = create_auth_token(&state, foreign_user.id, other_tenant);
    let app = Router::new()
        .route("/status", get(get_status))
        .route("/retry", post(retry))
        .with_state(state.clone());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/status")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let status: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(status["processed"], 11);
    assert_eq!(status["enabled"], false);
    assert_eq!(status["status"], "disabled");
    for (method, path) in [("GET", "/status"), ("POST", "/retry")] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", format!("Bearer {foreign_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
    }

    let cursor = Uuid::new_v4();
    sqlx::query("UPDATE memory_files_reconcile_jobs SET status='running', cursor_file_id=$2, lease_owner=$3, lease_generation=4, lease_until=NOW()+INTERVAL '1 minute', heartbeat_at=NOW() WHERE tenant_id=$1")
        .bind(tenant).bind(cursor).bind(Uuid::new_v4()).execute(&state.db_pool).await.unwrap();
    let resume = || {
        Request::builder()
            .method("POST")
            .uri("/retry")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(resume()).await.unwrap().status(),
        axum::http::StatusCode::CONFLICT
    );
    sqlx::query("UPDATE memory_files_reconcile_jobs SET lease_until=NOW()-INTERVAL '1 second' WHERE tenant_id=$1")
        .bind(tenant).execute(&state.db_pool).await.unwrap();
    state.memory_files_reconcile_worker = Some(std::sync::Arc::new(tokio::sync::Mutex::new(
        tokio::spawn(async {}),
    )));
    let enabled_app = Router::new()
        .route("/status", get(get_status))
        .with_state(state.clone());
    let response = enabled_app
        .oneshot(
            Request::builder()
                .uri("/status")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let paused: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(paused["enabled"], true);
    assert_eq!(paused["status"], "paused");
    assert_eq!(
        app.oneshot(resume()).await.unwrap().status(),
        axum::http::StatusCode::ACCEPTED
    );
    let row = sqlx::query("SELECT cursor_file_id, lease_generation, heartbeat_at FROM memory_files_reconcile_jobs WHERE tenant_id=$1")
        .bind(tenant).fetch_one(&state.db_pool).await.unwrap();
    assert_eq!(row.get::<Option<Uuid>, _>("cursor_file_id"), Some(cursor));
    assert_eq!(row.get::<i64, _>("lease_generation"), 5);
    assert!(row
        .get::<Option<DateTime<Utc>>, _>("heartbeat_at")
        .is_none());
    cleanup_tenant(&state.db_pool, tenant).await;
    cleanup_tenant(&state.db_pool, other_tenant).await;
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

async fn insert_orphan_cleanup_chunk(
    db: &PgPool,
    tenant_id: Uuid,
    owner_id: Uuid,
    chunk_id: Uuid,
    source_file_id: Option<Uuid>,
    embedding: Option<&str>,
) {
    sqlx::query("INSERT INTO note_index_chunks (id, tenant_id, workspace_id, note_id, source_file_id, file_name, file_path, content, mime_type, owner_id, embedding, acl_hash, acl_version, read_acl, visibility, embedding_policy) VALUES ($1,$2,$2,$3,$4,'note.md','/note.md','private test body','text/markdown',$5,$6::vector,'test',1,'{}','private','allowed')")
        .bind(chunk_id)
        .bind(tenant_id)
        .bind(Uuid::new_v4())
        .bind(source_file_id)
        .bind(owner_id)
        .bind(embedding)
        .execute(db)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../migrations")]
#[ignore = "requires PostgreSQL and migrations applied"]
async fn memory_orphan_cleanup_is_tenant_scoped_dry_run_first_and_idempotent(db: PgPool) {
    use rustshare_infrastructure::PgVectorStore;
    use support::calendar_harness::{
        cleanup_tenant, create_test_tenant, create_test_user, setup_test_env_with_pool,
    };

    let _guard = SERIAL.lock().await;
    let state = setup_test_env_with_pool(db.clone()).await;
    let tenant = create_test_tenant(&db).await;
    let foreign_tenant = create_test_tenant(&db).await;
    let owner = create_test_user(&state, "orphan_cleanup_owner", tenant).await;
    let foreign_owner = create_test_user(&state, "orphan_cleanup_foreign", foreign_tenant).await;
    let metadata = MetadataStore::new(db.clone());

    let mut active = rustshare_core::domain::File::new(
        "active.md".into(),
        "/active.md".into(),
        "active-hash".into(),
        1,
        "text/markdown".into(),
        None,
        owner.id,
        tenant,
    );
    active.id = Uuid::new_v4();
    metadata.create_file(&active).await.unwrap();

    let mut soft_deleted = rustshare_core::domain::File::new(
        "retained.md".into(),
        "/retained.md".into(),
        "soft-delete-hash".into(),
        1,
        "text/markdown".into(),
        None,
        owner.id,
        tenant,
    );
    soft_deleted.id = Uuid::new_v4();
    metadata.create_file(&soft_deleted).await.unwrap();
    metadata
        .delete_file(soft_deleted.id, owner.id)
        .await
        .unwrap();

    let mut hard_deleted = rustshare_core::domain::File::new(
        "purged.md".into(),
        "/purged.md".into(),
        "hard-delete-hash".into(),
        1,
        "text/markdown".into(),
        None,
        owner.id,
        tenant,
    );
    hard_deleted.id = Uuid::new_v4();
    metadata.create_file(&hard_deleted).await.unwrap();
    metadata
        .delete_file(hard_deleted.id, owner.id)
        .await
        .unwrap();
    assert!(metadata
        .permanently_delete_file(hard_deleted.id, owner.id)
        .await
        .unwrap());

    let mut foreign_deleted = rustshare_core::domain::File::new(
        "foreign.md".into(),
        "/foreign.md".into(),
        "foreign-delete-hash".into(),
        1,
        "text/markdown".into(),
        None,
        foreign_owner.id,
        foreign_tenant,
    );
    foreign_deleted.id = Uuid::new_v4();
    metadata.create_file(&foreign_deleted).await.unwrap();
    metadata
        .delete_file(foreign_deleted.id, foreign_owner.id)
        .await
        .unwrap();
    metadata
        .permanently_delete_file(foreign_deleted.id, foreign_owner.id)
        .await
        .unwrap();

    let vector = format!("[{}]", vec!["0"; 768].join(","));
    let vector_store = PgVectorStore::new(db.clone());
    let soft_chunk = Uuid::new_v4();
    let active_chunk = Uuid::new_v4();
    let keyword_orphan = Uuid::new_v4();
    let vector_orphan = Uuid::new_v4();
    let foreign_chunk = Uuid::new_v4();
    insert_orphan_cleanup_chunk(&db, tenant, owner.id, active_chunk, Some(active.id), None).await;
    insert_orphan_cleanup_chunk(
        &db,
        tenant,
        owner.id,
        soft_chunk,
        Some(soft_deleted.id),
        None,
    )
    .await;
    insert_orphan_cleanup_chunk(
        &db,
        tenant,
        owner.id,
        keyword_orphan,
        Some(hard_deleted.id),
        None,
    )
    .await;
    insert_orphan_cleanup_chunk(
        &db,
        tenant,
        owner.id,
        vector_orphan,
        Some(hard_deleted.id),
        Some(&vector),
    )
    .await;
    insert_orphan_cleanup_chunk(
        &db,
        foreign_tenant,
        foreign_owner.id,
        foreign_chunk,
        Some(foreign_deleted.id),
        None,
    )
    .await;

    let dry_run = vector_store
        .cleanup_orphan_chunks_page(tenant, None, 100, true)
        .await
        .unwrap();
    assert_eq!(dry_run.candidates, 2);
    assert_eq!(dry_run.removed, 0);
    assert_eq!(dry_run.unresolved, 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note_index_chunks WHERE tenant_id=$1")
            .bind(tenant)
            .fetch_one(&db)
            .await
            .unwrap(),
        4
    );

    let removed = vector_store
        .cleanup_orphan_chunks_page(tenant, None, 100, false)
        .await
        .unwrap();
    // Restoring a retained source after cleanup keeps the retained index usable.
    assert!(metadata
        .restore_file(soft_deleted.id, owner.id, tenant)
        .await
        .unwrap());
    assert_eq!(removed.candidates, 2);
    assert_eq!(removed.removed, 2);
    assert_eq!(removed.next_cursor, None);
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM note_index_chunks WHERE id=$1)")
            .bind(soft_chunk)
            .fetch_one(&db)
            .await
            .unwrap(),
        "soft-deleted Files rows still exist and must be preserved"
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM note_index_chunks WHERE id=$1)")
            .bind(active_chunk)
            .fetch_one(&db)
            .await
            .unwrap(),
        "chunks with an active authoritative Files row must be preserved"
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM note_index_chunks WHERE id=$1)")
            .bind(foreign_chunk)
            .fetch_one(&db)
            .await
            .unwrap(),
        "cleanup for one tenant must not delete another tenant's chunks"
    );
    let repeated = vector_store
        .cleanup_orphan_chunks_page(tenant, None, 100, false)
        .await
        .unwrap();
    assert_eq!(repeated.removed, 0);
    assert_eq!(repeated.candidates, 0);

    sqlx::query("DELETE FROM note_index_chunks WHERE tenant_id = ANY($1)")
        .bind([tenant, foreign_tenant].as_slice())
        .execute(&db)
        .await
        .unwrap();
    cleanup_tenant(&db, tenant).await;
    cleanup_tenant(&db, foreign_tenant).await;
}

#[sqlx::test(migrations = "../migrations")]
#[ignore = "requires PostgreSQL and migrations applied"]
async fn memory_orphan_cleanup_keeps_null_source_identity_unresolved(db: PgPool) {
    use rustshare_infrastructure::PgVectorStore;
    use support::calendar_harness::{
        cleanup_tenant, create_test_tenant, create_test_user, setup_test_env_with_pool,
    };

    let _guard = SERIAL.lock().await;
    let state = setup_test_env_with_pool(db.clone()).await;
    let tenant = create_test_tenant(&db).await;
    let owner = create_test_user(&state, "orphan_cleanup_null", tenant).await;
    let chunk_id = Uuid::new_v4();
    sqlx::query("ALTER TABLE note_index_chunks ALTER COLUMN source_file_id DROP NOT NULL")
        .execute(&db)
        .await
        .unwrap();
    insert_orphan_cleanup_chunk(&db, tenant, owner.id, chunk_id, None, None).await;

    let result = PgVectorStore::new(db.clone())
        .cleanup_orphan_chunks_page(tenant, None, 100, false)
        .await
        .unwrap();
    assert_eq!(result.unresolved, 1);
    assert_eq!(result.candidates, 0);
    assert_eq!(result.removed, 0);
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM note_index_chunks WHERE id=$1 AND source_file_id IS NULL)"
    )
    .bind(chunk_id)
    .fetch_one(&db)
    .await
    .unwrap());

    sqlx::query("DELETE FROM note_index_chunks WHERE id=$1")
        .bind(chunk_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE note_index_chunks ALTER COLUMN source_file_id SET NOT NULL")
        .execute(&db)
        .await
        .unwrap();
    cleanup_tenant(&db, tenant).await;
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
