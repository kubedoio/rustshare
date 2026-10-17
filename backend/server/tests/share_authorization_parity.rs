mod common;

use axum::{
    body::{to_bytes, Body},
    extract::{Path, State},
    http::{header, Request, StatusCode},
    Json,
};
use rustshare_core::domain::SharePermissions;
use rustshare_core::services::{PermissionResult, PermissionSource, Resource};
use rustshare_server::handlers::{
    extractors::AuthenticatedUser,
    groups::{self, CreateFileGroupShareRequest},
};
use tower::ServiceExt;
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

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and RustFS"]
async fn permission_explanations_match_live_resolution_and_hide_private_grants() {
    let (state, _) = common::setup_test_server().await;
    let pool = state.db_pool.clone();
    let fixture_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("Explanation {fixture_id}"))
        .execute(&pool)
        .await
        .unwrap();

    let owner_id = insert_user(&pool, fixture_id, tenant_id, "explain-owner").await;
    let member_id = insert_user(&pool, fixture_id, tenant_id, "explain-member").await;
    let grant_admin_id = insert_user(&pool, fixture_id, tenant_id, "explain-admin").await;
    let outsider_id = insert_user(&pool, fixture_id, tenant_id, "explain-outsider").await;
    let folder = state
        .folder_service
        .create_folder(
            format!("explanation-{fixture_id}"),
            None,
            owner_id,
            tenant_id,
        )
        .await
        .unwrap();
    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("explanation-group-{fixture_id}"))
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
    sqlx::query("INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id, parent_folder_id, tenant_id, current_version) VALUES ($1, $2, $3, 0, 'text/plain', 'hash', $4, $5, $6, $7, 1)")
        .bind(file_id)
        .bind("inherited.txt")
        .bind(format!("/{fixture_id}/inherited.txt"))
        .bind(format!("{tenant_id}/files/{file_id}"))
        .bind(owner_id)
        .bind(folder.id)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    let group_share_id = insert_folder_share(
        &pool,
        folder.id,
        owner_id,
        tenant_id,
        "View",
        None,
        Some(group_id),
        None,
    )
    .await;
    let unrelated_group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(unrelated_group_id)
    .bind(format!("unrelated-group-{fixture_id}"))
    .bind(tenant_id)
    .bind(owner_id)
    .execute(&pool)
    .await
    .unwrap();
    let unrelated_share_id = insert_folder_share(
        &pool,
        folder.id,
        owner_id,
        tenant_id,
        "Admin",
        None,
        Some(unrelated_group_id),
        None,
    )
    .await;
    let admin_share_id = insert_folder_share(
        &pool,
        folder.id,
        owner_id,
        tenant_id,
        "Admin",
        Some(grant_admin_id),
        None,
        None,
    )
    .await;

    let (status, member_body) = folder_contents_http(&state, member_id, tenant_id, folder.id).await;
    assert_eq!(status, StatusCode::OK);
    let current = &member_body["current_folder_access"];
    assert_eq!(current["source"], "group_share");
    assert_eq!(current["permission"], "View");
    let group_resolution = state
        .permission_resolver
        .resolve_permission_with_source(member_id, tenant_id, Resource::Folder(folder.id))
        .await
        .unwrap();
    assert_explanation_matches_resolution(current, group_resolution);
    assert!(current.get("share_id").is_none());
    let child = &member_body["files"][0]["effective_access"];
    assert_eq!(child["source"], "inherited");
    assert_eq!(child["permission"], "View");
    let inherited_resolution = state
        .permission_resolver
        .resolve_permission_with_source(member_id, tenant_id, Resource::File(file_id))
        .await
        .unwrap();
    assert_explanation_matches_resolution(child, inherited_resolution);
    assert!(child.get("share_id").is_none());
    let response_json = member_body.to_string();
    assert!(!response_json.contains(&group_id.to_string()));
    assert!(!response_json.contains(&unrelated_group_id.to_string()));
    assert!(!response_json.contains(&group_share_id.to_string()));
    assert!(!response_json.contains(&unrelated_share_id.to_string()));

    let (owner_status, owner_body) =
        folder_contents_http(&state, owner_id, tenant_id, folder.id).await;
    assert_eq!(owner_status, StatusCode::OK);
    assert_eq!(owner_body["current_folder_access"]["source"], "owner");
    assert_eq!(owner_body["current_folder_access"]["permission"], "Admin");
    let owner_resolution = state
        .permission_resolver
        .resolve_permission_with_source(owner_id, tenant_id, Resource::Folder(folder.id))
        .await
        .unwrap();
    assert_explanation_matches_resolution(&owner_body["current_folder_access"], owner_resolution);
    assert!(owner_body["current_folder_access"]
        .get("share_id")
        .is_none());

    // A resource Admin may inspect the winning share ID without belonging to
    // the recipient group. This uses the same Admin permission gate as share
    // management in UserShareService.
    let (admin_status, admin_body) =
        folder_contents_http(&state, grant_admin_id, tenant_id, folder.id).await;
    assert_eq!(admin_status, StatusCode::OK);
    assert_eq!(
        admin_body["current_folder_access"]["source"],
        "direct_share"
    );
    assert_eq!(admin_body["current_folder_access"]["permission"], "Admin");
    let admin_resolution = state
        .permission_resolver
        .resolve_permission_with_source(grant_admin_id, tenant_id, Resource::Folder(folder.id))
        .await
        .unwrap();
    assert_explanation_matches_resolution(&admin_body["current_folder_access"], admin_resolution);
    assert_eq!(
        admin_body["current_folder_access"]["share_id"],
        admin_share_id.to_string()
    );

    let (tree_status, tree_body) = folder_tree_http(&state, member_id, tenant_id, folder.id).await;
    assert_eq!(tree_status, StatusCode::OK);
    assert_explanation_matches_resolution(
        &tree_body["folder"]["effective_access"],
        group_resolution,
    );
    assert!(tree_body["folder"]["effective_access"]
        .get("share_id")
        .is_none());

    let (outsider_status, outsider_body) =
        folder_contents_http(&state, outsider_id, tenant_id, folder.id).await;
    let (missing_status, missing_body) =
        folder_contents_http(&state, outsider_id, tenant_id, Uuid::new_v4()).await;
    assert_eq!(outsider_status, StatusCode::NOT_FOUND);
    assert_eq!(missing_status, outsider_status);
    assert_eq!(missing_body, outsider_body);
    assert!(!outsider_body.to_string().contains(&folder.id.to_string()));
    assert!(!outsider_body
        .to_string()
        .contains(&group_share_id.to_string()));

    let foreign_tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(foreign_tenant_id)
        .bind(format!("Foreign explanation {fixture_id}"))
        .execute(&pool)
        .await
        .unwrap();
    let foreign_user_id =
        insert_user(&pool, fixture_id, foreign_tenant_id, "foreign-explain").await;
    let (foreign_status, foreign_body) =
        folder_contents_http(&state, foreign_user_id, foreign_tenant_id, folder.id).await;
    let (foreign_missing_status, foreign_missing_body) =
        folder_contents_http(&state, foreign_user_id, foreign_tenant_id, Uuid::new_v4()).await;
    assert_eq!(foreign_status, StatusCode::NOT_FOUND);
    assert_eq!(foreign_missing_status, foreign_status);
    assert_eq!(foreign_missing_body, foreign_body);
    assert!(!foreign_body.to_string().contains(&tenant_id.to_string()));
    assert!(!foreign_body.to_string().contains(&group_id.to_string()));

    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(folder.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM user_groups WHERE id = ANY($1)")
        .bind(vec![group_id, unrelated_group_id])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = ANY($1)")
        .bind(vec![
            owner_id,
            member_id,
            grant_admin_id,
            outsider_id,
            foreign_user_id,
        ])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tenants WHERE id = ANY($1)")
        .bind(vec![tenant_id, foreign_tenant_id])
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and RustFS"]
async fn permission_resolution_uses_highest_active_grant_and_revokes_each_path() {
    let (state, _) = common::setup_test_server().await;
    let pool = state.db_pool.clone();
    let fixture_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("Resolution {fixture_id}"))
        .execute(&pool)
        .await
        .unwrap();
    let owner_id = insert_user(&pool, fixture_id, tenant_id, "resolve-owner").await;
    let member_id = insert_user(&pool, fixture_id, tenant_id, "resolve-member").await;
    let folder = state
        .folder_service
        .create_folder(format!("resolver-{fixture_id}"), None, owner_id, tenant_id)
        .await
        .unwrap();
    let file_id = Uuid::new_v4();
    sqlx::query("INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id, parent_folder_id, tenant_id, current_version) VALUES ($1, 'resolver.txt', $2, 0, 'text/plain', 'hash', $3, $4, $5, $6, 1)")
        .bind(file_id)
        .bind(format!("/{fixture_id}/resolver.txt"))
        .bind(format!("{tenant_id}/files/{file_id}"))
        .bind(owner_id)
        .bind(folder.id)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();
    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("resolver-group-{fixture_id}"))
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

    let inherited_id = insert_folder_share(
        &pool,
        folder.id,
        owner_id,
        tenant_id,
        "View",
        Some(member_id),
        None,
        None,
    )
    .await;
    let direct_id = insert_file_share(
        &pool,
        file_id,
        owner_id,
        tenant_id,
        "Edit",
        Some(member_id),
        None,
        None,
    )
    .await;
    let group_share_id = insert_file_share(
        &pool,
        file_id,
        owner_id,
        tenant_id,
        "Admin",
        None,
        Some(group_id),
        None,
    )
    .await;
    let _expired_id = insert_file_share(
        &pool,
        file_id,
        owner_id,
        tenant_id,
        "Admin",
        Some(member_id),
        None,
        Some(chrono::Utc::now() - chrono::Duration::minutes(1)),
    )
    .await;

    let resolve = || {
        state.permission_resolver.resolve_permission_with_source(
            member_id,
            tenant_id,
            Resource::File(file_id),
        )
    };
    let highest = resolve().await.unwrap();
    assert_eq!(highest.permission, Some(SharePermissions::Admin));
    assert_eq!(highest.source, PermissionSource::GroupShare);
    assert_eq!(highest.share_id, Some(group_share_id));

    sqlx::query("UPDATE shares SET revoked_at = NOW() WHERE id = $1")
        .bind(group_share_id)
        .execute(&pool)
        .await
        .unwrap();
    let after_revoke = resolve().await.unwrap();
    assert_eq!(after_revoke.permission, Some(SharePermissions::Edit));
    assert_eq!(after_revoke.source, PermissionSource::DirectShare);
    assert_eq!(after_revoke.share_id, Some(direct_id));

    sqlx::query("UPDATE shares SET revoked_at = NULL, permissions = 'View' WHERE id = $1")
        .bind(group_share_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE shares SET revoked_at = NOW() WHERE id = $1")
        .bind(direct_id)
        .execute(&pool)
        .await
        .unwrap();
    let group_view = resolve().await.unwrap();
    assert_eq!(group_view.permission, Some(SharePermissions::View));
    assert_eq!(group_view.source, PermissionSource::GroupShare);

    sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
        .bind(group_id)
        .bind(member_id)
        .execute(&pool)
        .await
        .unwrap();
    let after_group_removal = resolve().await.unwrap();
    assert_eq!(after_group_removal.permission, Some(SharePermissions::View));
    assert_eq!(after_group_removal.source, PermissionSource::Inherited);
    assert_eq!(after_group_removal.share_id, Some(inherited_id));

    sqlx::query("UPDATE shares SET revoked_at = NOW() WHERE id = $1")
        .bind(inherited_id)
        .execute(&pool)
        .await
        .unwrap();
    let after_final_revocation = resolve().await.unwrap();
    assert_eq!(after_final_revocation.permission, None);
    assert_eq!(after_final_revocation.source, PermissionSource::None);
    assert_eq!(after_final_revocation.share_id, None);

    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(folder.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM user_groups WHERE id = $1")
        .bind(group_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = ANY($1)")
        .bind(vec![owner_id, member_id])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and RustFS"]
async fn nil_tenant_explanation_requires_share_and_resource_tenant_match() {
    let (state, _) = common::setup_test_server().await;
    let pool = state.db_pool.clone();
    let fixture_id = Uuid::new_v4();
    let tenant_id = Uuid::nil();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
        .bind(tenant_id)
        .bind(format!("nil-explanation-{fixture_id}"))
        .execute(&pool)
        .await
        .unwrap();
    let owner_id = insert_user(&pool, fixture_id, tenant_id, "nil-owner").await;
    let recipient_id = insert_user(&pool, fixture_id, tenant_id, "nil-recipient").await;
    let folder = state
        .folder_service
        .create_folder(
            format!("nil-resolver-{fixture_id}"),
            None,
            owner_id,
            tenant_id,
        )
        .await
        .unwrap();
    let share_id = insert_folder_share(
        &pool,
        folder.id,
        owner_id,
        tenant_id,
        "View",
        Some(recipient_id),
        None,
        None,
    )
    .await;

    let (same_tenant_status, same_tenant_body) =
        folder_contents_http(&state, recipient_id, tenant_id, folder.id).await;
    assert_eq!(same_tenant_status, StatusCode::OK);
    assert_eq!(
        same_tenant_body["current_folder_access"]["source"],
        "direct_share"
    );
    assert_eq!(
        same_tenant_body["current_folder_access"]["permission"],
        "View"
    );
    assert!(same_tenant_body["current_folder_access"]
        .get("share_id")
        .is_none());

    let other_tenant_id = Uuid::new_v4();
    sqlx::query("UPDATE shares SET tenant_id = $2 WHERE id = $1")
        .bind(share_id)
        .bind(other_tenant_id)
        .execute(&pool)
        .await
        .unwrap();
    let (mismatched_status, mismatched_body) =
        folder_contents_http(&state, recipient_id, tenant_id, folder.id).await;
    assert_eq!(mismatched_status, StatusCode::NOT_FOUND);
    assert!(!mismatched_body.to_string().contains(&folder.id.to_string()));
    assert!(!mismatched_body.to_string().contains(&share_id.to_string()));

    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(folder.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = ANY($1)")
        .bind(vec![owner_id, recipient_id])
        .execute(&pool)
        .await
        .unwrap();
}

async fn folder_contents_http(
    state: &rustshare_server::AppState,
    user_id: Uuid,
    tenant_id: Uuid,
    folder_id: Uuid,
) -> (StatusCode, serde_json::Value) {
    let token = state
        .jwt_manager
        .generate(user_id, "explanation@example.test", tenant_id)
        .unwrap();
    let response = rustshare_server::routes::share_routes()
        .with_state(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/shares/folders/{folder_id}/contents"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

async fn folder_tree_http(
    state: &rustshare_server::AppState,
    user_id: Uuid,
    tenant_id: Uuid,
    folder_id: Uuid,
) -> (StatusCode, serde_json::Value) {
    let token = state
        .jwt_manager
        .generate(user_id, "explanation@example.test", tenant_id)
        .unwrap();
    let response = rustshare_server::routes::share_routes()
        .with_state(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/shares/folders/{folder_id}/tree"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

fn assert_explanation_matches_resolution(
    explanation: &serde_json::Value,
    result: PermissionResult,
) {
    let source = match result.source {
        PermissionSource::Owner => "owner",
        PermissionSource::DirectShare => "direct_share",
        PermissionSource::GroupShare => "group_share",
        PermissionSource::Inherited => "inherited",
        PermissionSource::None => panic!("accessible resource had no permission source"),
    };
    assert_eq!(explanation["source"], source);
    assert_eq!(
        explanation["permission"],
        serde_json::to_value(
            result
                .permission
                .expect("permission source must grant access")
        )
        .unwrap()
    );
}

#[allow(clippy::too_many_arguments)]
async fn insert_folder_share(
    pool: &sqlx::PgPool,
    folder_id: Uuid,
    created_by: Uuid,
    tenant_id: Uuid,
    permission: &str,
    recipient_user_id: Option<Uuid>,
    recipient_group_id: Option<Uuid>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Uuid {
    let share_id = Uuid::new_v4();
    sqlx::query("INSERT INTO shares (id, folder_id, share_token, created_by, permissions, recipient_user_id, recipient_group_id, tenant_id, expires_at) VALUES ($1, $2, NULL, $3, $4, $5, $6, $7, $8)")
        .bind(share_id)
        .bind(folder_id)
        .bind(created_by)
        .bind(permission)
        .bind(recipient_user_id)
        .bind(recipient_group_id)
        .bind(tenant_id)
        .bind(expires_at)
        .execute(pool)
        .await
        .unwrap();
    share_id
}

#[allow(clippy::too_many_arguments)]
async fn insert_file_share(
    pool: &sqlx::PgPool,
    file_id: Uuid,
    created_by: Uuid,
    tenant_id: Uuid,
    permission: &str,
    recipient_user_id: Option<Uuid>,
    recipient_group_id: Option<Uuid>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Uuid {
    let share_id = Uuid::new_v4();
    sqlx::query("INSERT INTO shares (id, file_id, share_token, created_by, permissions, recipient_user_id, recipient_group_id, tenant_id, expires_at) VALUES ($1, $2, NULL, $3, $4, $5, $6, $7, $8)")
        .bind(share_id)
        .bind(file_id)
        .bind(created_by)
        .bind(permission)
        .bind(recipient_user_id)
        .bind(recipient_group_id)
        .bind(tenant_id)
        .bind(expires_at)
        .execute(pool)
        .await
        .unwrap();
    share_id
}

async fn insert_user(pool: &sqlx::PgPool, id: Uuid, tenant_id: Uuid, role: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota) VALUES ($1,$2,$3,'hash',$4,$5,10737418240)")
        .bind(user_id).bind(format!("{role}{id}")).bind(format!("{role}{id}@example.test"))
        .bind(tenant_id).bind(role).execute(pool).await.unwrap();
    user_id
}
