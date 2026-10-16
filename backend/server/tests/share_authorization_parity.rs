//! HTTP share management and resource access use the same tenant-scoped decision.

mod common;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use rustshare_core::services::{PermissionSource, Resource};
use rustshare_server::handlers::{
    groups::{create_file_group_share, CreateFileGroupShareRequest},
    user_shares::{list_file_recipients, remove_recipient, update_recipient_permission},
    AuthenticatedUser,
};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and S3-compatible storage"]
async fn share_handlers_match_source_authorization_and_hide_foreign_records() {
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must point to disposable PostgreSQL");
    rustshare_server::test_guard::validate_disposable_database_url(&database_url)
        .expect("refusing to run against a non-disposable database");
    let bucket = std::env::var("S3_BUCKET").expect("S3_BUCKET must be a test bucket");
    assert!(
        bucket.starts_with("rustshare-test"),
        "unexpected bucket: {bucket}"
    );
    let endpoint = url::Url::parse(
        &std::env::var("S3_ENDPOINT").expect("S3_ENDPOINT must point to test storage"),
    )
    .expect("S3_ENDPOINT must be a URL");
    assert!(
        matches!(endpoint.host_str(), Some("127.0.0.1" | "localhost" | "::1")),
        "S3_ENDPOINT must be loopback"
    );

    let (state, _) = common::setup_test_server().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let foreign_tenant_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    let admin_id = Uuid::new_v4();
    let foreign_user_id = Uuid::new_v4();
    let group_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let admin_share_id = Uuid::new_v4();
    let malformed_share_id = Uuid::new_v4();
    let test_label = Uuid::new_v4();

    for (id, name) in [(tenant_id, "Local"), (foreign_tenant_id, "Foreign")] {
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(id)
            .bind(format!("{name} share auth {test_label}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    for (user_id, username, user_tenant) in [
        (owner_id, "owner", tenant_id),
        (admin_id, "admin", tenant_id),
        (foreign_user_id, "foreign", foreign_tenant_id),
    ] {
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota) VALUES ($1, $2, $3, 'test', $4, $2, 1048576)",
        )
        .bind(user_id)
        .bind(format!("{username}-{test_label}"))
        .bind(format!("{username}-{test_label}@example.test"))
        .bind(user_tenant)
        .execute(&pool)
        .await
        .unwrap();
    }
    let foreign_email = format!("foreign-{test_label}@example.test");
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("group-{test_label}"))
    .bind(tenant_id)
    .bind(owner_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id, tenant_id) VALUES ($1, 'auth.txt', $2, 1, 'text/plain', 'auth-hash', $3, $4, $5)",
    )
    .bind(file_id)
    .bind(format!("/{test_label}/auth.txt"))
    .bind(format!("{test_label}/auth.txt"))
    .bind(owner_id)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();
    for (share_id, recipient_id, permission) in [
        (admin_share_id, admin_id, "Admin"),
        (malformed_share_id, foreign_user_id, "View"),
    ] {
        sqlx::query(
            "INSERT INTO shares (id, file_id, recipient_user_id, permissions, created_by, tenant_id) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(share_id)
        .bind(file_id)
        .bind(recipient_id)
        .bind(permission)
        .bind(owner_id)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();
    }

    let auth = AuthenticatedUser {
        user_id: admin_id,
        tenant_id,
    };
    let source = state
        .permission_resolver
        .resolve_permission_with_source(admin_id, tenant_id, Resource::File(file_id))
        .await
        .unwrap();
    assert_eq!(
        source.permission,
        Some(rustshare_core::domain::SharePermissions::Admin)
    );
    assert_eq!(source.source, PermissionSource::DirectShare);

    let created = create_file_group_share(
        State(state.clone()),
        auth.clone(),
        Path(file_id),
        Json(CreateFileGroupShareRequest {
            group_id,
            permission: "Edit".to_string(),
        }),
    )
    .await
    .expect("HTTP share management must accept the source Admin grant");
    assert_eq!(created.0, StatusCode::CREATED);
    let foreign = create_file_group_share(
        State(state.clone()),
        AuthenticatedUser {
            user_id: admin_id,
            tenant_id: foreign_tenant_id,
        },
        Path(file_id),
        Json(CreateFileGroupShareRequest {
            group_id,
            permission: "View".to_string(),
        }),
    )
    .await
    .expect_err("Foreign tenant resource must be hidden");
    assert_eq!(foreign.into_response().status(), StatusCode::NOT_FOUND);
    let missing = create_file_group_share(
        State(state.clone()),
        auth.clone(),
        Path(Uuid::new_v4()),
        Json(CreateFileGroupShareRequest {
            group_id,
            permission: "View".to_string(),
        }),
    )
    .await
    .expect_err("Missing resource must be hidden");
    assert_eq!(missing.into_response().status(), StatusCode::NOT_FOUND);

    let response = list_file_recipients(State(state.clone()), Path(file_id), auth.clone())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let recipients: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(recipients.iter().all(|recipient| {
        recipient.get("email").and_then(serde_json::Value::as_str) != Some(foreign_email.as_str())
    }));

    let update = update_recipient_permission(
        State(state.clone()),
        Path(malformed_share_id),
        auth.clone(),
        Json(
            rustshare_server::handlers::user_shares::UpdatePermissionRequest {
                permission: rustshare_core::domain::SharePermissions::Edit,
            },
        ),
    )
    .await
    .expect_err("Foreign recipient update must be hidden");
    assert_eq!(update.into_response().status(), StatusCode::NOT_FOUND);
    let remove = remove_recipient(State(state.clone()), Path(malformed_share_id), auth)
        .await
        .expect_err("Foreign recipient removal must be hidden");
    assert_eq!(remove.into_response().status(), StatusCode::NOT_FOUND);
    let still_active: bool =
        sqlx::query_scalar("SELECT revoked_at IS NULL FROM shares WHERE id = $1")
            .bind(malformed_share_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        still_active,
        "Malformed foreign-recipient share must stay untouched"
    );
    assert_eq!(
        state
            .permission_resolver
            .resolve_permission_with_source(admin_id, tenant_id, Resource::File(file_id))
            .await
            .unwrap()
            .permission,
        Some(rustshare_core::domain::SharePermissions::Admin),
        "Malformed recipient filtering must preserve the actor's valid Admin source"
    );

    sqlx::query("DELETE FROM files WHERE id = $1")
        .bind(file_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM user_groups WHERE id = $1")
        .bind(group_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = ANY($1)")
        .bind(vec![owner_id, admin_id, foreign_user_id])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tenants WHERE id = ANY($1)")
        .bind(vec![tenant_id, foreign_tenant_id])
        .execute(&pool)
        .await
        .unwrap();
}
