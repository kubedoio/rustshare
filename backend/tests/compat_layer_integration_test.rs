//! Integration tests for MetadataStoreCompat layer.
//!
//! These tests verify that the compat layer properly delegates to SQL
//! for operations not yet in the MetadataRepository trait.

use rustshare_core::services::ShareMetadataStoreOps;
use rustshare_storage::metadata_v2::compat::MetadataStoreCompat;
use sqlx::PgPool;
use uuid::Uuid;

async fn test_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    sqlx::PgPool::connect(&url)
        .await
        .expect("DB connect failed")
}

/// Test that group sharing works end-to-end through the compat layer
#[tokio::test]
async fn test_group_sharing_works_via_compat_layer() {
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

    // Create test users
    let owner_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (id, username, email, password_hash, tenant_id) VALUES ($1, $2, $3, $4, $5) RETURNING id"
    )
    .bind(Uuid::new_v4())
    .bind(format!("owner{}", test_id))
    .bind(format!("owner{}@example.com", test_id))
    .bind("hash")
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to create owner user");

    let member_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (id, username, email, password_hash, tenant_id) VALUES ($1, $2, $3, $4, $5) RETURNING id"
    )
    .bind(Uuid::new_v4())
    .bind(format!("member{}", test_id))
    .bind(format!("member{}@example.com", test_id))
    .bind("hash")
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to create member user");

    // Create test group
    let group_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind(Uuid::new_v4())
    .bind(format!("Test Group {}", test_id))
    .bind(tenant_id)
    .bind(owner_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to create group");

    // Add member to group
    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
        .bind(group_id)
        .bind(member_id)
        .execute(&pool)
        .await
        .expect("Failed to add user to group");

    // Create test file
    let file_id = Uuid::new_v4();
    sqlx::query(r#"
        INSERT INTO files (id, name, path, content_hash, size, mime_type, owner_id, tenant_id, current_version)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
    "#)
    .bind(file_id)
    .bind("test.txt")
    .bind(format!("/test_{}.txt", test_id))
    .bind("hash")
    .bind(0i64)
    .bind("text/plain")
    .bind(owner_id)
    .bind(tenant_id)
    .bind(1i32)
    .execute(&pool)
    .await
    .expect("Failed to create file");

    // Create MetadataStoreCompat instance
    let compat = create_test_compat(pool.clone()).await;

    // Test 1: Verify find_user_by_id works for owner
    let owner_user = compat
        .find_user_by_id(owner_id)
        .await
        .expect("Compat layer find_user_by_id should not fail");
    assert!(owner_user.is_some(), "Should find owner user");
    let owner_user = owner_user.unwrap();
    assert_eq!(owner_user.id, owner_id);
    assert_eq!(owner_user.email, format!("owner{}@example.com", test_id));

    // Test 2: Verify find_user_by_id works for member
    let member_user = compat
        .find_user_by_id(member_id)
        .await
        .expect("Compat layer find_user_by_id should not fail");
    assert!(member_user.is_some(), "Should find member user");
    let member_user = member_user.unwrap();
    assert_eq!(member_user.id, member_id);
    assert_eq!(member_user.email, format!("member{}@example.com", test_id));

    // Test 3: Verify is_user_in_group returns true for member
    let is_member = compat
        .is_user_in_group(member_id, group_id)
        .await
        .expect("Compat layer is_user_in_group should not fail");
    assert!(is_member, "Member should be in group");

    // Test 4: Verify is_user_in_group returns false for owner (not added to group)
    let is_owner_member = compat
        .is_user_in_group(owner_id, group_id)
        .await
        .expect("Compat layer is_user_in_group should not fail");
    assert!(!is_owner_member, "Owner should not be in group");

    // Test 5: Create a group share via compat layer
    let share_id = Uuid::new_v4();
    let share = rustshare_core::domain::Share {
        id: share_id,
        file_id: Some(file_id),
        folder_id: None,
        share_token: None,
        permissions: rustshare_core::domain::SharePermissions::View,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        created_by: owner_id,
        created_at: chrono::Utc::now(),
        revoked_at: None,
        access_count: 0,
        tenant_id,
        recipient_user_id: None,
        recipient_group_id: Some(group_id),
    };

    compat
        .create_share(&share)
        .await
        .expect("Should create share via compat layer");

    // Test 6: Verify share was created by retrieving it
    let retrieved_share = compat
        .get_share_by_id(share_id, owner_id)
        .await
        .expect("Should get share by ID")
        .expect("Share should exist");
    assert_eq!(retrieved_share.id, share_id);
    assert_eq!(retrieved_share.file_id, Some(file_id));
    assert_eq!(retrieved_share.recipient_group_id, Some(group_id));
    assert_eq!(
        retrieved_share.permissions,
        rustshare_core::domain::SharePermissions::View
    );

    // Test 7: Verify get_file_shares returns the group share
    let file_shares = compat
        .get_file_shares(file_id)
        .await
        .expect("Should get file shares");
    assert_eq!(file_shares.len(), 1);
    assert_eq!(file_shares[0].id, share_id);
    assert_eq!(file_shares[0].recipient_group_id, Some(group_id));

    // Test 8: Verify find_user_by_id returns None for non-existent user
    let non_existent_id = Uuid::new_v4();
    let not_found = compat
        .find_user_by_id(non_existent_id)
        .await
        .expect("Compat layer find_user_by_id should not fail for non-existent user");
    assert!(not_found.is_none(), "Should not find a non-existent user");

    // Test 9: Verify is_user_in_group returns false for non-existent user
    let is_non_member = compat
        .is_user_in_group(non_existent_id, group_id)
        .await
        .expect("Compat layer is_user_in_group should not fail for non-existent user");
    assert!(!is_non_member, "Non-existent user should not be in group");

    // Cleanup
    sqlx::query("DELETE FROM shares WHERE id = $1")
        .bind(share_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM files WHERE id = $1")
        .bind(file_id)
        .execute(&pool)
        .await
        .ok();
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
    sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
        .bind(owner_id)
        .bind(member_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .ok();
}

/// Test that find_user_by_id respects the disabled_at filter
#[tokio::test]
async fn test_find_user_by_id_respects_disabled_filter() {
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

    // Create active user
    let active_user_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (id, username, email, password_hash, tenant_id) VALUES ($1, $2, $3, $4, $5) RETURNING id"
    )
    .bind(Uuid::new_v4())
    .bind(format!("active{}", test_id))
    .bind(format!("active{}@example.com", test_id))
    .bind("hash")
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to create active user");

    // Create disabled user
    let disabled_user_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (id, username, email, password_hash, tenant_id, disabled_at) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id"
    )
    .bind(Uuid::new_v4())
    .bind(format!("disabled{}", test_id))
    .bind(format!("disabled{}@example.com", test_id))
    .bind("hash")
    .bind(tenant_id)
    .bind(chrono::Utc::now())
    .fetch_one(&pool)
    .await
    .expect("Failed to create disabled user");

    // Create compat layer
    let compat = create_test_compat(pool.clone()).await;

    // Test: Active user should be found
    let found_active = compat
        .find_user_by_id(active_user_id)
        .await
        .expect("find_user_by_id should not fail");
    assert!(found_active.is_some(), "Active user should be found");

    // Test: Disabled user should NOT be found (disabled_at IS NOT NULL filter)
    let found_disabled = compat
        .find_user_by_id(disabled_user_id)
        .await
        .expect("find_user_by_id should not fail");
    assert!(
        found_disabled.is_none(),
        "Disabled user should not be found via compat layer"
    );

    // Cleanup
    sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
        .bind(active_user_id)
        .bind(disabled_user_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .ok();
}

/// Test that is_user_in_group correctly handles edge cases
#[tokio::test]
async fn test_is_user_in_group_edge_cases() {
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
        "INSERT INTO users (id, username, email, password_hash, tenant_id) VALUES ($1, $2, $3, $4, $5) RETURNING id"
    )
    .bind(Uuid::new_v4())
    .bind(format!("user{}", test_id))
    .bind(format!("user{}@example.com", test_id))
    .bind("hash")
    .bind(tenant_id)
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

    // Create compat layer
    let compat = create_test_compat(pool.clone()).await;

    // Test 1: User not in group (no membership row)
    let is_member = compat
        .is_user_in_group(user_id, group_id)
        .await
        .expect("is_user_in_group should not fail");
    assert!(
        !is_member,
        "User should not be in group without membership row"
    );

    // Add user to group
    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
        .bind(group_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("Failed to add user to group");

    // Test 2: User is now in group
    let is_member = compat
        .is_user_in_group(user_id, group_id)
        .await
        .expect("is_user_in_group should not fail");
    assert!(is_member, "User should be in group after adding membership");

    // Test 3: Non-existent user in existing group
    let fake_user_id = Uuid::new_v4();
    let is_fake_member = compat
        .is_user_in_group(fake_user_id, group_id)
        .await
        .expect("is_user_in_group should not fail for non-existent user");
    assert!(!is_fake_member, "Non-existent user should not be in group");

    // Test 4: Existing user in non-existent group
    let fake_group_id = Uuid::new_v4();
    let is_fake_group = compat
        .is_user_in_group(user_id, fake_group_id)
        .await
        .expect("is_user_in_group should not fail for non-existent group");
    assert!(!is_fake_group, "User should not be in non-existent group");

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

/// Helper function to create a MetadataStoreCompat for testing
/// Uses a minimal mock repository since the SQL methods only need the pool
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

    // Minimal mock implementations that do nothing (the SQL methods only need the pool)
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
        async fn remove_from_index(&self, _resource_id: uuid::Uuid) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn search(
            &self,
            _tenant_id: uuid::Uuid,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<SearchResult>, RepositoryError> {
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
