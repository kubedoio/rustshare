mod common;

use axum::{
    body::{to_bytes, Body},
    extract::FromRef,
    http::{header, Request, StatusCode},
};
use rustshare_core::domain::{
    ApplicationId, ConnectorAuthority, ConnectorCapabilities, ConnectorDescriptor,
    ConnectorMetadata, ConnectorModes, ConnectorOperations, ConnectorPublications, ConnectorSource,
    CONNECTOR_API_VERSION,
};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_resource_auth::{PrincipalContext, Representation, ResourceRef, SourceError};
use rustshare_server::{routes, services::connector_service::RegisterConnector};
use rustshare_storage::{ConnectorMappingProvenance, ConnectorStore};
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SHELL_MEMORY_ID: &str = "io.elembra.connector.shell-memory";
const REVISION: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and RustFS"]
async fn forget_denies_foreign_tenant_instance_and_connector_targets() {
    let (state, _) = common::setup_test_server().await;
    let pool = state.db_pool.clone();
    let fixture = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let foreign_tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1,$2),($3,$4)")
        .bind(tenant_id)
        .bind(format!("forget-{fixture}"))
        .bind(foreign_tenant_id)
        .bind(format!("forget-foreign-{fixture}"))
        .execute(&pool)
        .await
        .unwrap();
    let owner_id = insert_user(&pool, fixture, tenant_id).await;
    let foreign_owner_id = insert_user(&pool, fixture, foreign_tenant_id).await;
    let primary = register_worker(&state, tenant_id, owner_id, SHELL_MEMORY_ID, fixture).await;
    let other_shell =
        register_worker(&state, tenant_id, owner_id, SHELL_MEMORY_ID, Uuid::new_v4()).await;
    let local_text = register_worker(
        &state,
        tenant_id,
        owner_id,
        "io.elembra.connector.local-text",
        Uuid::new_v4(),
    )
    .await;
    let owned_file_id = insert_file(&pool, owner_id, tenant_id, Uuid::new_v4()).await;
    let unrelated_owned_file_id = insert_file(&pool, owner_id, tenant_id, Uuid::new_v4()).await;
    let owned_source = source_ref(Uuid::new_v4());
    let owned_target = target_ref(owned_file_id);
    let unrelated_source = source_ref(Uuid::new_v4());
    let unrelated_target = target_ref(unrelated_owned_file_id);
    let store = ConnectorStore::new(pool.clone());
    store
        .record_mapping(
            tenant_id,
            Uuid::from(primary.instance_id),
            &owned_source,
            REVISION,
            &owned_target,
            ConnectorMappingProvenance::default(),
        )
        .await
        .unwrap();
    store
        .record_mapping(
            tenant_id,
            Uuid::from(primary.instance_id),
            &unrelated_source,
            REVISION,
            &unrelated_target,
            ConnectorMappingProvenance::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        forget(&state, &primary.credential, &owned_source).await,
        StatusCode::NO_CONTENT
    );
    assert!(!file_is_active(&pool, owned_file_id).await);
    // Simulate a stale projection surviving or arriving after index cleanup;
    // current Files authorization must still prevent it reaching Memory.
    sqlx::query("INSERT INTO note_index_chunks (id,tenant_id,note_id,source_file_id,file_name,file_path,content,mime_type,owner_id,workspace_id) VALUES ($1,$2,$3,$3,'stale.md','/stale.md','synthetic stale secret','text/markdown',$4,$2)")
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(owned_file_id)
        .bind(owner_id)
        .execute(&pool)
        .await
        .unwrap();
    let stale_chunks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM note_index_chunks WHERE tenant_id=$1 AND source_file_id=$2 AND content='synthetic stale secret'",
    )
    .bind(tenant_id)
    .bind(owned_file_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stale_chunks, 1, "test retains the stale index projection");
    let source = ResourceRef::new(
        ApplicationId::new("io.elembra.files"),
        "file",
        owned_file_id.to_string(),
    );
    let principal = PrincipalContext::user(
        PrincipalId(owner_id),
        TenantId(tenant_id),
        WorkspaceId(tenant_id),
    );
    assert!(
        matches!(
            state
                .source_authorizer
                .fetch(&principal, &source, Representation::Text)
                .await,
            Err(SourceError::NotFound | SourceError::Unauthorized)
        ),
        "a stale chunk cannot be materialized without current Files authorization"
    );
    assert!(file_is_active(&pool, unrelated_owned_file_id).await);
    let mismatched_revision = format!("sha256:{}", "b".repeat(64));
    assert_eq!(
        forget_with_revision(
            &state,
            &primary.credential,
            &unrelated_source,
            &mismatched_revision,
        )
        .await,
        StatusCode::NO_CONTENT
    );
    assert!(
        file_is_active(&pool, unrelated_owned_file_id).await,
        "an exact-revision mismatch cannot delete the mapped File"
    );
    let mismatched_mapping_active: bool = sqlx::query_scalar(
        "SELECT forgotten_at IS NULL FROM connector_external_mappings WHERE tenant_id=$1 AND instance_id=$2 AND external_resource_ref=$3 AND external_revision=$4",
    )
    .bind(tenant_id)
    .bind(Uuid::from(primary.instance_id))
    .bind(&unrelated_source)
    .bind(REVISION)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(mismatched_mapping_active);
    let forgotten_mapping: bool = sqlx::query_scalar(
        "SELECT forget_requested_at IS NOT NULL AND forgotten_at IS NOT NULL FROM connector_external_mappings WHERE tenant_id=$1 AND instance_id=$2 AND external_resource_ref=$3 AND external_revision=$4",
    )
    .bind(tenant_id)
    .bind(Uuid::from(primary.instance_id))
    .bind(&owned_source)
    .bind(REVISION)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(forgotten_mapping);
    let delete_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM events WHERE aggregate_id=$1 AND event_type=$2 AND user_id=$3",
    )
    .bind(owned_file_id)
    .bind("\"FileDeleted\"")
    .bind(owner_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        delete_events, 1,
        "Files lifecycle event is durable and owner-attributed"
    );
    assert_eq!(
        forget(&state, &primary.credential, &owned_source).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM events WHERE aggregate_id=$1 AND event_type=$2 AND user_id=$3",
        )
        .bind(owned_file_id)
        .bind("\"FileDeleted\"")
        .bind(owner_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1,
        "repeated forget must not append another delete event"
    );
    assert!(state
        .metadata_store
        .restore_file(owned_file_id, owner_id, tenant_id)
        .await
        .unwrap());
    assert_eq!(
        forget(&state, &primary.credential, &owned_source).await,
        StatusCode::NO_CONTENT
    );
    assert!(file_is_active(&pool, owned_file_id).await);

    // An unknown source forget is retained as a targetless tombstone. A later
    // replay checks it before creating a Files object.
    let never_imported_source = source_ref(Uuid::new_v4());
    assert_eq!(
        forget(&state, &primary.credential, &never_imported_source).await,
        StatusCode::NO_CONTENT
    );
    let tombstone: (Option<String>, bool) = sqlx::query_as(
        "SELECT target_resource_ref, forgotten_at IS NOT NULL FROM connector_external_mappings WHERE tenant_id=$1 AND instance_id=$2 AND external_resource_ref=$3 AND external_revision=$4",
    )
    .bind(tenant_id)
    .bind(Uuid::from(primary.instance_id))
    .bind(&never_imported_source)
    .bind(REVISION)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tombstone, (None, true));
    let never_imported_path = format!("never-imported-{}.md", Uuid::new_v4());
    let replay = import_source(
        &state,
        &primary.credential,
        &never_imported_source,
        &never_imported_path,
    )
    .await;
    assert_eq!(replay.status, StatusCode::OK);
    let replay_json: serde_json::Value = serde_json::from_slice(&replay.body).unwrap();
    assert_eq!(replay_json["forgotten"], true);
    assert!(replay_json["targetResourceRef"].is_null());
    let late_file_count: i64 = sqlx::query_scalar("SELECT count(*) FROM files WHERE path=$1")
        .bind(format!("/{never_imported_path}"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        late_file_count, 0,
        "tombstoned replay creates no Files object"
    );
    let late_mapping = store
        .record_mapping(
            tenant_id,
            Uuid::from(primary.instance_id),
            &never_imported_source,
            REVISION,
            &owned_target,
            ConnectorMappingProvenance::default(),
        )
        .await;
    assert!(
        late_mapping.is_err(),
        "a late mapping cannot replace a forget tombstone"
    );
    let target_after_replay: Option<String> = sqlx::query_scalar(
        "SELECT target_resource_ref FROM connector_external_mappings WHERE tenant_id=$1 AND instance_id=$2 AND external_resource_ref=$3 AND external_revision=$4",
    )
    .bind(tenant_id)
    .bind(Uuid::from(primary.instance_id))
    .bind(&never_imported_source)
    .bind(REVISION)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(target_after_replay.is_none());

    // If import owns the source lock first, forget waits for its mapping to
    // commit, then deletes the resulting Files target.
    let delayed_source = source_ref(Uuid::new_v4());
    let delayed_file_id = insert_file(&pool, owner_id, tenant_id, Uuid::new_v4()).await;
    let delayed_target = target_ref(delayed_file_id);
    let import_lock = store
        .lock_mapping_scope(
            tenant_id,
            Uuid::from(primary.instance_id),
            &delayed_source,
            REVISION,
        )
        .await
        .unwrap();
    store
        .record_mapping(
            tenant_id,
            Uuid::from(primary.instance_id),
            &delayed_source,
            REVISION,
            &delayed_target,
            ConnectorMappingProvenance::default(),
        )
        .await
        .unwrap();
    let delayed_state = state.clone();
    let delayed_credential = primary.credential.clone();
    let delayed_source_for_request = delayed_source.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let forget_task = tokio::spawn(async move {
        let _ = started_tx.send(());
        forget(
            &delayed_state,
            &delayed_credential,
            &delayed_source_for_request,
        )
        .await
    });
    started_rx.await.unwrap();
    wait_for_advisory_lock_wait(&pool).await;
    drop(import_lock);
    assert_eq!(forget_task.await.unwrap(), StatusCode::NO_CONTENT);
    assert!(!file_is_active(&pool, delayed_file_id).await);

    let foreign_file_id = insert_file(&pool, foreign_owner_id, foreign_tenant_id, fixture).await;
    let foreign_source = source_ref(Uuid::new_v4());
    let foreign_target = target_ref(foreign_file_id);
    let store = ConnectorStore::new(pool.clone());
    store
        .record_mapping(
            tenant_id,
            Uuid::from(primary.instance_id),
            &foreign_source,
            REVISION,
            &foreign_target,
            ConnectorMappingProvenance::default(),
        )
        .await
        .unwrap();
    let own_scope_source = source_ref(Uuid::new_v4());
    store
        .record_mapping(
            tenant_id,
            Uuid::from(other_shell.instance_id),
            &own_scope_source,
            REVISION,
            &foreign_target,
            ConnectorMappingProvenance::default(),
        )
        .await
        .unwrap();
    store
        .record_mapping(
            tenant_id,
            Uuid::from(primary.instance_id),
            &source_ref(Uuid::new_v4()),
            REVISION,
            &foreign_target,
            ConnectorMappingProvenance::default(),
        )
        .await
        .unwrap();

    // A mapped target in a different tenant is indistinguishable from a
    // missing Files resource to this worker and remains active.
    assert_eq!(
        forget(&state, &primary.credential, &foreign_source).await,
        StatusCode::NO_CONTENT
    );
    assert!(file_is_active(&pool, foreign_file_id).await);

    // A source mapped under another Shell Memory instance is not visible in
    // this instance's mapping scope and cannot affect its target.
    assert_eq!(
        forget(&state, &primary.credential, &own_scope_source).await,
        StatusCode::NO_CONTENT
    );
    assert!(file_is_active(&pool, foreign_file_id).await);
    let other_instance_mapping_active: bool = sqlx::query_scalar(
        "SELECT forgotten_at IS NULL FROM connector_external_mappings WHERE tenant_id=$1 AND instance_id=$2 AND external_resource_ref=$3 AND external_revision=$4",
    )
    .bind(tenant_id)
    .bind(Uuid::from(other_shell.instance_id))
    .bind(&own_scope_source)
    .bind(REVISION)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(other_instance_mapping_active);

    // A different Connector implementation cannot call Shell Memory forget.
    assert_eq!(
        forget(&state, &local_text.credential, &foreign_source).await,
        StatusCode::NOT_FOUND
    );
    assert!(file_is_active(&pool, foreign_file_id).await);

    sqlx::query("UPDATE connector_instances SET lifecycle_state='paused' WHERE id=$1")
        .bind(Uuid::from(primary.instance_id))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        forget(&state, &primary.credential, &foreign_source).await,
        StatusCode::UNAUTHORIZED
    );
    assert!(file_is_active(&pool, foreign_file_id).await);
    sqlx::query("UPDATE connector_instances SET lifecycle_state='active' WHERE id=$1")
        .bind(Uuid::from(primary.instance_id))
        .execute(&pool)
        .await
        .unwrap();
    let database = rustshare_server::state::DatabaseState::from_ref(&state);
    let rotated = database
        .connector_service
        .rotate_worker_credential(tenant_id, owner_id, Uuid::from(primary.instance_id))
        .await
        .unwrap();
    assert_eq!(
        forget(&state, &primary.credential, &unrelated_source).await,
        StatusCode::UNAUTHORIZED,
        "rotated credentials cannot invoke the old worker token"
    );
    assert!(file_is_active(&pool, unrelated_owned_file_id).await);
    assert_eq!(
        forget(&state, &rotated.credential, &unrelated_source).await,
        StatusCode::NO_CONTENT
    );
    assert!(!file_is_active(&pool, unrelated_owned_file_id).await);

    sqlx::query("DELETE FROM files WHERE id=$1")
        .bind(foreign_file_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM files WHERE id=$1")
        .bind(owned_file_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM files WHERE id=$1")
        .bind(unrelated_owned_file_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tenants WHERE id=ANY($1)")
        .bind(vec![tenant_id, foreign_tenant_id])
        .execute(&pool)
        .await
        .unwrap();
}

struct Worker {
    instance_id: Uuid,
    credential: String,
}

async fn register_worker(
    state: &rustshare_server::AppState,
    tenant_id: Uuid,
    owner_id: Uuid,
    connector_id: &str,
    key: Uuid,
) -> Worker {
    let database = rustshare_server::state::DatabaseState::from_ref(state);
    let descriptor = ConnectorDescriptor {
        api_version: CONNECTOR_API_VERSION.into(),
        kind: "Connector".into(),
        metadata: ConnectorMetadata {
            id: connector_id.into(),
            name: "Forget test Connector".into(),
            version: "1.0.0".into(),
        },
        source: ConnectorSource {
            system: "shell-history".into(),
            authority: ConnectorAuthority::Local,
        },
        supports: ConnectorCapabilities {
            operations: ConnectorOperations {
                read: true,
                ..Default::default()
            },
            modes: ConnectorModes {
                import: true,
                ..Default::default()
            },
        },
        publishes: ConnectorPublications::default(),
    };
    let (instance, _) = database
        .connector_service
        .register(
            tenant_id,
            owner_id,
            &format!("forget-test-{key}"),
            RegisterConnector {
                descriptor,
                source_config: json!({}),
                credential_ref: None,
                mode: "import".into(),
            },
        )
        .await
        .unwrap();
    let issued = database
        .connector_service
        .rotate_worker_credential(tenant_id, owner_id, Uuid::from(instance.id))
        .await
        .unwrap();
    Worker {
        instance_id: Uuid::from(instance.id),
        credential: issued.credential,
    }
}

async fn forget(
    state: &rustshare_server::AppState,
    credential: &str,
    source_ref: &str,
) -> StatusCode {
    forget_with_revision(state, credential, source_ref, REVISION).await
}

async fn forget_with_revision(
    state: &rustshare_server::AppState,
    credential: &str,
    source_ref: &str,
    source_revision: &str,
) -> StatusCode {
    let app = routes::connector_routes().with_state(state.clone());
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/api/v1/connectors/worker/files/forget")
            .header(header::AUTHORIZATION, format!("Connector {credential}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"sourceResourceRef": source_ref, "sourceRevision": source_revision})
                    .to_string(),
            ))
            .unwrap(),
    )
    .await
    .unwrap()
    .status()
}

async fn wait_for_advisory_lock_wait(pool: &PgPool) {
    for _ in 0..100 {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND wait_event='advisory' AND query LIKE '%pg_advisory_xact_lock%')",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        if waiting {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("forget request never reached the held source-revision advisory lock");
}

struct ImportResponse {
    status: StatusCode,
    body: Vec<u8>,
}

async fn import_source(
    state: &rustshare_server::AppState,
    credential: &str,
    source_ref: &str,
    relative_path: &str,
) -> ImportResponse {
    let app = routes::connector_routes().with_state(state.clone());
    let lease_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/connectors/worker/lease")
                .header(header::AUTHORIZATION, format!("Connector {credential}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(lease_response.status(), StatusCode::OK);
    let lease_body = to_bytes(lease_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let lease: serde_json::Value = serde_json::from_slice(&lease_body).unwrap();
    let lease = lease
        .as_object()
        .expect("worker receives an execution lease");
    let boundary = "synthetic-shell-import-boundary";
    let metadata = json!({
        "leaseId": lease["leaseId"],
        "fence": lease["fence"],
        "sourceResourceRef": source_ref,
        "sourceRevision": REVISION,
        "relativePath": relative_path,
        "observedModifiedAt": chrono::Utc::now(),
        "parentFolderId": null
    });
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"metadata\"\r\nContent-Type: application/json\r\n\r\n{}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"synthetic.md\"\r\nContent-Type: text/markdown\r\n\r\nsynthetic shell memory content\r\n--{boundary}--\r\n",
        metadata
    );
    let response = routes::connector_routes()
        .with_state(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/connectors/worker/files/import")
                .header(header::AUTHORIZATION, format!("Connector {credential}"))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    ImportResponse { status, body }
}

fn source_ref(id: Uuid) -> String {
    ResourceRef::new(ApplicationId::new(SHELL_MEMORY_ID), "file", id.to_string()).to_uri()
}

fn target_ref(id: Uuid) -> String {
    ResourceRef::new(
        ApplicationId::new("io.elembra.files"),
        "file",
        id.to_string(),
    )
    .with_version(REVISION.into())
    .to_uri()
}

async fn insert_user(pool: &PgPool, id: Uuid, tenant_id: Uuid) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id,username,email,password_hash,tenant_id,display_name,storage_quota) VALUES ($1,$2,$3,'hash',$4,'owner',10737418240)")
        .bind(user_id)
        .bind(format!("forget-user-{id}-{user_id}"))
        .bind(format!("forget-{user_id}@example.test"))
        .bind(tenant_id)
        .execute(pool)
        .await
        .unwrap();
    user_id
}

async fn insert_file(pool: &PgPool, owner_id: Uuid, tenant_id: Uuid, id: Uuid) -> Uuid {
    let file_id = Uuid::new_v4();
    sqlx::query("INSERT INTO files (id,name,path,size,mime_type,content_hash,storage_key,owner_id,tenant_id,current_version) VALUES ($1,'shell.md',$2,1,'text/markdown',$3,$4,$5,$6,1)")
        .bind(file_id)
        .bind(format!("/forget-{id}.md"))
        .bind(&REVISION[7..])
        .bind(format!("{tenant_id}/files/{file_id}"))
        .bind(owner_id)
        .bind(tenant_id)
        .execute(pool)
        .await
        .unwrap();
    file_id
}

async fn file_is_active(pool: &PgPool, file_id: Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT deleted_at IS NULL FROM files WHERE id=$1")
        .bind(file_id)
        .fetch_one(pool)
        .await
        .unwrap()
}
