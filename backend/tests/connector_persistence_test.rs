//! Persistent Connector lifecycle contract against disposable PostgreSQL.
//! Run with `DATABASE_URL=postgres://.../rustshare_test` and `--ignored`.

use rustshare_server::services::connector_service::ConnectorService;
use rustshare_storage::{ConnectorMutation, ConnectorRegistration, ConnectorStore};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

fn assert_disposable_database(url: &str) {
    let parsed = url::Url::parse(url).expect("DATABASE_URL must be valid");
    assert!(
        matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
            && parsed
                .path()
                .trim_start_matches('/')
                .starts_with("rustshare_test"),
        "Connector persistence tests only run against a local rustshare_test database"
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and migrations"]
async fn registration_is_idempotent_tenant_scoped_and_lifecycle_is_fenced() {
    let url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL to a local disposable rustshare_test database");
    assert_disposable_database(&url);
    let pool = PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../migrations").run(&pool).await.unwrap();

    let tenant_id = Uuid::new_v4();
    let other_tenant_id = Uuid::new_v4();
    let principal_id = Uuid::new_v4();
    let other_principal_id = Uuid::new_v4();
    for id in [tenant_id, other_tenant_id] {
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'connector test')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    for (id, tenant_id) in [
        (principal_id, tenant_id),
        (other_principal_id, other_tenant_id),
    ] {
        sqlx::query(
            "INSERT INTO users (id, tenant_id, username, email, password_hash, display_name, storage_quota) VALUES ($1, $2, $3, $4, 'unused', 'Connector test', 0)",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(format!("connector-{}", id.simple()))
        .bind(format!("connector-{}@example.invalid", id.simple()))
        .execute(&pool)
        .await
        .unwrap();
    }

    let store = ConnectorStore::new(pool.clone());
    let descriptor = json!({"apiVersion":"elembra.io/v1alpha1","kind":"Connector"});
    let config = json!({"roots":["/notes"]});
    let digest: [u8; 32] = Sha256::digest(b"request").into();
    let (first, created) = store
        .register(ConnectorRegistration {
            tenant_id,
            principal_id,
            connector_id: "io.elembra.connector.local-text",
            descriptor: &descriptor,
            source_config: &config,
            credential_ref: None,
            mode: "import",
            idempotency_key: "request-1",
            request_digest: &digest,
        })
        .await
        .unwrap();
    assert!(created);

    let (retry, created) = store
        .register(ConnectorRegistration {
            tenant_id,
            principal_id,
            connector_id: "io.elembra.connector.local-text",
            descriptor: &descriptor,
            source_config: &config,
            credential_ref: None,
            mode: "import",
            idempotency_key: "request-1",
            request_digest: &digest,
        })
        .await
        .unwrap();
    assert!(!created);
    assert_eq!(first.id, retry.id);
    let changed_digest: [u8; 32] = Sha256::digest(b"different request").into();
    assert!(store
        .register(ConnectorRegistration {
            tenant_id,
            principal_id,
            connector_id: "io.elembra.connector.local-text",
            descriptor: &descriptor,
            source_config: &config,
            credential_ref: None,
            mode: "import",
            idempotency_key: "request-1",
            request_digest: &changed_digest,
        })
        .await
        .is_err());
    assert!(store
        .get(other_tenant_id, other_principal_id, first.id)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .get(tenant_id, other_principal_id, first.id)
        .await
        .unwrap()
        .is_none());

    let worker_token = b"one-time-worker-secret";
    let worker_hash: [u8; 32] = Sha256::digest(worker_token).into();
    store
        .rotate_worker_credential(
            tenant_id,
            principal_id,
            first.id,
            Uuid::new_v4(),
            &worker_hash,
            chrono::Utc::now() + chrono::Duration::days(1),
        )
        .await
        .unwrap();
    let authenticated = store
        .authenticate_worker_credential(&worker_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authenticated.identity.tenant_id, tenant_id);
    assert_eq!(authenticated.identity.principal_id, principal_id);
    assert_eq!(authenticated.identity.instance_id, first.id);

    let replacement_token = b"rotated-worker-secret";
    let replacement_hash: [u8; 32] = Sha256::digest(replacement_token).into();
    store
        .rotate_worker_credential(
            tenant_id,
            principal_id,
            first.id,
            Uuid::new_v4(),
            &replacement_hash,
            chrono::Utc::now() + chrono::Duration::days(1),
        )
        .await
        .unwrap();
    assert!(store
        .authenticate_worker_credential(&worker_hash)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .authenticate_worker_credential(&replacement_hash)
        .await
        .unwrap()
        .is_some());
    assert!(store
        .authenticate_worker_credential(&Sha256::digest(b"unknown-tenant-token").into())
        .await
        .unwrap()
        .is_none());

    let second_digest: [u8; 32] = Sha256::digest(b"request 2").into();
    let (concurrent, created) = store
        .register(ConnectorRegistration {
            tenant_id,
            principal_id,
            connector_id: "io.elembra.connector.local-text",
            descriptor: &descriptor,
            source_config: &config,
            credential_ref: None,
            mode: "import",
            idempotency_key: "request-2",
            request_digest: &second_digest,
        })
        .await
        .unwrap();
    assert!(created);
    let service = ConnectorService::new(Arc::new(store.clone()));
    let issued = service
        .rotate_worker_credential(tenant_id, principal_id, concurrent.id)
        .await
        .unwrap();
    assert!(issued.credential.starts_with("ecw1_"));
    let worker = service
        .authenticate_worker(&issued.credential)
        .await
        .unwrap();
    assert_eq!(worker.context.principal_id.0, concurrent.id);
    assert_eq!(
        worker
            .context
            .delegation
            .as_ref()
            .map(|delegation| delegation.issuer_principal_id.0),
        Some(principal_id)
    );
    assert_eq!(
        worker
            .context
            .workload_identity
            .as_ref()
            .and_then(|identity| identity.subject.as_deref()),
        Some(format!("connector-instance:{}", concurrent.id).as_str())
    );
    let safe_view = serde_json::to_value(&worker.instance).unwrap();
    assert!(safe_view.get("credentialRef").is_none());
    let rotated = service
        .rotate_worker_credential(tenant_id, principal_id, concurrent.id)
        .await
        .unwrap();
    assert!(service
        .authenticate_worker(&issued.credential)
        .await
        .is_err());
    assert!(service
        .authenticate_worker(&rotated.credential)
        .await
        .is_ok());
    let (rotation_a, rotation_b) = tokio::join!(
        service.rotate_worker_credential(tenant_id, principal_id, concurrent.id),
        service.rotate_worker_credential(tenant_id, principal_id, concurrent.id),
    );
    let rotation_a = rotation_a.unwrap();
    let rotation_b = rotation_b.unwrap();
    let (valid_a, valid_b) = tokio::join!(
        service.authenticate_worker(&rotation_a.credential),
        service.authenticate_worker(&rotation_b.credential),
    );
    assert_ne!(valid_a.is_ok(), valid_b.is_ok());
    let expired_hash: [u8; 32] = Sha256::digest(b"expired-worker-secret").into();
    store
        .rotate_worker_credential(
            tenant_id,
            principal_id,
            concurrent.id,
            Uuid::new_v4(),
            &expired_hash,
            chrono::Utc::now() - chrono::Duration::seconds(1),
        )
        .await
        .unwrap();
    assert!(store
        .authenticate_worker_credential(&expired_hash)
        .await
        .unwrap()
        .is_none());
    let (first_writer, second_writer) = tokio::join!(
        store.mutate(ConnectorMutation {
            tenant_id,
            principal_id,
            id: concurrent.id,
            expected_version: 1,
            source_config: None,
            credential_ref: None,
            lifecycle_state: Some("paused"),
        }),
        store.mutate(ConnectorMutation {
            tenant_id,
            principal_id,
            id: concurrent.id,
            expected_version: 1,
            source_config: None,
            credential_ref: None,
            lifecycle_state: Some("active"),
        })
    );
    assert_ne!(
        first_writer.unwrap().is_some(),
        second_writer.unwrap().is_some(),
        "only one concurrent mutation may claim a version"
    );

    assert!(store
        .record_mapping(
            tenant_id,
            first.id,
            "elembra://io.elembra.connector.local-text/file/a.md",
            "sha256:abc",
            "elembra://io.elembra.files/file/target-1",
        )
        .await
        .unwrap());
    assert!(!store
        .record_mapping(
            tenant_id,
            first.id,
            "elembra://io.elembra.connector.local-text/file/a.md",
            "sha256:abc",
            "elembra://io.elembra.files/file/target-1",
        )
        .await
        .unwrap());
    assert_eq!(
        store
            .mapped_target(
                tenant_id,
                first.id,
                "elembra://io.elembra.connector.local-text/file/a.md",
                "sha256:abc",
            )
            .await
            .unwrap()
            .as_deref(),
        Some("elembra://io.elembra.files/file/target-1")
    );

    let paused = store
        .mutate(ConnectorMutation {
            tenant_id,
            principal_id,
            id: first.id,
            expected_version: 1,
            source_config: None,
            credential_ref: None,
            lifecycle_state: Some("paused"),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(paused.lifecycle_state, "paused");
    assert!(store
        .authenticate_worker_credential(&replacement_hash)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .mutate(ConnectorMutation {
            tenant_id,
            principal_id,
            id: first.id,
            expected_version: 1,
            source_config: None,
            credential_ref: None,
            lifecycle_state: Some("active"),
        })
        .await
        .unwrap()
        .is_none());

    let resumed = store
        .mutate(ConnectorMutation {
            tenant_id,
            principal_id,
            id: first.id,
            expected_version: 2,
            source_config: None,
            credential_ref: None,
            lifecycle_state: Some("active"),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resumed.lifecycle_state, "active");
    assert!(store
        .authenticate_worker_credential(&replacement_hash)
        .await
        .unwrap()
        .is_some());
    let disconnected = store
        .mutate(ConnectorMutation {
            tenant_id,
            principal_id,
            id: first.id,
            expected_version: 3,
            source_config: None,
            credential_ref: None,
            lifecycle_state: Some("disconnected"),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(disconnected.lifecycle_state, "disconnected");
    assert!(store
        .authenticate_worker_credential(&replacement_hash)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .mutate(ConnectorMutation {
            tenant_id,
            principal_id,
            id: first.id,
            expected_version: 4,
            source_config: None,
            credential_ref: None,
            lifecycle_state: Some("active"),
        })
        .await
        .unwrap()
        .is_none());

    sqlx::query("DELETE FROM tenants WHERE id = ANY($1)")
        .bind(vec![tenant_id, other_tenant_id])
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}
