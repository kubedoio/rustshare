mod common;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use rustshare_core::domain::SharePermissions;
use rustshare_server::handlers::{
    extractors::AuthenticatedUser,
    groups::{self, CreateFileGroupShareRequest},
};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and RustFS"]
async fn group_share_http_handler_matches_permission_resolver() {
    let (state, _) = common::setup_test_server().await;
    let pool = state.db_pool.clone();
    let id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("Parity {id}"))
        .execute(&pool)
        .await
        .unwrap();
    let owner_id = insert_user(&pool, id, tenant_id, "owner").await;
    let member_id = insert_user(&pool, id, tenant_id, "member").await;
    let outsider_id = insert_user(&pool, id, tenant_id, "outsider").await;
    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("Group {id}"))
    .bind(tenant_id)
    .bind(owner_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
        .bind(group_id)
        .bind(member_id)
        .execute(&pool)
        .await
        .unwrap();
    let file_id = Uuid::new_v4();
    sqlx::query("INSERT INTO files (id, name, path, content_hash, size, mime_type, owner_id, tenant_id, current_version, storage_key) VALUES ($1,$2,$3,$4,0,$5,$6,$7,1,$8)")
        .bind(file_id).bind("parity.txt").bind(format!("/{id}.txt")).bind("hash")
        .bind("text/plain").bind(owner_id).bind(tenant_id).bind(format!("{tenant_id}/files/{file_id}"))
        .execute(&pool).await.unwrap();

    let owner_source = state
        .permission_resolver
        .check_file_permission(owner_id, tenant_id, file_id, SharePermissions::Admin)
        .await
        .unwrap();
    assert!(owner_source);
    let created = groups::create_file_group_share(
        State(state.clone()),
        AuthenticatedUser {
            user_id: owner_id,
            tenant_id,
        },
        Path(file_id),
        Json(CreateFileGroupShareRequest {
            group_id,
            permission: "View".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(created.0, StatusCode::CREATED);
    assert!(state
        .permission_resolver
        .check_file_permission(member_id, tenant_id, file_id, SharePermissions::View,)
        .await
        .unwrap());

    let outsider_source = state
        .permission_resolver
        .check_file_permission(outsider_id, tenant_id, file_id, SharePermissions::View)
        .await
        .unwrap();
    assert!(!outsider_source);
    assert!(groups::create_file_group_share(
        State(state.clone()),
        AuthenticatedUser {
            user_id: outsider_id,
            tenant_id
        },
        Path(file_id),
        Json(CreateFileGroupShareRequest {
            group_id,
            permission: "Admin".into()
        }),
    )
    .await
    .is_err());

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
        .bind(vec![owner_id, member_id, outsider_id])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();
}

async fn insert_user(pool: &sqlx::PgPool, id: Uuid, tenant_id: Uuid, role: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota) VALUES ($1,$2,$3,'hash',$4,$5,10737418240)")
        .bind(user_id).bind(format!("{role}{id}")).bind(format!("{role}{id}@example.test"))
        .bind(tenant_id).bind(role).execute(pool).await.unwrap();
    user_id
}
