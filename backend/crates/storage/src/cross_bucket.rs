//! Cross-Bucket Reader Implementation
//!
//! Provides read access to resources in other users' buckets using
//! Portable Storage Locators.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::user_bucket::{UserBucketStore, UserId};

/// Portable Storage Locator for cross-user references
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortableStorageLocator {
    pub locator_version: u32,
    pub storage_provider_kind: String,
    pub endpoint_ref: String,
    pub bucket: String,
    pub key: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub version_id: Option<String>,
    pub content_hash: Option<String>,
}

impl PortableStorageLocator {
    /// Create a new S3 locator
    pub fn new_s3(
        endpoint_ref: impl Into<String>,
        bucket: impl Into<String>,
        key: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: Uuid,
    ) -> Self {
        Self {
            locator_version: 1,
            storage_provider_kind: "s3".to_string(),
            endpoint_ref: endpoint_ref.into(),
            bucket: bucket.into(),
            key: key.into(),
            resource_type: resource_type.into(),
            resource_id,
            version_id: None,
            content_hash: None,
        }
    }

    /// Create a locator for a file in a user's bucket
    pub fn for_file(
        bucket_prefix: &str,
        owner_id: UserId,
        file_id: Uuid,
        content_hash: Option<String>,
    ) -> Self {
        let bucket = format!("{}{}", bucket_prefix, owner_id);
        let key = format!("owned/files/{}.json", file_id);

        Self {
            locator_version: 1,
            storage_provider_kind: "s3".to_string(),
            endpoint_ref: "primary".to_string(),
            bucket,
            key,
            resource_type: "file".to_string(),
            resource_id: file_id,
            version_id: None,
            content_hash: content_hash.map(|h| format!("sha256:{}", h)),
        }
    }

    /// Create a locator for a folder in a user's bucket
    pub fn for_folder(bucket_prefix: &str, owner_id: UserId, folder_id: Uuid) -> Self {
        let bucket = format!("{}{}", bucket_prefix, owner_id);
        let key = format!("owned/folders/{}.json", folder_id);

        Self {
            locator_version: 1,
            storage_provider_kind: "s3".to_string(),
            endpoint_ref: "primary".to_string(),
            bucket,
            key,
            resource_type: "folder".to_string(),
            resource_id: folder_id,
            version_id: None,
            content_hash: None,
        }
    }

    /// Extract user ID from bucket name
    pub fn extract_user_id(&self, bucket_prefix: &str) -> Option<UserId> {
        self.bucket
            .strip_prefix(bucket_prefix)
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    /// Update endpoint reference (for relocation)
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint_ref = endpoint.into();
        self
    }

    /// Update bucket (for relocation)
    pub fn with_bucket(mut self, bucket: impl Into<String>) -> Self {
        self.bucket = bucket.into();
        self
    }
}

/// Trait for cross-bucket reading
#[async_trait]
pub trait CrossBucketReader: Send + Sync {
    /// Read object from another user's bucket using locator
    async fn read_with_locator(&self, locator: &PortableStorageLocator) -> Result<Option<Bytes>>;

    /// Check if locator points to accessible resource
    async fn check_locator(&self, locator: &PortableStorageLocator) -> Result<bool>;
}

/// Extension trait for typed cross-bucket reading
pub trait CrossBucketReaderExt: CrossBucketReader {
    /// Read and deserialize object
    fn read_typed<T: serde::de::DeserializeOwned>(
        &self,
        locator: &PortableStorageLocator,
    ) -> impl std::future::Future<Output = Result<Option<T>>> + Send;
}

impl<R: CrossBucketReader + ?Sized> CrossBucketReaderExt for R {
    async fn read_typed<T: serde::de::DeserializeOwned>(
        &self,
        locator: &PortableStorageLocator,
    ) -> Result<Option<T>> {
        match self.read_with_locator(locator).await? {
            Some(data) => {
                let obj = serde_json::from_slice(&data)?;
                Ok(Some(obj))
            }
            None => Ok(None),
        }
    }
}

/// Cross-bucket reader using UserBucketStore
pub struct UserBucketCrossReader {
    user_buckets: Arc<dyn UserBucketStore>,
    bucket_prefix: String,
}

impl UserBucketCrossReader {
    /// Create a new cross-bucket reader
    pub fn new(user_buckets: Arc<dyn UserBucketStore>, bucket_prefix: String) -> Self {
        Self {
            user_buckets,
            bucket_prefix,
        }
    }
}

#[async_trait]
impl CrossBucketReader for UserBucketCrossReader {
    async fn read_with_locator(&self, locator: &PortableStorageLocator) -> Result<Option<Bytes>> {
        // Extract user ID from bucket name
        let user_id = locator
            .extract_user_id(&self.bucket_prefix)
            .ok_or_else(|| anyhow::anyhow!("Invalid bucket name: {}", locator.bucket))?;

        // Read using the user bucket store
        // Note: The key in the locator is the full path within the bucket
        self.user_buckets.get_object(user_id, &locator.key).await
    }

    async fn check_locator(&self, locator: &PortableStorageLocator) -> Result<bool> {
        // Extract user ID from bucket name
        let user_id = match locator.extract_user_id(&self.bucket_prefix) {
            Some(id) => id,
            None => return Ok(false),
        };

        // Check if object exists
        self.user_buckets.object_exists(user_id, &locator.key).await
    }
}

/// Factory for creating cross-bucket readers
pub struct CrossBucketReaderFactory;

impl CrossBucketReaderFactory {
    /// Create a cross-bucket reader
    pub fn create(
        user_buckets: Arc<dyn UserBucketStore>,
        bucket_prefix: String,
    ) -> Arc<dyn CrossBucketReader> {
        Arc::new(UserBucketCrossReader::new(user_buckets, bucket_prefix))
    }
}

/// In-memory cross-bucket reader for testing
/// 
/// This implementation delegates to a UserBucketStore to actually read data,
/// making it work correctly with the rest of the test infrastructure.
pub struct MemoryCrossBucketReader {
    user_buckets: Option<Arc<dyn UserBucketStore>>,
    storage: std::sync::Mutex<std::collections::HashMap<String, Bytes>>,
    bucket_prefix: String,
}

impl MemoryCrossBucketReader {
    /// Create a new memory cross-bucket reader (legacy mode - uses internal storage)
    pub fn new() -> Self {
        Self {
            user_buckets: None,
            storage: std::sync::Mutex::new(std::collections::HashMap::new()),
            bucket_prefix: "rustshare-user-".to_string(),
        }
    }

    /// Create with a UserBucketStore delegate
    pub fn with_user_buckets(user_buckets: Arc<dyn UserBucketStore>) -> Self {
        Self {
            user_buckets: Some(user_buckets),
            storage: std::sync::Mutex::new(std::collections::HashMap::new()),
            bucket_prefix: "rustshare-user-".to_string(),
        }
    }

    /// Create with custom bucket prefix
    pub fn with_prefix(bucket_prefix: String) -> Self {
        Self {
            user_buckets: None,
            storage: std::sync::Mutex::new(std::collections::HashMap::new()),
            bucket_prefix,
        }
    }

    /// Store a value for testing (only used in legacy mode)
    pub fn store(&self, bucket: &str, key: &str, data: Bytes) {
        let full_key = format!("{}/{}", bucket, key);
        let mut storage = self.storage.lock().unwrap();
        storage.insert(full_key, data);
    }

    fn build_key(&self, bucket: &str, key: &str) -> String {
        format!("{}/{}", bucket, key)
    }

    /// Extract user ID from bucket name
    fn extract_user_id(&self, bucket: &str) -> Option<Uuid> {
        let prefix = &self.bucket_prefix;
        if let Some(suffix) = bucket.strip_prefix(prefix) {
            // Handle "rustshare-user-{uuid}" format
            if let Ok(user_id) = Uuid::parse_str(suffix) {
                return Some(user_id);
            }
        }
        None
    }
}

impl Default for MemoryCrossBucketReader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CrossBucketReader for MemoryCrossBucketReader {
    async fn read_with_locator(&self, locator: &PortableStorageLocator) -> Result<Option<Bytes>> {
        // If we have a user_buckets delegate, use it
        if let Some(ref user_buckets) = self.user_buckets {
            if let Some(user_id) = self.extract_user_id(&locator.bucket) {
                return user_buckets.get_object(user_id, &locator.key).await;
            }
        }
        // Fall back to internal storage
        let key = self.build_key(&locator.bucket, &locator.key);
        let storage = self.storage.lock().unwrap();
        Ok(storage.get(&key).cloned())
    }

    async fn check_locator(&self, locator: &PortableStorageLocator) -> Result<bool> {
        // If we have a user_buckets delegate, use it
        if let Some(ref user_buckets) = self.user_buckets {
            if let Some(user_id) = self.extract_user_id(&locator.bucket) {
                return user_buckets.object_exists(user_id, &locator.key).await;
            }
        }
        // Fall back to internal storage
        let key = self.build_key(&locator.bucket, &locator.key);
        let storage = self.storage.lock().unwrap();
        Ok(storage.contains_key(&key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_bucket::MemoryUserBucketStore;

    #[test]
    fn test_locator_creation() {
        let user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        let locator = PortableStorageLocator::for_file("rustshare-user-", user_id, file_id, None);

        assert_eq!(locator.locator_version, 1);
        assert_eq!(locator.storage_provider_kind, "s3");
        assert_eq!(locator.resource_type, "file");
        assert_eq!(locator.resource_id, file_id);
        assert!(locator.bucket.contains(&user_id.to_string()));
        assert!(locator.key.contains(&file_id.to_string()));
    }

    #[test]
    fn test_extract_user_id() {
        let user_id = Uuid::new_v4();
        let locator = PortableStorageLocator::for_file("rustshare-user-", user_id, Uuid::new_v4(), None);

        let extracted = locator.extract_user_id("rustshare-user-");
        assert_eq!(extracted, Some(user_id));

        // Invalid bucket name
        let invalid = PortableStorageLocator {
            bucket: "invalid-bucket".to_string(),
            ..locator
        };
        assert_eq!(invalid.extract_user_id("rustshare-user-"), None);
    }

    #[test]
    fn test_locator_relocation() {
        let user_id = Uuid::new_v4();
        let locator = PortableStorageLocator::for_file("rustshare-user-", user_id, Uuid::new_v4(), None);

        // Relocate to different endpoint and bucket
        let relocated = locator
            .clone()
            .with_endpoint("eu-west")
            .with_bucket(format!("rustshare-user-{}-eu", user_id));

        assert_eq!(relocated.endpoint_ref, "eu-west");
        assert_eq!(relocated.resource_id, locator.resource_id); // Same resource
        assert_eq!(relocated.key, locator.key); // Same key
    }

    #[tokio::test]
    async fn test_cross_bucket_reader() {
        let user_buckets: Arc<dyn UserBucketStore> = Arc::new(MemoryUserBucketStore::new());
        let reader = CrossBucketReaderFactory::create(
            user_buckets.clone(),
            "rustshare-user-".to_string(),
        );

        let owner_id = Uuid::new_v4();
        user_buckets.create_bucket(owner_id).await.unwrap();

        let file_id = Uuid::new_v4();
        let data = Bytes::from(r#"{"id": "test", "name": "file.txt"}"#);

        // Store file in owner's bucket
        let key = format!("owned/files/{}.json", file_id);
        user_buckets.put_object(owner_id, &key, data.clone()).await.unwrap();

        // Create locator
        let locator = PortableStorageLocator::for_file("rustshare-user-", owner_id, file_id, None);

        // Read via cross-bucket reader
        let result = reader.read_with_locator(&locator).await.unwrap();
        assert_eq!(result, Some(data));

        // Check locator
        assert!(reader.check_locator(&locator).await.unwrap());

        // Non-existent locator
        let bad_locator = PortableStorageLocator::for_file(
            "rustshare-user-",
            owner_id,
            Uuid::new_v4(),
            None,
        );
        assert!(!reader.check_locator(&bad_locator).await.unwrap());
    }
}
