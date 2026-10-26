//! PostgreSQL integration coverage for hard-delete orphan cleanup.

use sqlx::{PgPool, Row};
use std::sync::LazyLock;
use uuid::Uuid;

mod support;

static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

async fn insert_chunk(db: &PgPool, tenant: Uuid, source: Uuid, id: Uuid) {
    sqlx::query("INSERT INTO note_index_chunks (id, tenant_id, workspace_id, note_id, source_file_id, file_name, file_path, content, mime_type, owner_id, embedding, acl_hash, acl_version, read_acl, visibility, embedding_policy) VALUES ($1,$2,$2,$3,$4,'note.md','/note.md','private test body','text/markdown',$5,NULL,'test',1,'{}','private','allowed')")
        .bind(id)
        .bind(tenant)
        .bind(Uuid::new_v4())
        .bind(source)
        .bind(Uuid::new_v4())
        .execute(db)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../migrations")]
#[ignore = "requires PostgreSQL and migrations applied"]
async fn memory_orphan_cleanup_worker_discovers_and_removes_hard_deleted_source(db: PgPool) {
    use rustshare_infrastructure::PgVectorStore;
    use rustshare_server::memory_orphan_cleanup_worker::{
        spawn_memory_orphan_cleanup_worker, MemoryOrphanCleanupConfig,
    };
    use std::{sync::Arc, time::Duration};
    use support::calendar_harness::{cleanup_tenant, create_test_tenant};

    let _guard = SERIAL.lock().await;
    let tenant = create_test_tenant(&db).await;
    let chunk = Uuid::new_v4();
    insert_chunk(&db, tenant, Uuid::new_v4(), chunk).await;
    let (shutdown, receiver) = tokio::sync::broadcast::channel(2);
    let worker = spawn_memory_orphan_cleanup_worker(
        db.clone(),
        Arc::new(PgVectorStore::new(db.clone())),
        MemoryOrphanCleanupConfig {
            interval: Duration::from_secs(3600),
            poll_interval: Duration::from_millis(10),
            lease: Duration::from_secs(30),
            max_concurrent: 1,
            page_timeout: Duration::from_secs(10),
            dry_run: false,
        },
        receiver,
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let removed: i64 = sqlx::query_scalar(
                "SELECT removed FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=$1",
            )
            .bind(tenant)
            .fetch_optional(&db)
            .await
            .unwrap()
            .unwrap_or_default();
            if removed == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("orphan worker did not clean the indexed chunk");
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM note_index_chunks WHERE id=$1)"
    )
    .bind(chunk)
    .fetch_one(&db)
    .await
    .unwrap());
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), worker)
        .await
        .expect("worker shutdown timed out")
        .unwrap();
    sqlx::query("DELETE FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    cleanup_tenant(&db, tenant).await;
}

#[sqlx::test(migrations = "../migrations")]
#[ignore = "requires PostgreSQL and migrations applied"]
async fn memory_orphan_cleanup_discovery_advances_in_bounded_pages(db: PgPool) {
    use rustshare_server::memory_orphan_cleanup_worker::discover_tenant_page;
    use support::calendar_harness::{cleanup_tenant, create_test_tenant};

    let _guard = SERIAL.lock().await;
    let tenant = create_test_tenant(&db).await;
    let late_tenant = create_test_tenant(&db).await;
    sqlx::query("INSERT INTO note_index_chunks (id, tenant_id, workspace_id, note_id, source_file_id, file_name, file_path, content, mime_type, owner_id) SELECT ('00000000-0000-0000-0000-' || lpad(to_hex(i),12,'0'))::uuid, $1, $1, gen_random_uuid(), gen_random_uuid(), 'note.md', '/note.md', 'test', 'text/markdown', gen_random_uuid() FROM generate_series(1,201) i")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    for expected in [100_u128, 200] {
        discover_tenant_page(&db).await.unwrap();
        let cursor: Option<Uuid> = sqlx::query_scalar(
            "SELECT cursor_chunk_id FROM memory_note_orphan_cleanup_discovery WHERE singleton=true",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(cursor, Some(Uuid::from_u128(expected)));
    }
    sqlx::query("INSERT INTO note_index_chunks (id, tenant_id, workspace_id, note_id, source_file_id, file_name, file_path, content, mime_type, owner_id) VALUES ($1,$2,$2,gen_random_uuid(),gen_random_uuid(),'note.md','/note.md','test','text/markdown',gen_random_uuid())")
        .bind(Uuid::from_u128(0))
        .bind(late_tenant)
        .execute(&db)
        .await
        .unwrap();
    discover_tenant_page(&db).await.unwrap();
    let cursor: Option<Uuid> = sqlx::query_scalar(
        "SELECT cursor_chunk_id FROM memory_note_orphan_cleanup_discovery WHERE singleton=true",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(cursor, None);
    discover_tenant_page(&db).await.unwrap();
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=$1)"
    )
    .bind(late_tenant)
    .fetch_one(&db)
    .await
    .unwrap());
    sqlx::query("DELETE FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=ANY($1)")
        .bind([tenant, late_tenant].as_slice())
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM note_index_chunks WHERE tenant_id=ANY($1)")
        .bind([tenant, late_tenant].as_slice())
        .execute(&db)
        .await
        .unwrap();
    cleanup_tenant(&db, tenant).await;
    cleanup_tenant(&db, late_tenant).await;
}

#[sqlx::test(migrations = "../migrations")]
#[ignore = "requires PostgreSQL and migrations applied"]
async fn memory_orphan_cleanup_worker_fences_retries_and_resumes_saved_cursor(db: PgPool) {
    use rustshare_infrastructure::{vector::pg_vector_store::OrphanCleanupPage, PgVectorStore};
    use rustshare_server::memory_orphan_cleanup_worker::{
        claim, fail_page, finish_page, heartbeat, process_page_and_checkpoint,
        MemoryOrphanCleanupConfig,
    };
    use std::{sync::Arc, time::Duration};
    use support::calendar_harness::{cleanup_tenant, create_test_tenant};

    let _guard = SERIAL.lock().await;
    let tenant = create_test_tenant(&db).await;
    for _ in 0..101 {
        insert_chunk(&db, tenant, Uuid::new_v4(), Uuid::new_v4()).await;
    }
    sqlx::query("INSERT INTO memory_note_orphan_cleanup_jobs (tenant_id) VALUES ($1)")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    let store = Arc::new(PgVectorStore::new(db.clone()));
    let config = MemoryOrphanCleanupConfig {
        interval: Duration::from_secs(3600),
        poll_interval: Duration::from_millis(5),
        lease: Duration::from_secs(30),
        max_concurrent: 1,
        page_timeout: Duration::from_secs(10),
        dry_run: false,
    };
    let first_owner = Uuid::new_v4();
    let other_owner = Uuid::new_v4();
    let first = claim(&db, first_owner, config.lease)
        .await
        .unwrap()
        .unwrap();
    assert!(claim(&db, other_owner, config.lease)
        .await
        .unwrap()
        .is_none());

    sqlx::query("CREATE FUNCTION m34_reject_index_delete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected cleanup failure'; END $$")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER m34_reject_index_delete BEFORE DELETE ON note_index_chunks FOR EACH ROW EXECUTE FUNCTION m34_reject_index_delete()")
        .execute(&db)
        .await
        .unwrap();
    assert!(
        process_page_and_checkpoint(&db, &store, config, first_owner, &first)
            .await
            .is_err()
    );
    fail_page(&db, &first, first_owner).await.unwrap();
    let retry_count: i32 = sqlx::query_scalar(
        "SELECT retry_count FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=$1",
    )
    .bind(tenant)
    .fetch_one(&db)
    .await
    .unwrap();
    let saved_cursor: Option<Uuid> = sqlx::query_scalar(
        "SELECT cursor_chunk_id FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=$1",
    )
    .bind(tenant)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(retry_count, 1);
    assert_eq!(saved_cursor, None);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note_index_chunks WHERE tenant_id=$1")
            .bind(tenant)
            .fetch_one(&db)
            .await
            .unwrap(),
        101
    );

    sqlx::query("DROP TRIGGER m34_reject_index_delete ON note_index_chunks")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION m34_reject_index_delete()")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE memory_note_orphan_cleanup_jobs SET next_run_at=NOW(), lease_until=NOW()-INTERVAL '1 second' WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    let next_owner = Uuid::new_v4();
    let next = claim(&db, next_owner, config.lease).await.unwrap().unwrap();
    assert!(next.generation > first.generation);
    assert_eq!(next.cursor, None);
    assert!(heartbeat(&db, &first, first_owner, config.lease)
        .await
        .is_err());
    assert!(finish_page(
        &db,
        &first,
        first_owner,
        OrphanCleanupPage {
            next_cursor: Some(Uuid::new_v4()),
            scanned: 1,
            candidates: 1,
            removed: 1,
            unresolved: 0,
        },
        config.interval,
    )
    .await
    .is_err());
    process_page_and_checkpoint(&db, &store, config, next_owner, &next)
        .await
        .unwrap();

    let saved_cursor: Option<Uuid> = sqlx::query_scalar(
        "SELECT cursor_chunk_id FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=$1",
    )
    .bind(tenant)
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(
        saved_cursor.is_some(),
        "first successful page checkpoints its cursor"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT removed FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=$1"
        )
        .bind(tenant)
        .fetch_one(&db)
        .await
        .unwrap(),
        100
    );

    // A new process claims the job and resumes strictly after the persisted page cursor.
    let resumed_owner = Uuid::new_v4();
    let resumed = claim(&db, resumed_owner, config.lease)
        .await
        .unwrap()
        .unwrap();
    assert!(resumed.generation > next.generation);
    assert_eq!(resumed.cursor, saved_cursor);
    assert!(finish_page(
        &db,
        &next,
        next_owner,
        OrphanCleanupPage {
            next_cursor: Some(Uuid::new_v4()),
            scanned: 1,
            candidates: 1,
            removed: 1,
            unresolved: 0,
        },
        config.interval,
    )
    .await
    .is_err());
    process_page_and_checkpoint(&db, &store, config, resumed_owner, &resumed)
        .await
        .unwrap();

    let row = sqlx::query("SELECT cursor_chunk_id, removed, retry_count, last_success_at FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=$1")
        .bind(tenant)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<Option<Uuid>, _>("cursor_chunk_id"), None);
    assert_eq!(row.get::<i64, _>("removed"), 101);
    assert_eq!(row.get::<i32, _>("retry_count"), 0);
    assert!(row
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_success_at")
        .is_some());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM note_index_chunks WHERE tenant_id=$1")
            .bind(tenant)
            .fetch_one(&db)
            .await
            .unwrap(),
        0
    );

    let earlier_chunk = Uuid::from_u128(0);
    insert_chunk(&db, tenant, Uuid::new_v4(), earlier_chunk).await;
    sqlx::query("UPDATE memory_note_orphan_cleanup_jobs SET next_run_at=clock_timestamp() WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    let sweep_owner = Uuid::new_v4();
    let sweep = claim(&db, sweep_owner, config.lease)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        sweep.cursor, None,
        "a periodic full sweep restarts at the beginning"
    );
    process_page_and_checkpoint(&db, &store, config, sweep_owner, &sweep)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT removed FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=$1"
        )
        .bind(tenant)
        .fetch_one(&db)
        .await
        .unwrap(),
        102
    );
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM note_index_chunks WHERE tenant_id=$1 AND id=$2)"
    )
    .bind(tenant)
    .bind(earlier_chunk)
    .fetch_one(&db)
    .await
    .unwrap());

    sqlx::query("DELETE FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=$1")
        .bind(tenant)
        .execute(&db)
        .await
        .unwrap();
    cleanup_tenant(&db, tenant).await;
}

#[sqlx::test(migrations = "../migrations")]
#[ignore = "requires PostgreSQL and migrations applied"]
async fn memory_orphan_cleanup_does_not_resurrect_a_concurrent_stale_upsert(db: PgPool) {
    use rustshare_core::services::ai::{IndexedDocument, NoteAclPayload, VectorStore};
    use rustshare_infrastructure::PgVectorStore;
    use std::{sync::Arc, time::Duration};
    use support::calendar_harness::{
        cleanup_tenant, create_test_tenant, create_test_user, setup_test_env_with_pool,
    };

    let _guard = SERIAL.lock().await;
    let state = setup_test_env_with_pool(db.clone()).await;
    let tenant = create_test_tenant(&db).await;
    let owner = create_test_user(&state, "orphan_cleanup_race", tenant).await;
    let metadata = rustshare_storage::MetadataStore::new(db.clone());
    let mut file = rustshare_core::domain::File::new(
        "racing.md".into(),
        "/racing.md".into(),
        "racing-hash".into(),
        1,
        "text/markdown".into(),
        None,
        owner.id,
        tenant,
    );
    file.id = Uuid::new_v4();
    metadata.create_file(&file).await.unwrap();

    let mut source_lock = db.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text || ':' || $2::text, 0))")
        .bind(tenant)
        .bind(file.id)
        .execute(&mut *source_lock)
        .await
        .unwrap();
    let acl = NoteAclPayload {
        tenant_id: tenant,
        workspace_id: tenant,
        note_id: Uuid::new_v4(),
        source_file_id: file.id,
        source_folder_id: None,
        owner_id: owner.id,
        read_acl: vec![format!("owner:{}", owner.id)],
        visibility: "private".into(),
        acl_hash: "race-test".into(),
        acl_version: 1,
        embedding_policy: "allowed".into(),
    };
    let doc = IndexedDocument {
        file_id: file.id,
        file_name: file.name.clone(),
        file_path: file.path.clone(),
        content: "stale private body".into(),
        embedding: Vec::new(),
        mime_type: file.mime_type.clone(),
        owner_id: owner.id,
        tenant_id: tenant,
        indexed_at: chrono::Utc::now(),
        acl: Some(acl.clone()),
        chunk_id: file.id,
    };
    let store = Arc::new(PgVectorStore::new(db.clone()));
    let upsert_store = Arc::clone(&store);
    let upsert_acl = acl.clone();
    let upsert_doc = doc.clone();
    let upsert = tokio::spawn(async move {
        upsert_store
            .upsert_keyword_chunk(tenant, file.id, &upsert_doc, &upsert_acl)
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    metadata.delete_file(file.id, owner.id).await.unwrap();
    assert!(metadata
        .permanently_delete_file(file.id, owner.id)
        .await
        .unwrap());
    source_lock.commit().await.unwrap();
    upsert.await.unwrap().unwrap();
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM note_index_chunks WHERE tenant_id=$1 AND source_file_id=$2)"
    )
    .bind(tenant)
    .bind(file.id)
    .fetch_one(&db)
    .await
    .unwrap());
    cleanup_tenant(&db, tenant).await;
}

#[sqlx::test(migrations = "../migrations")]
#[ignore = "requires PostgreSQL, object storage, and migrations applied"]
async fn memory_orphan_cleanup_status_and_retry_are_tenant_scoped(db: PgPool) {
    use axum::{
        body::Body,
        http::Request,
        routing::{get, post},
        Router,
    };
    use rustshare_server::handlers::memory_orphan_cleanup::{get_status, retry};
    use support::calendar_harness::{
        cleanup_tenant, create_auth_token, create_test_tenant, create_test_user,
        setup_test_env_with_pool,
    };
    use tower::ServiceExt;

    let _guard = SERIAL.lock().await;
    let state = setup_test_env_with_pool(db.clone()).await;
    let tenant = create_test_tenant(&db).await;
    let foreign_tenant = create_test_tenant(&db).await;
    let admin = create_test_user(&state, "orphan_status_admin", tenant).await;
    let foreign_admin = create_test_user(&state, "orphan_status_foreign", foreign_tenant).await;
    let member = create_test_user(&state, "orphan_status_member", tenant).await;
    sqlx::query("UPDATE users SET is_admin=true WHERE id=ANY($1)")
        .bind([admin.id, foreign_admin.id].as_slice())
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_note_orphan_cleanup_jobs (tenant_id, scanned, candidates, removed, unresolved) VALUES ($1, 9, 3, 2, 1), ($2, 99, 99, 99, 99)")
        .bind(tenant)
        .bind(foreign_tenant)
        .execute(&db)
        .await
        .unwrap();
    let token = create_auth_token(&state, admin.id, tenant);
    let foreign_token = create_auth_token(&state, foreign_admin.id, foreign_tenant);
    let member_token = create_auth_token(&state, member.id, tenant);
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
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["enabled"], false);
    assert_eq!(body["dry_run"], true);
    assert_eq!(body["scanned"], 9);
    assert_eq!(body["candidates"], 3);
    assert_eq!(body["removed"], 2);
    assert_eq!(body["unresolved"], 1);
    let foreign_status = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/status?tenant_id={tenant}"))
                .header("authorization", format!("Bearer {foreign_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(foreign_status.status(), axum::http::StatusCode::OK);
    let foreign_body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(foreign_status.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(foreign_body["scanned"], 99);

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
        let expected = if method == "GET" {
            axum::http::StatusCode::OK
        } else {
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        };
        assert_eq!(response.status(), expected);
    }
    for (method, path) in [("GET", "/status"), ("POST", "/retry")] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", format!("Bearer {member_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
    }
    sqlx::query("DELETE FROM memory_note_orphan_cleanup_jobs WHERE tenant_id = ANY($1)")
        .bind([tenant, foreign_tenant].as_slice())
        .execute(&db)
        .await
        .unwrap();
    cleanup_tenant(&db, tenant).await;
    cleanup_tenant(&db, foreign_tenant).await;
}
