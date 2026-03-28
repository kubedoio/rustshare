//! Storage layer for RustShare.
//!
//! Handles persistence to RustFS with optional Redis coordination.

pub mod admin;
pub mod blob;
pub mod coordination;
pub mod services;

// RustShare V2 - User bucket isolation
pub mod cross_bucket;
pub mod user_bucket;

// Compatibility modules (minimal stubs)
pub mod event_store;
pub mod metadata;

// New metadata system
pub mod metadata_v2;
pub mod object_store;
pub mod repos;
pub mod service_integration;
pub mod session;

// Re-export metadata_v2 types
pub use metadata_v2::{
    EventLogStore, IndexStore, MetadataBackendConfig, MetadataBackendType,
    MetadataDocumentStore, MetadataDocumentStoreExt, ObjectMetadata, PutOptions, PutResult,
    RuntimeMetadataCache,
};

// Re-export repos types
pub use repos::{
    CombinedMetadataRepository, EventRepository, FileRepository, FileVersionRepository,
    FolderChildrenIndexRepository, FolderRepository, MetadataRepository, PathBuilder,
    RepositoryError, RepositoryFactory, RepositoryFactoryConfig, ShareRepository,
    TombstoneRepository,
};

// Re-export service integration
pub use service_integration::{
    create_s3_client, init_metadata_system, MetadataAdminHandler, MetadataConfig,
    MetadataSystemBuilder,
};

// Re-export object store
pub use object_store::ObjectStore;

// Re-export blob store
pub use blob::{BlobStore, MemoryBlobStore};

// Re-export V2 services
pub use services::{
    FileServiceV2, FolderServiceV2, ShareServiceV2, FavouriteServiceV2,
    V2ServiceFactory, ShareInfo, FavouriteDetail, FavouriteError,
};
pub use services::models::{
    SharePermissionV2, ShareResourceTypeV2, FavouriteResourceType,
    FavouritesIndex, FileDocV2, FolderDocV2, FileVersionDocV2,
    OutboundShareDocV2, ReceivedShareDocV2, TombstoneDocV2,
};

// Re-export user bucket types
pub use user_bucket::{
    MemoryUserBucketStore, S3UserBucketStore, UserBucketConfig, UserBucketStore, UserId,
    UserBucketStoreFactory,
};

// Re-export user bucket store types
pub use metadata_v2::user_bucket_store::{
    UserBucketStorageSystem, UserBucketBlobStore, UserBucketEventStore,
    UserScopedDocumentStore, UserScopedStoreFactory,
};

// Re-export cross-bucket types
pub use cross_bucket::{
    CrossBucketReader, CrossBucketReaderExt, CrossBucketReaderFactory, MemoryCrossBucketReader, PortableStorageLocator,
};

// Re-export compatibility types
pub use metadata::{EventStore, MetadataStore, ShareAccessLogEntry, UserSecurityEvent, UserSecurityEventRecord};

// ObjectStore implements ObjectStoreOps trait
use anyhow::Result;
use rustshare_core::services::ObjectStoreOps as CoreObjectStoreOps;

impl CoreObjectStoreOps for ObjectStore {
    async fn put(&self, key: &str, data: bytes::Bytes) -> Result<()> {
        self.put(key, data).await
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        self.exists(key).await
    }

    async fn get_presigned_url(&self, key: &str, expires_in_secs: u64) -> Result<String> {
        self.get_presigned_url(key, expires_in_secs).await
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.delete(key).await
    }

    async fn get(&self, key: &str) -> Result<bytes::Bytes> {
        self.get(key).await
    }
}

/// Initialize storage layer with default configuration
pub async fn init_storage(
    endpoint: &str,
    region: &str,
    bucket: &str,
) -> Result<(ObjectStore, repos::RepositoryFactory)> {
    let object_store = ObjectStore::new(endpoint.to_string(), region.to_string(), bucket.to_string()).await?;
    let repo_factory = repos::RepositoryFactory::new(repos::RepositoryFactoryConfig::default());

    Ok((object_store, repo_factory))
}

/// Testing utilities
pub mod testing {
    pub use super::blob::{BlobStore, MemoryBlobStore};
    pub use super::cross_bucket::{CrossBucketReader, MemoryCrossBucketReader};
    pub use super::user_bucket::{MemoryUserBucketStore, UserBucketStore};
}
