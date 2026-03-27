//! Storage layer for RustShare.
//!
//! Handles persistence to RustFS with optional Redis coordination.
//!
//! # Migration Notice
//!
//! The PostgreSQL-based storage has been deprecated. Use `metadata_v2` module instead.

pub mod admin;
pub mod coordination;

// Legacy modules - deprecated, will be removed in a future release
#[deprecated(since = "0.2.0", note = "Use metadata_v2 instead")]
pub mod event_store;
#[deprecated(since = "0.2.0", note = "Use metadata_v2 instead")]
pub mod metadata;

// New metadata system
pub mod metadata_v2;
pub mod object_store;
pub mod repos;
pub mod service_integration;
pub mod session;

// Re-export metadata_v2 types
pub use metadata_v2::{
    BlobStore, EventLogStore, IndexStore, MetadataBackendConfig, MetadataBackendType,
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

// Re-export legacy types with deprecation warnings
#[allow(deprecated)]
#[deprecated(since = "0.2.0", note = "Use metadata_v2::MetadataRepository instead")]
pub use metadata::MetadataStore;

#[allow(deprecated)]
#[deprecated(since = "0.2.0", note = "Use metadata_v2::EventLogStore instead")]
pub use event_store::EventStore;

#[allow(deprecated)]
#[deprecated(since = "0.2.0", note = "Use metadata_v2 types instead")]
pub use metadata::{
    OwnedPublicShare, PublicShareAccessLogEntry, ReplicationAttemptRecord, ShareAccessLogEntry,
    UserSecurityEvent, UserSecurityEventRecord,
};

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

    async fn get(&self, key: &str) -> Result<bytes::Bytes> {
        self.get(key).await
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.delete(key).await
    }
}
