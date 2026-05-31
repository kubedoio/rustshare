//! Integration tests for group sharing functionality

use rustshare_core::domain::SharePermissions;
use rustshare_core::events::{AggregateType, EventBroadcaster, EventType};
use rustshare_core::services::{
    JwtOps, PermissionResolver, Resource, ShareError, ShareNotificationRepo, ShareService,
};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_storage::metadata_v2::compat::MetadataStoreCompat;
use rustshare_storage::{EventStore, MetadataStore};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

async fn test_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    sqlx::PgPool::connect(&url)
        .await
        .expect("DB connect failed")
}

// Mock JWT manager for testing
struct MockJwtManager;

impl JwtOps for MockJwtManager {
    fn encode_custom_claims<T: serde::Serialize>(&self, _claims: &T) -> Result<String, String> {
        Ok("test_jwt_token".to_string())
    }
}

// Mock notification repo for testing
struct MockNotificationRepo;

#[async_trait::async_trait]
impl ShareNotificationRepo for MockNotificationRepo {
    async fn was_notified(
        &self,
        _user_id: rustshare_core::domain::UserId,
        _share_id: uuid::Uuid,
    ) -> Result<bool, sqlx::Error> {
        Ok(false)
    }

    async fn record_notification(
        &self,
        _user_id: rustshare_core::domain::UserId,
        _share_id: uuid::Uuid,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }
}

fn create_test_share_service(
    pool: PgPool,
) -> ShareService<EventStore, MetadataStore, MockJwtManager, MockNotificationRepo> {
    let event_store = Arc::new(EventStore::new(pool.clone()));
    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));
    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let jwt_manager = Arc::new(MockJwtManager);
    let notification_repo = Arc::new(MockNotificationRepo);

    ShareService::new(
        event_store,
        metadata_store,
        broadcaster,
        jwt_manager,
        notification_repo,
    )
}

struct GroupShareFixture {
    tenant_id: Uuid,
    owner_id: Uuid,
    member_id: Uuid,
    group_id: Uuid,
    file_id: Uuid,
    folder_id: Uuid,
}

async fn setup_group_share_fixture(pool: &PgPool) -> GroupShareFixture {
    let test_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("Test Tenant {}", test_id))
        .execute(pool)
        .await
        .unwrap();

    let owner_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota) VALUES ($1, $2, $3, $4, $5, $6, $7)")
        .bind(owner_id)
        .bind(format!("owner{}", test_id))
        .bind(format!("owner{}@example.com", test_id))
        .bind("hash")
        .bind(tenant_id)
        .bind(format!("Owner {}", test_id))
        .bind(10_737_418_240i64)
        .execute(pool)
        .await
        .unwrap();

    let member_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota) VALUES ($1, $2, $3, $4, $5, $6, $7)")
        .bind(member_id)
        .bind(format!("member{}", test_id))
        .bind(format!("member{}@example.com", test_id))
        .bind("hash")
        .bind(tenant_id)
        .bind(format!("Member {}", test_id))
        .bind(10_737_418_240i64)
        .execute(pool)
        .await
        .unwrap();

    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("Test Group {}", test_id))
    .bind(tenant_id)
    .bind(owner_id)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
        .bind(group_id)
        .bind(member_id)
        .execute(pool)
        .await
        .unwrap();

    let file_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO files (id, name, path, content_hash, size, mime_type, owner_id, tenant_id, current_version, storage_key)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        "#,
    )
    .bind(file_id)
    .bind("test.txt")
    .bind(format!("/test_{}.txt", test_id))
    .bind("hash")
    .bind(0i64)
    .bind("text/plain")
    .bind(owner_id)
    .bind(tenant_id)
    .bind(1i32)
    .bind(format!("{}/files/{}", tenant_id, file_id))
    .execute(pool)
    .await
    .unwrap();

    let folder_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO folders (id, name, path, owner_id, tenant_id)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(folder_id)
    .bind("test_folder")
    .bind(format!("/test_folder_{}", test_id))
    .bind(owner_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .unwrap();

    GroupShareFixture {
        tenant_id,
        owner_id,
        member_id,
        group_id,
        file_id,
        folder_id,
    }
}

async fn cleanup_group_share_fixture(
    pool: &PgPool,
    fixture: &GroupShareFixture,
    share_id: Option<Uuid>,
) {
    if let Some(sid) = share_id {
        sqlx::query("DELETE FROM shares WHERE id = $1")
            .bind(sid)
            .execute(pool)
            .await
            .ok();
    }
    sqlx::query("DELETE FROM files WHERE id = $1")
        .bind(fixture.file_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(fixture.folder_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM group_members WHERE group_id = $1")
        .bind(fixture.group_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM user_groups WHERE id = $1")
        .bind(fixture.group_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
        .bind(fixture.owner_id)
        .bind(fixture.member_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(fixture.tenant_id)
        .execute(pool)
        .await
        .ok();
}

/// Test creating a group share and verifying member access
#[tokio::test]
#[ignore] // Requires database
async fn test_create_group_share_success() {
    let pool = test_pool().await;
    let f = setup_group_share_fixture(&pool).await;
    let share_service = create_test_share_service(pool.clone());

    let share = share_service
        .create_group_share(
            Resource::File(f.file_id),
            f.group_id,
            SharePermissions::View,
            f.owner_id,
            f.tenant_id,
        )
        .await
        .expect("Failed to create group share");

    assert_eq!(share.file_id, Some(f.file_id));
    assert_eq!(share.recipient_group_id, Some(f.group_id));
    assert_eq!(share.permissions, SharePermissions::View);
    assert!(
        share.share_token.is_none(),
        "Group share should not have a token"
    );

    // Verify member has access via permission resolver
    let resolver =
        PermissionResolver::new(Arc::new(PermissionResolverRepository::new(pool.clone())));
    let has_access = resolver
        .check_file_permission(f.member_id, f.file_id, SharePermissions::View)
        .await
        .expect("Permission check failed");
    assert!(
        has_access,
        "Group member should have View access to shared file"
    );

    cleanup_group_share_fixture(&pool, &f, Some(share.id)).await;
}

/// Test that non-members cannot share with group
#[tokio::test]
#[ignore] // Requires database
async fn test_non_member_cannot_share_with_group() {
    let pool = test_pool().await;
    let mut f = setup_group_share_fixture(&pool).await;
    let share_service = create_test_share_service(pool.clone());

    // Remove member from group so they are no longer a member
    sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
        .bind(f.group_id)
        .bind(f.member_id)
        .execute(&pool)
        .await
        .unwrap();

    // Non-owner, non-member tries to create group share
    let result = share_service
        .create_group_share(
            Resource::File(f.file_id),
            f.group_id,
            SharePermissions::View,
            f.member_id,
            f.tenant_id,
        )
        .await;

    assert!(
        matches!(result, Err(ShareError::NotGroupMember(_))),
        "Non-member should not be able to share with group"
    );

    cleanup_group_share_fixture(&pool, &f, None).await;
}

/// Test that resource owners can share with any group in their tenant
#[tokio::test]
#[ignore] // Requires database
async fn test_admin_can_share_with_any_group() {
    let pool = test_pool().await;
    let f = setup_group_share_fixture(&pool).await;
    let share_service = create_test_share_service(pool.clone());

    // Owner is not a member of the group by default, but should still be able
    // to share because they own the resource.
    let share = share_service
        .create_group_share(
            Resource::File(f.file_id),
            f.group_id,
            SharePermissions::Edit,
            f.owner_id,
            f.tenant_id,
        )
        .await
        .expect("Owner should be able to share with any group in their tenant");

    assert_eq!(share.permissions, SharePermissions::Edit);

    cleanup_group_share_fixture(&pool, &f, Some(share.id)).await;
}

/// Test cross-tenant sharing is blocked
#[tokio::test]
#[ignore] // Requires database
async fn test_cross_tenant_sharing_blocked() {
    let pool = test_pool().await;
    let f = setup_group_share_fixture(&pool).await;
    let share_service = create_test_share_service(pool.clone());

    let other_tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(other_tenant_id)
        .bind("Other Tenant")
        .execute(&pool)
        .await
        .unwrap();

    let other_user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota) VALUES ($1, $2, $3, $4, $5, $6, $7)")
        .bind(other_user_id)
        .bind("other_user")
        .bind("other@example.com")
        .bind("hash")
        .bind(other_tenant_id)
        .bind("Other User")
        .bind(10_737_418_240i64)
        .execute(&pool)
        .await
        .unwrap();

    // Attempt cross-tenant share
    let result = share_service
        .create_group_share(
            Resource::File(f.file_id),
            f.group_id,
            SharePermissions::View,
            other_user_id,
            other_tenant_id,
        )
        .await;

    assert!(
        matches!(result, Err(ShareError::CrossTenantSharingNotAllowed)),
        "Cross-tenant sharing should be blocked"
    );

    // Cleanup cross-tenant data
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(other_user_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(other_tenant_id)
        .execute(&pool)
        .await
        .ok();
    cleanup_group_share_fixture(&pool, &f, None).await;
}

/// Test duplicate group share prevention
#[tokio::test]
#[ignore] // Requires database
async fn test_duplicate_group_share_prevented() {
    let pool = test_pool().await;
    let f = setup_group_share_fixture(&pool).await;
    let share_service = create_test_share_service(pool.clone());

    let share = share_service
        .create_group_share(
            Resource::File(f.file_id),
            f.group_id,
            SharePermissions::View,
            f.owner_id,
            f.tenant_id,
        )
        .await
        .expect("First group share should succeed");

    let result = share_service
        .create_group_share(
            Resource::File(f.file_id),
            f.group_id,
            SharePermissions::Edit,
            f.owner_id,
            f.tenant_id,
        )
        .await;

    assert!(
        matches!(result, Err(ShareError::GroupShareAlreadyExists)),
        "Duplicate group share should be prevented"
    );

    cleanup_group_share_fixture(&pool, &f, Some(share.id)).await;
}

/// Test that group share access fails after share revoke
#[tokio::test]
#[ignore] // Requires database
async fn test_revoke_group_share() {
    let pool = test_pool().await;
    let f = setup_group_share_fixture(&pool).await;
    let share_service = create_test_share_service(pool.clone());

    let share = share_service
        .create_group_share(
            Resource::File(f.file_id),
            f.group_id,
            SharePermissions::View,
            f.owner_id,
            f.tenant_id,
        )
        .await
        .expect("Failed to create group share");

    let resolver =
        PermissionResolver::new(Arc::new(PermissionResolverRepository::new(pool.clone())));

    // Member should have access before revocation
    let has_access_before = resolver
        .check_file_permission(f.member_id, f.file_id, SharePermissions::View)
        .await
        .expect("Permission check failed");
    assert!(
        has_access_before,
        "Member should have access before revocation"
    );

    // Revoke the group share
    share_service
        .revoke_group_share(share.id, f.owner_id)
        .await
        .expect("Failed to revoke group share");

    // Member should lose access after revocation
    let has_access_after = resolver
        .check_file_permission(f.member_id, f.file_id, SharePermissions::View)
        .await
        .expect("Permission check failed");
    assert!(
        !has_access_after,
        "Member should lose access after group share revocation"
    );

    cleanup_group_share_fixture(&pool, &f, Some(share.id)).await;
}

/// Test that group share access fails after membership removal
#[tokio::test]
#[ignore] // Requires database
async fn test_group_share_access_fails_after_membership_removal() {
    let pool = test_pool().await;
    let f = setup_group_share_fixture(&pool).await;
    let share_service = create_test_share_service(pool.clone());

    let share = share_service
        .create_group_share(
            Resource::File(f.file_id),
            f.group_id,
            SharePermissions::View,
            f.owner_id,
            f.tenant_id,
        )
        .await
        .expect("Failed to create group share");

    let resolver =
        PermissionResolver::new(Arc::new(PermissionResolverRepository::new(pool.clone())));

    // Member should have access before removal
    let has_access_before = resolver
        .check_file_permission(f.member_id, f.file_id, SharePermissions::View)
        .await
        .expect("Permission check failed");
    assert!(
        has_access_before,
        "Member should have access before removal"
    );

    // Remove member from group
    sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
        .bind(f.group_id)
        .bind(f.member_id)
        .execute(&pool)
        .await
        .unwrap();

    // Member should lose access after membership removal
    let has_access_after = resolver
        .check_file_permission(f.member_id, f.file_id, SharePermissions::View)
        .await
        .expect("Permission check failed");
    assert!(
        !has_access_after,
        "Member should lose access after being removed from group"
    );

    cleanup_group_share_fixture(&pool, &f, Some(share.id)).await;
}

/// Test updating group share permission
#[tokio::test]
#[ignore] // Requires database
async fn test_update_group_share_permission() {
    let pool = test_pool().await;
    let f = setup_group_share_fixture(&pool).await;
    let share_service = create_test_share_service(pool.clone());

    let share = share_service
        .create_group_share(
            Resource::File(f.file_id),
            f.group_id,
            SharePermissions::View,
            f.owner_id,
            f.tenant_id,
        )
        .await
        .expect("Failed to create group share");

    let resolver =
        PermissionResolver::new(Arc::new(PermissionResolverRepository::new(pool.clone())));

    // Initially View only
    let can_edit_before = resolver
        .check_file_permission(f.member_id, f.file_id, SharePermissions::Edit)
        .await
        .expect("Permission check failed");
    assert!(
        !can_edit_before,
        "Member should not have Edit before update"
    );

    // Update to Edit
    let updated = share_service
        .update_group_share_permission(share.id, SharePermissions::Edit, f.owner_id)
        .await
        .expect("Failed to update group share permission");

    assert_eq!(updated.permissions, SharePermissions::Edit);

    // Now member should have Edit
    let can_edit_after = resolver
        .check_file_permission(f.member_id, f.file_id, SharePermissions::Edit)
        .await
        .expect("Permission check failed");
    assert!(can_edit_after, "Member should have Edit after update");

    cleanup_group_share_fixture(&pool, &f, Some(share.id)).await;
}

/// Test that revoking a group share emits an auditable ShareRevoked event
#[tokio::test]
#[ignore] // Requires database
async fn test_group_share_revoke_emits_audit_event() {
    let pool = test_pool().await;
    let f = setup_group_share_fixture(&pool).await;
    let share_service = create_test_share_service(pool.clone());
    let event_store = Arc::new(EventStore::new(pool.clone()));

    let share = share_service
        .create_group_share(
            Resource::File(f.file_id),
            f.group_id,
            SharePermissions::View,
            f.owner_id,
            f.tenant_id,
        )
        .await
        .expect("Failed to create group share");

    // Revoke the share
    share_service
        .revoke_group_share(share.id, f.owner_id)
        .await
        .expect("Failed to revoke group share");

    // Verify a ShareRevoked event was emitted
    let events = event_store
        .get_events(share.id, AggregateType::Share)
        .await
        .expect("Failed to fetch events");

    let revoked_events: Vec<_> = events
        .into_iter()
        .filter(|e| e.event_type == EventType::ShareRevoked)
        .collect();

    assert_eq!(
        revoked_events.len(),
        1,
        "Exactly one ShareRevoked event should be emitted"
    );
    assert_eq!(
        revoked_events[0].aggregate_id, share.id,
        "ShareRevoked event should reference the correct share"
    );

    cleanup_group_share_fixture(&pool, &f, Some(share.id)).await;
}

/// Test that folder group shares are revoked correctly
#[tokio::test]
#[ignore] // Requires database
async fn test_group_folder_share_revoke_denies_access() {
    let pool = test_pool().await;
    let f = setup_group_share_fixture(&pool).await;
    let share_service = create_test_share_service(pool.clone());

    let share = share_service
        .create_group_share(
            Resource::Folder(f.folder_id),
            f.group_id,
            SharePermissions::View,
            f.owner_id,
            f.tenant_id,
        )
        .await
        .expect("Failed to create folder group share");

    let resolver =
        PermissionResolver::new(Arc::new(PermissionResolverRepository::new(pool.clone())));

    // Member should have folder access before revocation
    let has_access_before = resolver
        .check_folder_permission(f.member_id, f.folder_id, SharePermissions::View)
        .await
        .expect("Permission check failed");
    assert!(
        has_access_before,
        "Member should have folder access before revocation"
    );

    // Revoke
    share_service
        .revoke_group_share(share.id, f.owner_id)
        .await
        .expect("Failed to revoke folder group share");

    // Member should lose folder access
    let has_access_after = resolver
        .check_folder_permission(f.member_id, f.folder_id, SharePermissions::View)
        .await
        .expect("Permission check failed");
    assert!(
        !has_access_after,
        "Member should lose folder access after revocation"
    );

    cleanup_group_share_fixture(&pool, &f, Some(share.id)).await;
}

/// Test SQL-based group membership check (the underlying query used by compat layer)
#[tokio::test]
async fn test_compat_layer_group_membership_sql() {
    let pool = test_pool().await;

    // Create a unique identifier for this test run
    let test_id = Uuid::new_v4();

    // Create test tenant
    let tenant_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenants (id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(format!("Test Tenant {}", test_id))
    .fetch_one(&pool)
    .await
    .expect("Failed to create tenant");

    // Create test user
    let user_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota) VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id"
    )
    .bind(Uuid::new_v4())
    .bind(format!("testuser{}", test_id))
    .bind(format!("test{}@example.com", test_id))
    .bind("hash")
    .bind(tenant_id)
    .bind(format!("Test User {}", test_id))
    .bind(10_737_418_240i64)
    .fetch_one(&pool)
    .await
    .expect("Failed to create user");

    // Create test group
    let group_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind(Uuid::new_v4())
    .bind(format!("Test Group {}", test_id))
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to create group");

    // Test the SQL query used by compat layer - before adding to group should return false
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM group_members
            WHERE group_id = $1 AND user_id = $2
        )
        "#,
    )
    .bind(group_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to check membership");

    assert!(!is_member, "User should not be a member before being added");

    // Add user to group
    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
        .bind(group_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("Failed to add user to group");

    // Test the SQL query again - after adding to group should return true
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM group_members
            WHERE group_id = $1 AND user_id = $2
        )
        "#,
    )
    .bind(group_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to check membership");

    assert!(is_member, "User should be a member after being added");

    // Cleanup
    sqlx::query("DELETE FROM group_members WHERE group_id = $1")
        .bind(group_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM user_groups WHERE id = $1")
        .bind(group_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .ok();
}

/// Test that compat layer can find users by ID
#[tokio::test]
async fn test_compat_layer_find_user_by_id() {
    use rustshare_core::services::ShareMetadataStoreOps;

    let pool = test_pool().await;

    // Create a unique identifier for this test run
    let test_id = Uuid::new_v4();

    // Create test tenant
    let tenant_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenants (id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(format!("Test Tenant {}", test_id))
    .fetch_one(&pool)
    .await
    .expect("Failed to create tenant");

    // Create test user
    let user_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (id, username, email, password_hash, display_name, tenant_id, storage_quota) VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id"
    )
    .bind(Uuid::new_v4())
    .bind(format!("user{}", test_id))
    .bind(format!("test{}@example.com", test_id))
    .bind("hash")
    .bind(format!("Test User {}", test_id))
    .bind(tenant_id)
    .bind(10_737_418_240i64)
    .fetch_one(&pool)
    .await
    .expect("Failed to create user");

    // Create the compat layer with a mock repository
    // The find_user_by_id method only uses the pool for SQL queries
    let compat = create_test_compat(pool.clone()).await;

    // Test finding the user via the compat layer
    let found_user = compat
        .find_user_by_id(user_id)
        .await
        .expect("Compat layer find_user_by_id should not fail");

    assert!(
        found_user.is_some(),
        "Should find the user by ID via compat layer"
    );
    let user = found_user.unwrap();
    assert_eq!(user.id, user_id);
    assert_eq!(user.email, format!("test{}@example.com", test_id));

    // Test non-existent user via compat layer
    let non_existent_id = Uuid::new_v4();
    let not_found = compat
        .find_user_by_id(non_existent_id)
        .await
        .expect("Compat layer find_user_by_id should not fail for non-existent user");

    assert!(
        not_found.is_none(),
        "Should not find a non-existent user via compat layer"
    );

    // Cleanup
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .ok();
}

/// Helper function to create a MetadataStoreCompat for testing
/// Uses a minimal mock repository since find_user_by_id only needs the pool
async fn create_test_compat(pool: sqlx::PgPool) -> MetadataStoreCompat {
    use async_trait::async_trait;
    use rustshare_core::domain::{FileId, FolderId, ShareId, UserId};
    use rustshare_storage::metadata_v2::schemas::*;
    use rustshare_storage::repos::{
        EventRepository, FileRepository, FileVersionRepository, FolderChildrenIndexRepository,
        FolderRepository, RepositoryError, SearchIndexRepository, ShareRepository,
        TombstoneRepository,
    };
    use std::sync::Arc;

    // Minimal mock implementations that do nothing (find_user_by_id doesn't use them)
    struct MockFolderRepo;
    struct MockFileRepo;
    struct MockFileVersionRepo;
    struct MockShareRepo;
    struct MockEventRepo;
    struct MockFolderChildrenIndexRepo;
    struct MockTombstoneRepo;
    struct MockSearchIndexRepo;

    #[async_trait]
    impl FolderRepository for MockFolderRepo {
        async fn get(&self, _id: FolderId) -> Result<Option<FolderDocument>, RepositoryError> {
            Ok(None)
        }
        async fn create(&self, _folder: &FolderDocument) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn update(&self, _folder: &FolderDocument) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn delete(&self, _id: FolderId, _deleted_by: UserId) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn hard_delete(&self, _id: FolderId) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn list_descendants(
            &self,
            _folder_id: FolderId,
        ) -> Result<Vec<FolderDocument>, RepositoryError> {
            Ok(vec![])
        }
        async fn get_user_roots(
            &self,
            _user_id: UserId,
        ) -> Result<Vec<FolderDocument>, RepositoryError> {
            Ok(vec![])
        }
        async fn name_exists(
            &self,
            _parent_id: Option<FolderId>,
            _name: &str,
            _owner_id: UserId,
        ) -> Result<bool, RepositoryError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl FileRepository for MockFileRepo {
        async fn get(&self, _id: FileId) -> Result<Option<FileDocument>, RepositoryError> {
            Ok(None)
        }
        async fn create(&self, _file: &FileDocument) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn update(&self, _file: &FileDocument) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn delete(&self, _id: FileId, _deleted_by: UserId) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn hard_delete(&self, _id: FileId) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn name_exists(
            &self,
            _parent_id: Option<FolderId>,
            _name: &str,
            _owner_id: UserId,
        ) -> Result<bool, RepositoryError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl FileVersionRepository for MockFileVersionRepo {
        async fn get(
            &self,
            _version_id: uuid::Uuid,
        ) -> Result<Option<FileVersionDocument>, RepositoryError> {
            Ok(None)
        }
        async fn get_by_number(
            &self,
            _file_id: FileId,
            _version_number: i32,
        ) -> Result<Option<FileVersionDocument>, RepositoryError> {
            Ok(None)
        }
        async fn create(&self, _version: &FileVersionDocument) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn list_by_file(
            &self,
            _file_id: FileId,
        ) -> Result<Vec<FileVersionDocument>, RepositoryError> {
            Ok(vec![])
        }
        async fn get_latest(
            &self,
            _file_id: FileId,
        ) -> Result<Option<FileVersionDocument>, RepositoryError> {
            Ok(None)
        }
    }

    #[async_trait]
    impl ShareRepository for MockShareRepo {
        async fn get(&self, _id: ShareId) -> Result<Option<ShareDocument>, RepositoryError> {
            Ok(None)
        }
        async fn get_by_token(
            &self,
            _token_hash: &str,
        ) -> Result<Option<ShareDocument>, RepositoryError> {
            Ok(None)
        }
        async fn create(&self, _share: &ShareDocument) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn update(&self, _share: &ShareDocument) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn revoke(&self, _id: ShareId, _revoked_by: UserId) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn delete(&self, _id: ShareId) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn list_by_resource(
            &self,
            _resource_type: &str,
            _resource_id: uuid::Uuid,
        ) -> Result<Vec<ShareDocument>, RepositoryError> {
            Ok(vec![])
        }
        async fn list_by_creator(
            &self,
            _user_id: UserId,
        ) -> Result<Vec<ShareDocument>, RepositoryError> {
            Ok(vec![])
        }
        async fn list_by_recipient(
            &self,
            _user_id: UserId,
        ) -> Result<Vec<ShareDocument>, RepositoryError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl EventRepository for MockEventRepo {
        async fn append(&self, _event: &EventDocument) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn read_for_resource(
            &self,
            _resource_type: &str,
            _resource_id: uuid::Uuid,
            _limit: usize,
        ) -> Result<Vec<EventDocument>, RepositoryError> {
            Ok(vec![])
        }
        async fn read_range(
            &self,
            _start: chrono::DateTime<chrono::Utc>,
            _end: chrono::DateTime<chrono::Utc>,
            _limit: usize,
        ) -> Result<Vec<EventDocument>, RepositoryError> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl FolderChildrenIndexRepository for MockFolderChildrenIndexRepo {
        async fn get(
            &self,
            _folder_id: FolderId,
        ) -> Result<Option<FolderChildrenIndex>, RepositoryError> {
            Ok(None)
        }
        async fn save(&self, _index: &FolderChildrenIndex) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn rebuild(
            &self,
            _folder_id: FolderId,
        ) -> Result<FolderChildrenIndex, RepositoryError> {
            Ok(FolderChildrenIndex::new(_folder_id))
        }
    }

    #[async_trait]
    impl TombstoneRepository for MockTombstoneRepo {
        async fn get(
            &self,
            _resource_type: &str,
            _resource_id: uuid::Uuid,
        ) -> Result<Option<TombstoneDocument>, RepositoryError> {
            Ok(None)
        }
        async fn create(&self, _tombstone: &TombstoneDocument) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn list_by_user(
            &self,
            _user_id: UserId,
        ) -> Result<Vec<TombstoneDocument>, RepositoryError> {
            Ok(vec![])
        }
        async fn delete(
            &self,
            _resource_type: &str,
            _resource_id: uuid::Uuid,
        ) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    #[async_trait]
    impl SearchIndexRepository for MockSearchIndexRepo {
        async fn index_file(&self, _file: &FileDocument) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn index_folder(&self, _folder: &FolderDocument) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn remove_from_index(&self, _resource_id: Uuid) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn search(
            &self,
            _tenant_id: Uuid,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<rustshare_storage::metadata_v2::schemas::SearchResult>, RepositoryError>
        {
            Ok(vec![])
        }
    }

    struct MockMetadataRepository {
        folders: MockFolderRepo,
        files: MockFileRepo,
        file_versions: MockFileVersionRepo,
        shares: MockShareRepo,
        events: MockEventRepo,
        folder_children_index: MockFolderChildrenIndexRepo,
        tombstones: MockTombstoneRepo,
        search_index: MockSearchIndexRepo,
    }

    impl MockMetadataRepository {
        fn new() -> Self {
            Self {
                folders: MockFolderRepo,
                files: MockFileRepo,
                file_versions: MockFileVersionRepo,
                shares: MockShareRepo,
                events: MockEventRepo,
                folder_children_index: MockFolderChildrenIndexRepo,
                tombstones: MockTombstoneRepo,
                search_index: MockSearchIndexRepo,
            }
        }
    }

    impl rustshare_storage::repos::MetadataRepository for MockMetadataRepository {
        fn folders(&self) -> &dyn FolderRepository {
            &self.folders
        }
        fn files(&self) -> &dyn FileRepository {
            &self.files
        }
        fn file_versions(&self) -> &dyn FileVersionRepository {
            &self.file_versions
        }
        fn shares(&self) -> &dyn ShareRepository {
            &self.shares
        }
        fn events(&self) -> &dyn EventRepository {
            &self.events
        }
        fn folder_children_index(&self) -> &dyn FolderChildrenIndexRepository {
            &self.folder_children_index
        }
        fn tombstones(&self) -> &dyn TombstoneRepository {
            &self.tombstones
        }
        fn search_index(&self) -> &dyn SearchIndexRepository {
            &self.search_index
        }
    }

    MetadataStoreCompat::new(Arc::new(MockMetadataRepository::new()), pool)
}
