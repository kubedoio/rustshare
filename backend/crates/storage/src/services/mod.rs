//! V2 Object-Store-Native Services
//!
//! These services use per-user buckets with portable storage locators.

pub mod favourite_service;
pub mod file_service;
pub mod folder_service;
pub mod indexes;
pub mod models;
pub mod paths;
pub mod share_service;

pub use favourite_service::{FavouriteServiceV2, FavouriteDetail, FavouriteError};
pub use file_service::FileServiceV2;
pub use folder_service::FolderServiceV2;
pub use share_service::{ShareServiceV2, ShareInfo};

use std::sync::Arc;

/// Service factory for creating V2 services with shared dependencies
pub struct V2ServiceFactory {
    user_buckets: Arc<dyn crate::UserBucketStore>,
    cross_bucket_reader: Arc<dyn crate::CrossBucketReader>,
    blob_store: Arc<dyn crate::BlobStore>,
    indexes: Arc<indexes::UserBucketIndexes>,
    storage_endpoint: String,
}

impl V2ServiceFactory {
    /// Create a new service factory
    pub fn new(
        user_buckets: Arc<dyn crate::UserBucketStore>,
        cross_bucket_reader: Arc<dyn crate::CrossBucketReader>,
        blob_store: Arc<dyn crate::BlobStore>,
        storage_endpoint: String,
    ) -> Self {
        let indexes = Arc::new(indexes::UserBucketIndexes::new(user_buckets.clone()));
        
        Self {
            user_buckets,
            cross_bucket_reader,
            blob_store,
            indexes,
            storage_endpoint,
        }
    }

    /// Create a file service
    pub fn file_service(&self) -> FileServiceV2 {
        FileServiceV2::new(
            self.user_buckets.clone(),
            self.blob_store.clone(),
            self.indexes.clone(),
        )
    }

    /// Create a folder service
    pub fn folder_service(&self) -> FolderServiceV2 {
        FolderServiceV2::new(
            self.user_buckets.clone(),
            self.indexes.clone(),
        )
    }

    /// Create a share service
    pub fn share_service(&self) -> ShareServiceV2 {
        ShareServiceV2::new(
            self.user_buckets.clone(),
            self.cross_bucket_reader.clone(),
            self.indexes.clone(),
            self.storage_endpoint.clone(),
        )
    }

    /// Create a favourite service
    pub fn favourite_service(&self) -> FavouriteServiceV2 {
        FavouriteServiceV2::new(
            self.user_buckets.clone(),
            self.cross_bucket_reader.clone(),
            self.indexes.clone(),
        )
    }

    /// Get the indexes (for direct index access)
    pub fn indexes(&self) -> Arc<indexes::UserBucketIndexes> {
        self.indexes.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemoryUserBucketStore, MemoryCrossBucketReader, MemoryBlobStore};

    #[tokio::test]
    async fn test_service_factory_creation() {
        let user_buckets = Arc::new(MemoryUserBucketStore::new("rustshare-user-{}".to_string()));
        let cross_bucket_reader = Arc::new(MemoryCrossBucketReader::new());
        let blob_store = Arc::new(MemoryBlobStore::new());

        let factory = V2ServiceFactory::new(
            user_buckets,
            cross_bucket_reader,
            blob_store,
            "http://localhost:9000".to_string(),
        );

        let _file_service = factory.file_service();
        let _folder_service = factory.folder_service();
        let _share_service = factory.share_service();
        let _favourite_service = factory.favourite_service();
    }
}
