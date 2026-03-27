//! RustShare V2 Contract Tests
//!
//! This test suite enforces the architectural contracts defined in:
//! - docs/RUSTSHARE_V2_SPECIFICATION.md
//! - docs/CONTRACT_TEST_PLAN.md
//!
//! Run with: `cargo test --test contracts`

// Contract test modules
mod favourites_tests;
mod file_lifecycle_tests;
mod folder_lifecycle_tests;
mod index_tests;
mod isolation_tests;
mod locator_tests;
mod redis_optionality_tests;
mod restore_tests;
mod sharing_tests;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

// Re-export types from real implementations
pub use rustshare_storage::{
    MemoryUserBucketStore, MemoryCrossBucketReader, MemoryBlobStore,
    UserBucketStore, CrossBucketReader, BlobStore,
    PortableStorageLocator,
    FileServiceV2, FolderServiceV2, ShareServiceV2, FavouriteServiceV2,
    V2ServiceFactory, ShareInfo, FavouriteDetail, FavouriteError,
    SharePermissionV2, ShareResourceTypeV2, FavouriteResourceType,
    FavouritesIndex, FileDocV2, FolderDocV2, FileVersionDocV2, TombstoneDocV2,
};

pub use rustshare_storage::services::models::{
    FolderChildrenIndex, FolderChildRef, FolderChildType,
    SharedWithMeIndex, SharedWithMeEntry, UserRootsIndex,
    OutboundShareDocV2 as OutboundShareDocument,
    ReceivedShareDocV2 as ReceivedShareDocument,
    TombstoneResourceType,
};

pub use rustshare_storage::coordination::{
    MemoryCoordinationStore, CoordinationStore,
};

pub use rustshare_core::domain::{File as DomainFile, Folder as DomainFolder};

// Type aliases for backward compatibility with test names
pub type FileDocument = FileDocV2;
pub type FileVersionDocument = FileVersionDocV2;
pub type TombstoneDocument = TombstoneDocV2;
pub type FolderDocument = FolderDocV2;

/// User ID type alias
pub type UserId = Uuid;

/// Share permission levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharePermission {
    Read,
    Write,
    Admin,
}

impl From<SharePermission> for SharePermissionV2 {
    fn from(p: SharePermission) -> Self {
        match p {
            SharePermission::Read => SharePermissionV2::Read,
            SharePermission::Write => SharePermissionV2::Write,
            SharePermission::Admin => SharePermissionV2::Admin,
        }
    }
}

impl From<SharePermissionV2> for SharePermission {
    fn from(p: SharePermissionV2) -> Self {
        match p {
            SharePermissionV2::Read => SharePermission::Read,
            SharePermissionV2::Write => SharePermission::Write,
            SharePermissionV2::Admin => SharePermission::Admin,
        }
    }
}

/// File domain type for tests
#[derive(Debug, Clone)]
pub struct File {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub path: String,
    pub owner_id: UserId,
    pub current_version_id: Uuid,
    pub version_number: i32,
    pub size: i64,
    pub mime_type: String,
    pub checksum: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
}

impl From<DomainFile> for File {
    fn from(f: DomainFile) -> Self {
        Self {
            id: f.id,
            parent_id: f.parent_folder_id,
            name: f.name,
            path: f.path,
            owner_id: f.owner_id,
            current_version_id: Uuid::new_v4(), // DomainFile doesn't have this
            version_number: f.current_version,
            size: f.size,
            mime_type: f.mime_type,
            checksum: f.content_hash,
            created_at: f.created_at,
            updated_at: f.created_at, // DomainFolder has no modified_at
            deleted: f.deleted,
        }
    }
}

/// Folder domain type for tests
#[derive(Debug, Clone)]
pub struct Folder {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub path: String,
    pub owner_id: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
}

impl From<DomainFolder> for Folder {
    fn from(f: DomainFolder) -> Self {
        Self {
            id: f.id,
            parent_id: f.parent_folder_id,
            name: f.name,
            path: f.path,
            owner_id: f.owner_id,
            created_at: f.created_at,
            updated_at: f.created_at, // DomainFolder has no modified_at
            deleted: f.deleted,
        }
    }
}

/// File version for tests
#[derive(Debug, Clone)]
pub struct FileVersion {
    pub id: Uuid,
    pub file_id: Uuid,
    pub version_number: i32,
    pub size: i64,
    pub checksum: String,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
}

impl From<rustshare_core::domain::FileVersion> for FileVersion {
    fn from(v: rustshare_core::domain::FileVersion) -> Self {
        Self {
            id: v.id,
            file_id: v.file_id,
            version_number: v.version_number,
            size: v.size,
            checksum: v.content_hash,
            created_by: v.created_by,
            created_at: v.created_at,
        }
    }
}

/// Share for tests
#[derive(Debug, Clone)]
pub struct Share {
    pub id: Uuid,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub permissions: SharePermission,
    pub recipient_user_id: Option<UserId>,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl From<ShareInfo> for Share {
    fn from(s: ShareInfo) -> Self {
        Self {
            id: s.share_id,
            resource_type: match s.resource_type {
                ShareResourceTypeV2::File => "file".to_string(),
                ShareResourceTypeV2::Folder => "folder".to_string(),
            },
            resource_id: s.resource_id,
            permissions: s.permissions.into(),
            recipient_user_id: Some(s.shared_with),
            created_by: s.shared_by,
            created_at: s.created_at,
            revoked_at: None,
        }
    }
}

/// Received share for tests
#[derive(Debug, Clone)]
pub struct ReceivedShare {
    pub share_id: Uuid,
    pub owner_user_id: UserId,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub resource_name: String,
    pub permissions: SharePermission,
    pub shared_at: DateTime<Utc>,
}

/// Folder contents for tests
#[derive(Debug, Clone)]
pub struct FolderContents {
    pub folders: Vec<Folder>,
    pub files: Vec<File>,
}

/// Favourite entry for tests
#[derive(Debug, Clone)]
pub struct FavouriteEntry {
    pub id: Uuid,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub resource_locator: Option<PortableStorageLocator>,
    pub starred_at: DateTime<Utc>,
    pub notes: Option<String>,
}

impl From<rustshare_storage::services::models::FavouriteEntry> for FavouriteEntry {
    fn from(e: rustshare_storage::services::models::FavouriteEntry) -> Self {
        Self {
            id: e.resource_id,
            resource_type: match e.resource_type {
                rustshare_storage::services::models::FavouriteResourceType::OwnedFile => "owned_file".to_string(),
                rustshare_storage::services::models::FavouriteResourceType::OwnedFolder => "owned_folder".to_string(),
                rustshare_storage::services::models::FavouriteResourceType::ReceivedFile => "received_file".to_string(),
                rustshare_storage::services::models::FavouriteResourceType::ReceivedFolder => "received_folder".to_string(),
            },
            resource_id: e.resource_id,
            resource_locator: None,
            starred_at: e.added_at,
            notes: None,
        }
    }
}

/// Received share reference (for isolation tests)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivedShareReference {
    pub schema_version: u32,
    pub id: Uuid,
    pub share_id: Uuid,
    pub owner_user_id: UserId,
    pub resource_locator: PortableStorageLocator,
    pub resource_name: String,
    pub resource_type: String,
    pub permissions: SharePermission,
    pub shared_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub hidden: bool,
    pub version: u64,
}

/// Export manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifest {
    pub user_id: UserId,
    pub exported_at: DateTime<Utc>,
    pub schema_versions: HashMap<String, u32>,
}

/// Exported object
#[derive(Debug, Clone)]
pub struct ExportedObject {
    pub key: String,
    pub data: Bytes,
}

/// User bucket export
#[derive(Debug, Clone)]
pub struct UserBucketExport {
    pub manifest: ExportManifest,
    pub objects: Vec<ExportedObject>,
}

/// Test context
pub struct TestContext {
    pub user_buckets: Arc<dyn UserBucketStore>,
    pub cross_bucket: Arc<dyn CrossBucketReader>,
    pub coordination: Arc<dyn CoordinationStore>,
    pub blob_store: Arc<dyn BlobStore>,
    pub service_factory: V2ServiceFactory,
    use_redis: bool,
}

impl TestContext {
    /// Create a new test context
    pub async fn new() -> Self {
        let user_buckets: Arc<dyn UserBucketStore> = Arc::new(MemoryUserBucketStore::new());
        let blob_store: Arc<dyn BlobStore> = Arc::new(MemoryBlobStore::new());
        let cross_bucket: Arc<dyn CrossBucketReader> = Arc::new(MemoryCrossBucketReader::new());
        let coordination: Arc<dyn CoordinationStore> = Arc::new(MemoryCoordinationStore::new());

        let service_factory = V2ServiceFactory::new(
            user_buckets.clone(),
            cross_bucket.clone(),
            blob_store.clone(),
            "http://localhost:9000".to_string(),
        );

        Self {
            user_buckets,
            cross_bucket,
            coordination,
            blob_store,
            service_factory,
            use_redis: true,
        }
    }

    /// Create without Redis
    pub async fn new_without_redis() -> Self {
        let mut ctx = Self::new().await;
        ctx.use_redis = false;
        ctx.coordination = Arc::new(MemoryCoordinationStore::unavailable());
        ctx
    }

    /// Create without PostgreSQL (same as new - no PostgreSQL dependency)
    pub async fn new_without_postgres() -> Self {
        Self::new().await
    }

    /// Get file service
    pub fn file_service(&self) -> FileServiceV2 {
        self.service_factory.file_service()
    }

    /// Get folder service
    pub fn folder_service(&self) -> FolderServiceV2 {
        self.service_factory.folder_service()
    }

    /// Get share service
    pub fn share_service(&self) -> ShareServiceV2 {
        self.service_factory.share_service()
    }

    /// Get favourite service
    pub fn favourite_service(&self) -> FavouriteServiceV2 {
        self.service_factory.favourite_service()
    }

    /// Create user bucket
    pub async fn create_user(&self, user_id: UserId) -> Result<()> {
        self.user_buckets.create_bucket(user_id).await
    }

    /// List bucket objects
    pub async fn list_bucket_objects(&self, user_id: UserId) -> Result<Vec<String>> {
        self.user_buckets.list_objects(user_id, "").await
    }

    /// Export user bucket
    pub async fn export_user_bucket(&self, user_id: UserId) -> Result<UserBucketExport> {
        let objects = self.user_buckets.list_objects(user_id, "").await?;
        let mut exported = Vec::new();

        for key in &objects {
            if let Some(data) = self.user_buckets.get_object(user_id, key).await? {
                exported.push(ExportedObject {
                    key: key.clone(),
                    data,
                });
            }
        }

        let manifest = ExportManifest {
            user_id,
            exported_at: Utc::now(),
            schema_versions: [
                ("FileDocument".to_string(), 1),
                ("FolderDocument".to_string(), 1),
                ("ShareDocument".to_string(), 1),
                ("FavouritesIndex".to_string(), 1),
            ].into_iter().collect(),
        };

        Ok(UserBucketExport { manifest, objects: exported })
    }

    /// Delete user bucket
    pub async fn delete_user_bucket(&self, user_id: UserId) -> Result<()> {
        let objects = self.user_buckets.list_objects(user_id, "").await?;
        for key in objects {
            self.user_buckets.delete_object(user_id, &key).await?;
        }
        Ok(())
    }

    /// Restore user bucket
    pub async fn restore_user_bucket(&self, user_id: UserId, export: &UserBucketExport) -> Result<()> {
        for obj in &export.objects {
            self.user_buckets.put_object(user_id, &obj.key, obj.data.clone()).await?;
        }
        Ok(())
    }

    /// Simulate Redis loss
    pub async fn simulate_redis_loss(&self) {
        tracing::warn!("Simulating Redis loss");
    }
}
