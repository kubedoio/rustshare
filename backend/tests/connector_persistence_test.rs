//! Persistent Connector lifecycle contract against disposable PostgreSQL.
//! Run with `DATABASE_URL=postgres://.../rustshare_test` and `--ignored`.

use rustshare_storage::{ConnectorMutation, ConnectorRegistration, ConnectorStore};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
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
