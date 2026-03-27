//! Blob Storage Abstraction
//!
//! Content-addressed blob storage for file content.
//! Blobs are stored with keys derived from their content hash.

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Blob storage trait
/// 
/// Implementations provide content-addressed storage for file data.
/// Blobs are immutable and identified by their content hash.
#[async_trait]
pub trait BlobStore: Send + Sync {
    /// Store a blob and return its content-addressed key
    async fn put(&self, key: &str, data: Bytes) -> Result<()>;

    /// Retrieve a blob by its content-addressed key
    async fn get(&self, key: &str) -> Result<Option<Bytes>>;

    /// Check if a blob exists
    async fn exists(&self, key: &str) -> Result<bool>;

    /// Delete a blob
    async fn delete(&self, key: &str) -> Result<()>;

    /// Generate a content-addressed key from data
    fn content_key(&self, content_hash: &str) -> String {
        format!("blobs/{}", content_hash)
    }
}

/// In-memory blob store for testing
pub struct MemoryBlobStore {
    storage: Arc<RwLock<HashMap<String, Bytes>>>,
}

impl MemoryBlobStore {
    /// Create a new memory blob store
    pub fn new() -> Self {
        Self {
            storage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Calculate SHA-256 hash of data
    pub fn calculate_hash(data: &Bytes) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }
}

impl Default for MemoryBlobStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BlobStore for MemoryBlobStore {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        let mut storage = self.storage.write().await;
        storage.insert(key.to_string(), data);
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Bytes>> {
        let storage = self.storage.read().await;
        Ok(storage.get(key).cloned())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let storage = self.storage.read().await;
        Ok(storage.contains_key(key))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut storage = self.storage.write().await;
        storage.remove(key);
        Ok(())
    }
}

/// S3-backed blob store
#[cfg(feature = "s3")]
pub struct S3BlobStore {
    client: aws_sdk_s3::Client,
    bucket: String,
}

#[cfg(feature = "s3")]
impl S3BlobStore {
    /// Create a new S3 blob store
    pub fn new(client: aws_sdk_s3::Client, bucket: String) -> Self {
        Self { client, bucket }
    }
}

#[cfg(feature = "s3")]
#[async_trait]
impl BlobStore for S3BlobStore {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        use aws_sdk_s3::primitives::ByteStream;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(data.to_vec()))
            .send()
            .await?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Bytes>> {
        use aws_sdk_s3::error::SdkError;
        use aws_sdk_s3::operation::get_object::GetObjectError;

        match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(response) => {
                let data = response.body.collect().await?.into_bytes();
                Ok(Some(data))
            }
            Err(SdkError::ServiceError(e)) => {
                if let GetObjectError::NoSuchKey(_) = e.err() {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        use aws_sdk_s3::error::SdkError;
        use aws_sdk_s3::operation::head_object::HeadObjectError;

        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(SdkError::ServiceError(e)) => {
                if let HeadObjectError::NotFound(_) = e.err() {
                    Ok(false)
                } else {
                    Err(e.into())
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_blob_store() {
        let store = MemoryBlobStore::new();
        let data = Bytes::from_static(b"hello world");
        let hash = MemoryBlobStore::calculate_hash(&data);
        let key = store.content_key(&hash);

        // Put
        store.put(&key, data.clone()).await.unwrap();

        // Exists
        assert!(store.exists(&key).await.unwrap());

        // Get
        let retrieved = store.get(&key).await.unwrap().unwrap();
        assert_eq!(retrieved, data);

        // Delete
        store.delete(&key).await.unwrap();
        assert!(!store.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn test_content_addressed_key() {
        let store = MemoryBlobStore::new();
        let hash = "abc123def456";
        let key = store.content_key(hash);
        assert_eq!(key, "blobs/abc123def456");
    }
}
