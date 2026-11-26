mod common;

use axum::{
    body::Body,
    extract::FromRef,
    http::{header, Request, StatusCode},
};
use rustshare_core::domain::{
    ApplicationId, ConnectorAuthority, ConnectorCapabilities, ConnectorDescriptor,
    ConnectorMetadata, ConnectorModes, ConnectorOperations, ConnectorPublications,
    ConnectorSource, CONNECTOR_API_VERSION,
};
use rustshare_resource_auth::ResourceRef;
use rustshare_server::{
    routes,
    services::connector_service::RegisterConnector,
};
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
    let other_shell = register_worker(
        &state,
        tenant_id,
        owner_id,
        SHELL_MEMORY_ID,
        Uuid::new_v4(),
    )
    .await;
    let local_text = register_worker(
        &state,
        tenant_id,
        owner_id,
        "io.elembra.connector.local-text",
        Uuid::new_v4(),
    )
    .await;
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

    sqlx::query("DELETE FROM files WHERE id=$1")
        .bind(foreign_file_id)
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

async fn forget(state: &rustshare_server::AppState, credential: &str, source_ref: &str) -> StatusCode {
    let app = routes::connector_routes().with_state(state.clone());
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/api/v1/connectors/worker/files/forget")
            .header(header::AUTHORIZATION, format!("Connector {credential}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"sourceResourceRef": source_ref, "sourceRevision": REVISION}).to_string(),
            ))
            .unwrap(),
    )
    .await
    .unwrap()
    .status()
}

fn source_ref(id: Uuid) -> String {
    ResourceRef::new(
        ApplicationId::new(SHELL_MEMORY_ID),
        "file",
        id.to_string(),
    )
    .to_uri()
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
