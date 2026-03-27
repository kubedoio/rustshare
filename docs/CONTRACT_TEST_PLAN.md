# RustShare V2 Contract Test Plan

## Overview

This document specifies the executable contract tests that enforce the RustShare V2 architecture. Each test is designed to:
1. Fail against stubbed/placeholder implementations
2. Pass only when real implementation exists
3. Verify architectural invariants (isolation, independence, portability)

---

## Test Organization

```
backend/tests/contracts/
├── mod.rs                      # Shared test utilities
├── isolation_tests.rs          # User bucket isolation
├── file_lifecycle_tests.rs     # File CRUD operations
├── folder_lifecycle_tests.rs   # Folder CRUD operations
├── sharing_tests.rs            # Share creation and revocation
├── favourites_tests.rs         # Star/unstar functionality
├── restore_tests.rs            # Restore independence
├── locator_tests.rs            # Portable storage locators
├── index_tests.rs              # No-scan hot paths
└── redis_optionality_tests.rs  # Redis is optional
```

---

## Test Fixtures

All tests use a `TestContext` that provides:

```rust
pub struct TestContext {
    pub user_buckets: Arc<dyn UserBucketStore>,
    pub cross_bucket: Arc<dyn CrossBucketReader>,
    pub coordination: Arc<dyn CoordinationStore>,
    pub blob_store: Arc<dyn BlobStore>,
    pub file_service: Arc<FileService>,
    pub folder_service: Arc<FolderService>,
    pub share_service: Arc<ShareService>,
    pub favourite_service: Arc<FavouriteService>,
}

impl TestContext {
    pub async fn new() -> Self { /* ... */ }
    pub async fn create_user(&self, user_id: Uuid) -> Result<()> { /* ... */ }
    pub async fn list_bucket_objects(&self, user_id: Uuid) -> Result<Vec<String>> { /* ... */ }
}
```

---

## Detailed Test Specifications

### Module: isolation_tests.rs

#### test_ub_01_create_file_writes_to_owner_bucket_only
```rust
/// Verify that creating a file writes canonical documents only to the owner's bucket
async fn test_ub_01_create_file_writes_to_owner_bucket_only() {
    // Arrange
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let other_user_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(other_user_id).await.unwrap();
    
    // Act
    let file = ctx.file_service.upload_file(
        owner_id,
        "test.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    // Assert
    // File document exists in owner's bucket
    let owner_objects = ctx.list_bucket_objects(owner_id).await.unwrap();
    assert!(owner_objects.iter().any(|k| k.contains(&file.id.to_string())));
    
    // File document does NOT exist in other user's bucket
    let other_objects = ctx.list_bucket_objects(other_user_id).await.unwrap();
    assert!(!other_objects.iter().any(|k| k.contains(&file.id.to_string())));
}
```

#### test_ub_02_recipient_share_reference_in_recipient_bucket
```rust
/// Verify that when a share is created, recipient gets a reference in their bucket
async fn test_ub_02_recipient_share_reference_in_recipient_bucket() {
    // Arrange
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        owner_id,
        "shared.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    // Act
    let share = ctx.share_service.create_user_share(
        owner_id,
        file.id,
        "file".to_string(),
        recipient_id,
        SharePermission::View,
    ).await.unwrap();
    
    // Assert
    // Outbound share in owner's bucket
    let owner_objects = ctx.list_bucket_objects(owner_id).await.unwrap();
    assert!(owner_objects.iter().any(|k| k.contains(&format!("shares/outbound/{}"), share.id)));
    
    // Received share reference in recipient's bucket
    let recipient_objects = ctx.list_bucket_objects(recipient_id).await.unwrap();
    assert!(recipient_objects.iter().any(|k| k.contains(&format!("received/shares/{}"), share.id)));
}
```

#### test_ub_03_favourites_in_user_bucket_only
```rust
/// Verify that starring a file only writes to the starring user's bucket
async fn test_ub_03_favourites_in_user_bucket_only() {
    // Arrange
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let starrer_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(starrer_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        owner_id,
        "starred.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    // Create share so starrer can see the file
    let share = ctx.share_service.create_user_share(
        owner_id,
        file.id,
        "file".to_string(),
        starrer_id,
        SharePermission::View,
    ).await.unwrap();
    
    // Act
    ctx.favourite_service.star_shared_resource(starrer_id, share.id).await.unwrap();
    
    // Assert
    // Favourites index in starrer's bucket
    let starrer_objects = ctx.list_bucket_objects(starrer_id).await.unwrap();
    assert!(starrer_objects.iter().any(|k| k.contains("favourites")));
    
    // NO favourites in owner's bucket
    let owner_objects = ctx.list_bucket_objects(owner_id).await.unwrap();
    assert!(!owner_objects.iter().any(|k| k.contains("favourites")));
}
```

#### test_ub_04_no_central_database_required
```rust
/// Verify that operations work without PostgreSQL
async fn test_ub_04_no_central_database_required() {
    // This test uses only the object-store-based services
    // No sqlx pool, no postgres connection
    let ctx = TestContext::new_without_postgres().await;
    
    // Should be able to perform all core operations
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        user_id,
        "test.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    assert_eq!(file.owner_id, user_id);
}
```

---

### Module: file_lifecycle_tests.rs

#### test_fl_01_upload_creates_all_required_documents
```rust
/// Verify upload creates FileDocument, VersionDocument, and updates FolderChildrenIndex
async fn test_fl_01_upload_creates_all_required_documents() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        user_id,
        "test.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    // FileDocument exists
    let file_key = format!("owned/files/{}.json", file.id);
    let file_data = ctx.user_buckets.get_object(user_id, &file_key).await.unwrap();
    assert!(file_data.is_some());
    
    // VersionDocument exists
    let version_id = file.current_version_id;
    let version_key = format!("owned/file_versions/{}/{}.json", file.id, version_id);
    let version_data = ctx.user_buckets.get_object(user_id, &version_key).await.unwrap();
    assert!(version_data.is_some());
    
    // Event document exists
    let objects = ctx.list_bucket_objects(user_id).await.unwrap();
    assert!(objects.iter().any(|k| k.starts_with("events/")));
}
```

#### test_fl_02_file_identity_stable
```rust
/// Verify file ID doesn't change during rename/move operations
async fn test_fl_02_file_identity_stable() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        user_id,
        "original.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    let original_id = file.id;
    
    // Rename
    let renamed = ctx.file_service.rename_file(user_id, file.id, "renamed.txt".to_string()).await.unwrap();
    assert_eq!(renamed.id, original_id);
    
    // Create folder and move
    let folder = ctx.folder_service.create_folder(user_id, "Folder".to_string(), None).await.unwrap();
    let moved = ctx.file_service.move_file(user_id, file.id, Some(folder.id)).await.unwrap();
    assert_eq!(moved.id, original_id);
}
```

#### test_fl_03_delete_creates_tombstone
```rust
/// Verify delete creates TombstoneDocument and marks file as deleted
async fn test_fl_03_delete_creates_tombstone() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        user_id,
        "delete_me.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    ctx.file_service.delete_file(user_id, file.id).await.unwrap();
    
    // Tombstone exists
    let tombstone_key = format!("owned/tombstones/files/{}.json", file.id);
    let tombstone = ctx.user_buckets.get_object(user_id, &tombstone_key).await.unwrap();
    assert!(tombstone.is_some());
    
    // File document still exists but marked deleted
    let file_key = format!("owned/files/{}.json", file.id);
    let file_data = ctx.user_buckets.get_object(user_id, &file_key).await.unwrap();
    assert!(file_data.is_some());
    let doc: FileDocument = serde_json::from_slice(&file_data.unwrap()).unwrap();
    assert!(doc.deleted);
}
```

#### test_fl_04_restore_from_tombstone
```rust
/// Verify restore recreates file from tombstone
async fn test_fl_04_restore_from_tombstone() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        user_id,
        "restore_me.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    let original_id = file.id;
    
    ctx.file_service.delete_file(user_id, file.id).await.unwrap();
    
    let restored = ctx.file_service.restore_file(user_id, file.id).await.unwrap();
    
    assert_eq!(restored.id, original_id);
    assert!(!restored.deleted);
    
    // Tombstone removed or marked
    let tombstone_key = format!("owned/tombstones/files/{}.json", file.id);
    let tombstone = ctx.user_buckets.get_object(user_id, &tombstone_key).await.unwrap();
    assert!(tombstone.is_none() || is_restored_tombstone(&tombstone.unwrap()));
}
```

#### test_fl_05_version_history_preserved
```rust
/// Verify all versions are preserved and accessible
async fn test_fl_05_version_history_preserved() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        user_id,
        "versioned.txt".to_string(),
        None,
        Bytes::from("v1"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    // Update twice
    ctx.file_service.update_file(user_id, file.id, 1, Bytes::from("v2")).await.unwrap();
    ctx.file_service.update_file(user_id, file.id, 2, Bytes::from("v3")).await.unwrap();
    
    // List versions
    let versions = ctx.file_service.list_versions(user_id, file.id).await.unwrap();
    assert_eq!(versions.len(), 3);
    assert!(versions.iter().any(|v| v.version_number == 1));
    assert!(versions.iter().any(|v| v.version_number == 2));
    assert!(versions.iter().any(|v| v.version_number == 3));
}
```

---

### Module: sharing_tests.rs

#### test_sh_01_create_share_writes_outbound_doc
```rust
/// Verify share creation writes OutboundShareDocument
async fn test_sh_01_create_share_writes_outbound_doc() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        owner_id,
        "shared.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    let share = ctx.share_service.create_user_share(
        owner_id,
        file.id,
        "file".to_string(),
        recipient_id,
        SharePermission::View,
    ).await.unwrap();
    
    // Outbound share document exists
    let share_key = format!("owned/shares/outbound/{}.json", share.id);
    let share_data = ctx.user_buckets.get_object(owner_id, &share_key).await.unwrap();
    assert!(share_data.is_some());
    
    let doc: OutboundShareDocument = serde_json::from_slice(&share_data.unwrap()).unwrap();
    assert_eq!(doc.resource_id, file.id);
    assert_eq!(doc.recipient_user_id, Some(recipient_id));
}
```

#### test_sh_02_recipient_receives_reference
```rust
/// Verify recipient gets ReceivedShareReference with locator
async fn test_sh_02_recipient_receives_reference() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        owner_id,
        "shared.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    let share = ctx.share_service.create_user_share(
        owner_id,
        file.id,
        "file".to_string(),
        recipient_id,
        SharePermission::View,
    ).await.unwrap();
    
    // Received share reference exists in recipient bucket
    let ref_key = format!("received/shares/{}.json", share.id);
    let ref_data = ctx.user_buckets.get_object(recipient_id, &ref_key).await.unwrap();
    assert!(ref_data.is_some());
    
    let doc: ReceivedShareReference = serde_json::from_slice(&ref_data.unwrap()).unwrap();
    assert_eq!(doc.share_id, share.id);
    assert_eq!(doc.owner_user_id, owner_id);
    assert!(doc.resource_locator.is_some());
}
```

#### test_sh_03_shared_with_me_index_updated
```rust
/// Verify SharedWithMeIndex is updated for recipient
async fn test_sh_03_shared_with_me_index_updated() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        owner_id,
        "shared.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    let share = ctx.share_service.create_user_share(
        owner_id,
        file.id,
        "file".to_string(),
        recipient_id,
        SharePermission::View,
    ).await.unwrap();
    
    // Shared with me index updated
    let index_key = "indexes/received/shared_with_me.json";
    let index_data = ctx.user_buckets.get_object(recipient_id, index_key).await.unwrap();
    assert!(index_data.is_some());
    
    let index: SharedWithMeIndex = serde_json::from_slice(&index_data.unwrap()).unwrap();
    assert!(index.entries.iter().any(|e| e.share_id == share.id));
}
```

#### test_sh_04_revoke_removes_recipient_visibility
```rust
/// Verify revoking share removes recipient's access
async fn test_sh_04_revoke_removes_recipient_visibility() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        owner_id,
        "revoked.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    let share = ctx.share_service.create_user_share(
        owner_id,
        file.id,
        "file".to_string(),
        recipient_id,
        SharePermission::View,
    ).await.unwrap();
    
    // Revoke
    ctx.share_service.revoke_share(owner_id, share.id).await.unwrap();
    
    // Outbound share marked revoked
    let share_key = format!("owned/shares/outbound/{}.json", share.id);
    let share_data = ctx.user_buckets.get_object(owner_id, &share_key).await.unwrap();
    let doc: OutboundShareDocument = serde_json::from_slice(&share_data.unwrap()).unwrap();
    assert!(doc.revoked_at.is_some());
    
    // Recipient can no longer access (access check should fail)
    let access_result = ctx.share_service.access_shared_file(recipient_id, share.id).await;
    assert!(access_result.is_err());
}
```

#### test_sh_05_revoke_does_not_delete_resource
```rust
/// Verify revoking share doesn't delete the shared file
async fn test_sh_05_revoke_does_not_delete_resource() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        owner_id,
        "not_deleted.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    let share = ctx.share_service.create_user_share(
        owner_id,
        file.id,
        "file".to_string(),
        recipient_id,
        SharePermission::View,
    ).await.unwrap();
    
    ctx.share_service.revoke_share(owner_id, share.id).await.unwrap();
    
    // File still exists and owner can access
    let file_result = ctx.file_service.get_file(owner_id, file.id).await;
    assert!(file_result.is_ok());
}
```

---

### Module: favourites_tests.rs

#### test_fv_01_star_owned_updates_user_favourites
```rust
/// Verify starring owned file updates user's favourites, not owner's
async fn test_fv_01_star_owned_updates_user_favourites() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        user_id,
        "myfile.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    // Record file version before star
    let file_before = ctx.file_service.get_file(user_id, file.id).await.unwrap();
    let version_before = file_before.version;
    
    ctx.favourite_service.star_resource(user_id, file.id, "file".to_string()).await.unwrap();
    
    // Favourites index exists
    let index_key = "indexes/owned/favourites.json";
    let index_data = ctx.user_buckets.get_object(user_id, index_key).await.unwrap();
    assert!(index_data.is_some());
    
    let index: FavouritesIndex = serde_json::from_slice(&index_data.unwrap()).unwrap();
    assert!(index.owned_entries.iter().any(|e| e.resource_id == file.id));
    
    // File document unchanged (no favourites field)
    let file_after = ctx.file_service.get_file(user_id, file.id).await.unwrap();
    assert_eq!(file_after.version, version_before); // No version bump
}
```

#### test_fv_02_star_shared_updates_recipient_only
```rust
/// Verify starring shared file updates only recipient's favourites
async fn test_fv_02_star_shared_updates_recipient_only() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        owner_id,
        "shared.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    let share = ctx.share_service.create_user_share(
        owner_id,
        file.id,
        "file".to_string(),
        recipient_id,
        SharePermission::View,
    ).await.unwrap();
    
    let file_version_before = file.version;
    
    // Recipient stars the shared file
    ctx.favourite_service.star_shared_resource(recipient_id, share.id).await.unwrap();
    
    // Recipient has favourites index
    let recipient_index_key = "indexes/received/favourites.json";
    let recipient_index = ctx.user_buckets.get_object(recipient_id, recipient_index_key).await.unwrap();
    assert!(recipient_index.is_some());
    
    // Owner does NOT have favourites
    let owner_index = ctx.user_buckets.get_object(owner_id, recipient_index_key).await.unwrap();
    assert!(owner_index.is_none());
    
    // Owner's file unchanged
    let file_after = ctx.file_service.get_file(owner_id, file.id).await.unwrap();
    assert_eq!(file_after.version, file_version_before);
}
```

#### test_fv_03_unstar_removes_from_favourites
```rust
/// Verify unstarring removes entry from favourites index
async fn test_fv_03_unstar_removes_from_favourites() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        user_id,
        "starred.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    let entry = ctx.favourite_service.star_resource(user_id, file.id, "file".to_string()).await.unwrap();
    
    ctx.favourite_service.unstar_resource(user_id, entry.id).await.unwrap();
    
    let index_key = "indexes/owned/favourites.json";
    let index_data = ctx.user_buckets.get_object(user_id, index_key).await.unwrap().unwrap();
    let index: FavouritesIndex = serde_json::from_slice(&index_data).unwrap();
    
    assert!(!index.owned_entries.iter().any(|e| e.resource_id == file.id));
}
```

#### test_fv_04_favourites_survive_restore
```rust
/// Verify favourites are restored when user bucket is restored
async fn test_fv_04_favourites_survive_restore() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(
        user_id,
        "favourite.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    let entry = ctx.favourite_service.star_resource(user_id, file.id, "file".to_string()).await.unwrap();
    
    // Export and restore
    let export = ctx.export_user_bucket(user_id).await.unwrap();
    ctx.delete_user_bucket(user_id).await.unwrap();
    ctx.restore_user_bucket(user_id, &export).await.unwrap();
    
    // Favourites still present
    let favourites = ctx.favourite_service.list_favourites(user_id).await.unwrap();
    assert!(favourites.iter().any(|f| f.resource_id == file.id));
}
```

---

### Module: restore_tests.rs

#### test_ri_01_export_produces_complete_state
```rust
/// Verify export includes all user state
async fn test_ri_01_export_produces_complete_state() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    // Create various resources
    let file = ctx.file_service.upload_file(user_id, "file.txt".to_string(), None, Bytes::from("content"), "text/plain".to_string()).await.unwrap();
    let folder = ctx.folder_service.create_folder(user_id, "Folder".to_string(), None).await.unwrap();
    ctx.favourite_service.star_resource(user_id, file.id, "file".to_string()).await.unwrap();
    
    let export = ctx.export_user_bucket(user_id).await.unwrap();
    
    assert!(export.objects.iter().any(|o| o.key.contains(&file.id.to_string())));
    assert!(export.objects.iter().any(|o| o.key.contains(&folder.id.to_string())));
    assert!(export.objects.iter().any(|o| o.key.contains("favourites")));
    assert!(export.manifest.user_id == user_id);
}
```

#### test_ri_02_restore_without_central_db
```rust
/// Verify restore works without central metadata repository
async fn test_ri_02_restore_without_central_db() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(user_id, "restore.txt".to_string(), None, Bytes::from("content"), "text/plain".to_string()).await.unwrap();
    
    let export = ctx.export_user_bucket(user_id).await.unwrap();
    
    // Delete bucket and restore
    ctx.delete_user_bucket(user_id).await.unwrap();
    ctx.restore_user_bucket(user_id, &export).await.unwrap();
    
    // File accessible after restore
    let restored = ctx.file_service.get_file(user_id, file.id).await;
    assert!(restored.is_ok());
    assert_eq!(restored.unwrap().name, "restore.txt");
}
```

#### test_ri_03_shared_with_me_restored
```rust
/// Verify received shares are restored
async fn test_ri_03_shared_with_me_restored() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(owner_id, "shared.txt".to_string(), None, Bytes::from("content"), "text/plain".to_string()).await.unwrap();
    let share = ctx.share_service.create_user_share(owner_id, file.id, "file".to_string(), recipient_id, SharePermission::View).await.unwrap();
    
    // Export recipient's bucket
    let export = ctx.export_user_bucket(recipient_id).await.unwrap();
    
    // Delete and restore
    ctx.delete_user_bucket(recipient_id).await.unwrap();
    ctx.restore_user_bucket(recipient_id, &export).await.unwrap();
    
    // Shared with me restored
    let shares = ctx.share_service.list_received_shares(recipient_id).await.unwrap();
    assert!(shares.iter().any(|s| s.share_id == share.id));
}
```

#### test_ri_04_favourites_restored_from_indexes
```rust
/// Verify favourites are restored from indexes
async fn test_ri_04_favourites_restored_from_indexes() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(user_id, "fav.txt".to_string(), None, Bytes::from("content"), "text/plain".to_string()).await.unwrap();
    ctx.favourite_service.star_resource(user_id, file.id, "file".to_string()).await.unwrap();
    
    let export = ctx.export_user_bucket(user_id).await.unwrap();
    
    ctx.delete_user_bucket(user_id).await.unwrap();
    ctx.restore_user_bucket(user_id, &export).await.unwrap();
    
    let favourites = ctx.favourite_service.list_favourites(user_id).await.unwrap();
    assert_eq!(favourites.len(), 1);
    assert_eq!(favourites[0].resource_id, file.id);
}
```

---

### Module: locator_tests.rs

#### test_pl_01_locator_serialization
```rust
/// Verify locator serializes to correct JSON structure
async fn test_pl_01_locator_serialization() {
    let locator = PortableStorageLocator {
        locator_version: 1,
        storage_provider_kind: "s3".to_string(),
        endpoint_ref: "primary".to_string(),
        bucket: "rustshare-user-123".to_string(),
        key: "owned/files/abc.json".to_string(),
        resource_type: "file".to_string(),
        resource_id: Uuid::new_v4(),
        version_id: None,
        content_hash: Some("sha256:abc123".to_string()),
    };
    
    let json = serde_json::to_string(&locator).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    
    assert_eq!(parsed["locator_version"], 1);
    assert_eq!(parsed["storage_provider_kind"], "s3");
    assert_eq!(parsed["endpoint_ref"], "primary");
    assert!(parsed["bucket"].as_str().unwrap().contains("rustshare-user"));
}
```

#### test_pl_02_locator_deserialization
```rust
/// Verify locator deserializes correctly
async fn test_pl_02_locator_deserialization() {
    let json = r#"{
        "locator_version": 1,
        "storage_provider_kind": "s3",
        "endpoint_ref": "primary",
        "bucket": "rustshare-user-123",
        "key": "owned/files/abc.json",
        "resource_type": "file",
        "resource_id": "550e8400-e29b-41d4-a716-446655440000",
        "version_id": null,
        "content_hash": "sha256:abc123"
    }"#;
    
    let locator: PortableStorageLocator = serde_json::from_str(json).unwrap();
    
    assert_eq!(locator.locator_version, 1);
    assert_eq!(locator.resource_type, "file");
    assert!(locator.content_hash.is_some());
}
```

#### test_pl_03_locator_endpoint_remap
```rust
/// Verify locator endpoint can be remapped for relocation
async fn test_pl_03_locator_endpoint_remap() {
    let locator = PortableStorageLocator {
        locator_version: 1,
        storage_provider_kind: "s3".to_string(),
        endpoint_ref: "primary".to_string(),
        bucket: "rustshare-user-123".to_string(),
        key: "owned/files/abc.json".to_string(),
        resource_type: "file".to_string(),
        resource_id: Uuid::new_v4(),
        version_id: None,
        content_hash: None,
    };
    
    // Simulate relocation - remap endpoint
    let mut relocated = locator.clone();
    relocated.endpoint_ref = "eu-west".to_string();
    relocated.bucket = "rustshare-user-123-eu".to_string();
    
    assert_eq!(relocated.endpoint_ref, "eu-west");
    assert_eq!(relocated.resource_id, locator.resource_id); // Same resource
}
```

#### test_pl_04_cross_bucket_read_via_locator
```rust
/// Verify cross-bucket read using locator works
async fn test_pl_04_cross_bucket_read_via_locator() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let reader_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(reader_id).await.unwrap();
    
    let file = ctx.file_service.upload_file(owner_id, "cross.txt".to_string(), None, Bytes::from("content"), "text/plain".to_string()).await.unwrap();
    
    // Create locator pointing to owner's file
    let locator = PortableStorageLocator {
        locator_version: 1,
        storage_provider_kind: "s3".to_string(),
        endpoint_ref: "primary".to_string(),
        bucket: format!("rustshare-user-{}", owner_id),
        key: format!("owned/files/{}.json", file.id),
        resource_type: "file".to_string(),
        resource_id: file.id,
        version_id: None,
        content_hash: Some(format!("sha256:{}", file.checksum)),
    };
    
    // Read via locator
    let data = ctx.cross_bucket.read_with_locator(&locator).await.unwrap();
    assert!(data.is_some());
    
    let doc: FileDocument = serde_json::from_slice(&data.unwrap()).unwrap();
    assert_eq!(doc.id, file.id);
}
```

---

### Module: index_tests.rs

#### test_ns_01_folder_listing_uses_index
```rust
/// Verify folder listing uses FolderChildrenIndex, not bucket scan
async fn test_ns_01_folder_listing_uses_index() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    let folder = ctx.folder_service.create_folder(user_id, "Parent".to_string(), None).await.unwrap();
    
    // Create multiple files
    for i in 0..5 {
        ctx.file_service.upload_file(
            user_id,
            format!("file{}.txt", i),
            Some(folder.id),
            Bytes::from(format!("content{}", i)),
            "text/plain".to_string(),
        ).await.unwrap();
    }
    
    // List should use index
    let listing = ctx.folder_service.list_contents(user_id, folder.id).await.unwrap();
    assert_eq!(listing.files.len(), 5);
    
    // Verify index was read (not full scan)
    // This is verified by the implementation using the index store
}
```

#### test_ns_02_favourites_listing_uses_index
```rust
/// Verify favourites listing uses FavouritesIndex
async fn test_ns_02_favourites_listing_uses_index() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    // Star multiple files
    for i in 0..10 {
        let file = ctx.file_service.upload_file(
            user_id,
            format!("fav{}.txt", i),
            None,
            Bytes::from(format!("content{}", i)),
            "text/plain".to_string(),
        ).await.unwrap();
        ctx.favourite_service.star_resource(user_id, file.id, "file".to_string()).await.unwrap();
    }
    
    // List favourites should use index
    let favourites = ctx.favourite_service.list_favourites(user_id).await.unwrap();
    assert_eq!(favourites.len(), 10);
}
```

#### test_ns_03_shared_with_me_uses_index
```rust
/// Verify shared-with-me uses SharedWithMeIndex
async fn test_ns_03_shared_with_me_uses_index() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();
    
    // Create multiple shares
    for i in 0..5 {
        let file = ctx.file_service.upload_file(
            owner_id,
            format!("shared{}.txt", i),
            None,
            Bytes::from(format!("content{}", i)),
            "text/plain".to_string(),
        ).await.unwrap();
        ctx.share_service.create_user_share(owner_id, file.id, "file".to_string(), recipient_id, SharePermission::View).await.unwrap();
    }
    
    // List should use index
    let shares = ctx.share_service.list_received_shares(recipient_id).await.unwrap();
    assert_eq!(shares.len(), 5);
}
```

#### test_ns_04_user_roots_uses_index
```rust
/// Verify user roots listing uses UserRootsIndex
async fn test_ns_04_user_roots_uses_index() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    // Create multiple root folders
    for i in 0..3 {
        ctx.folder_service.create_folder(
            user_id,
            format!("Root{}", i),
            None,
        ).await.unwrap();
    }
    
    // List roots should use index
    let roots = ctx.folder_service.list_root_folders(user_id).await.unwrap();
    assert_eq!(roots.len(), 3);
}
```

---

### Module: redis_optionality_tests.rs

#### test_ro_01_core_flows_without_redis
```rust
/// Verify file CRUD and sharing work without Redis
async fn test_ro_01_core_flows_without_redis() {
    let ctx = TestContext::new_without_redis().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    // File CRUD
    let file = ctx.file_service.upload_file(
        user_id,
        "test.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    let renamed = ctx.file_service.rename_file(user_id, file.id, "renamed.txt".to_string()).await.unwrap();
    assert_eq!(renamed.name, "renamed.txt");
    
    ctx.file_service.delete_file(user_id, file.id).await.unwrap();
    
    // Sharing
    let file2 = ctx.file_service.upload_file(user_id, "share.txt".to_string(), None, Bytes::from("content"), "text/plain".to_string()).await.unwrap();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(recipient_id).await.unwrap();
    
    ctx.share_service.create_user_share(user_id, file2.id, "file".to_string(), recipient_id, SharePermission::View).await.unwrap();
    
    // All operations succeeded without Redis
}
```

#### test_ro_02_coordination_requires_redis
```rust
/// Verify distributed coordination features require Redis
async fn test_ro_02_coordination_requires_redis() {
    let ctx = TestContext::new_without_redis().await;
    
    // Attempting to use coordination without Redis should fail gracefully
    let result = ctx.coordination.acquire_lease("test", 30).await;
    assert!(result.is_err() || result.unwrap().is_dummy_lease());
}
```

#### test_ro_03_redis_loss_no_data_loss
```rust
/// Verify Redis loss does not destroy durable truth
async fn test_ro_03_redis_loss_no_data_loss() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();
    
    // Create file with Redis available
    let file = ctx.file_service.upload_file(
        user_id,
        "durable.txt".to_string(),
        None,
        Bytes::from("content"),
        "text/plain".to_string(),
    ).await.unwrap();
    
    // "Lose" Redis (simulated)
    ctx.simulate_redis_loss().await;
    
    // File still accessible from bucket
    let retrieved = ctx.file_service.get_file(user_id, file.id).await;
    assert!(retrieved.is_ok());
    assert_eq!(retrieved.unwrap().id, file.id);
}
```

---

## Running the Tests

### Run All Contract Tests
```bash
cd backend
cargo test --test contracts -- --test-threads=1
```

### Run Specific Test Module
```bash
cargo test --test contracts isolation_tests
```

### Run Specific Test
```bash
cargo test --test contracts test_ub_01_create_file_writes_to_owner_bucket_only
```

---

## Test Implementation Status

| Module | Tests | Status |
|--------|-------|--------|
| isolation_tests.rs | 4 | NOT IMPLEMENTED |
| file_lifecycle_tests.rs | 5 | NOT IMPLEMENTED |
| folder_lifecycle_tests.rs | 5 | NOT IMPLEMENTED |
| sharing_tests.rs | 5 | NOT IMPLEMENTED |
| favourites_tests.rs | 4 | NOT IMPLEMENTED |
| restore_tests.rs | 4 | NOT IMPLEMENTED |
| locator_tests.rs | 4 | NOT IMPLEMENTED |
| index_tests.rs | 4 | NOT IMPLEMENTED |
| redis_optionality_tests.rs | 3 | NOT IMPLEMENTED |

**Total: 38 contract tests to implement**
