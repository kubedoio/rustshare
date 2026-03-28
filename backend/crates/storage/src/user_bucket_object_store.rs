//! User Bucket Object Store Adapter
//!
//! This module provides an ObjectStore implementation that works with the per-user
//! bucket architecture. It parses user IDs from storage keys and routes operations
//! to the appropriate user bucket.
//!
//! Storage key format: "{user_id}/blobs/{content_hash}"

use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use rustshare_core::services::ObjectStoreOps;

use crate::user_bucket::UserBucketStore;

/// Object store implementation using per-user buckets
///
/// This adapter implements the ObjectStoreOps trait from rustshare_core by:
/// 1. Parsing the user_id from the storage key (format: "{user_id}/blobs/{hash}")
/// 2. Routing the operation to the appropriate user's bucket
/// 3. Handling blob storage in user-isolated buckets
pub struct UserBucketObjectStore {
    bucket_store: Arc<dyn UserBucketStore>,
}

impl UserBucketObjectStore {
    /// Create a new user bucket object store
    pub fn new(bucket_store: Arc<dyn UserBucketStore>) -> Self {
        Self { bucket_store }
    }

    /// Parse user_id and blob hash from storage key
    ///
    /// Key format: "{user_id}/blobs/{hash}"
    fn parse_key(&self, key: &str) -> Result<(uuid::Uuid, String)> {
        // Expected format: "{user_id}/blobs/{hash}"
        let parts: Vec<&str> = key.split('/').collect();
        
        if parts.len() != 3 || parts[1] != "blobs" {
            anyhow::bail!(
                "Invalid storage key format '{}'. Expected: '{{user_id}}/blobs/{{hash}}'",
                key
            );
        }
        
        let user_id = uuid::Uuid::parse_str(parts[0])
            .with_context(|| format!("Invalid user_id in storage key: {}", parts[0]))?;
        let hash = parts[2].to_string();
        
        Ok((user_id, hash))
    }

    /// Ensure a user's bucket exists
    async fn ensure_bucket(&self, user_id: uuid::Uuid) -> Result<()> {
        if !self.bucket_store.bucket_exists(user_id).await? {
            self.bucket_store.create_bucket(user_id).await?;
            tracing::info!(user_id = %user_id, "Created user bucket for blob storage");
        }
        Ok(())
    }
}

/// Implementation of ObjectStoreOps for per-user bucket storage
/// 
/// Note: All methods receive keys in format "{user_id}/blobs/{hash}"
impl ObjectStoreOps for UserBucketObjectStore {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        let (user_id, hash) = self.parse_key(key)?;
        
        // Ensure user bucket exists
        self.ensure_bucket(user_id).await?;
        
        // Store blob in user's bucket at "blobs/{hash}"
        let blob_key = format!("blobs/{}", hash);
        self.bucket_store.put_object(user_id, &blob_key, data).await?;
        
        tracing::debug!(
            user_id = %user_id,
            hash = %hash,
            "Stored blob in user bucket"
        );
        
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let (user_id, hash) = self.parse_key(key)?;
        
        // Check if bucket exists first
        if !self.bucket_store.bucket_exists(user_id).await? {
            return Ok(false);
        }
        
        let blob_key = format!("blobs/{}", hash);
        self.bucket_store.object_exists(user_id, &blob_key).await
    }

    async fn get_presigned_url(&self, key: &str, expiry_secs: u64) -> Result<String> {
        let (user_id, hash) = self.parse_key(key)?;
        
        // For per-user buckets, we generate a direct URL to the user's bucket
        // Format: {endpoint}/{bucket_prefix}{user_id}/blobs/{hash}?...
        let bucket_name = self.bucket_store.bucket_for_user(user_id);
        
        // Get the endpoint from environment or use default
        let endpoint = std::env::var("RUSTFS_PUBLIC_ENDPOINT")
            .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
            .unwrap_or_else(|_| "http://localhost:9000".to_string());
        
        // Construct the URL
        // Note: In a production implementation, this would generate a proper presigned URL
        // For now, we return a direct URL that requires authentication
        let url = format!(
            "{}/{}/blobs/{}?expires_in={}",
            endpoint,
            bucket_name,
            hash,
            expiry_secs
        );
        
        tracing::debug!(
            user_id = %user_id,
            hash = %hash,
            "Generated presigned URL for blob"
        );
        
        Ok(url)
    }

    async fn get(&self, key: &str) -> Result<Bytes> {
        let (user_id, hash) = self.parse_key(key)?;
        
        let blob_key = format!("blobs/{}", hash);
        
        match self.bucket_store.get_object(user_id, &blob_key).await? {
            Some(data) => Ok(data),
            None => anyhow::bail!("Blob not found: {} for user {}", hash, user_id),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let (user_id, hash) = self.parse_key(key)?;
        
        let blob_key = format!("blobs/{}", hash);
        
        self.bucket_store.delete_object(user_id, &blob_key).await?;
        
        tracing::debug!(
            user_id = %user_id,
            hash = %hash,
            "Deleted blob from user bucket"
        );
        
        Ok(())
    }
}

/// Factory for creating UserBucketObjectStore instances
pub struct UserBucketObjectStoreFactory;

impl UserBucketObjectStoreFactory {
    /// Create a new UserBucketObjectStore from a UserBucketStore
    pub fn create(bucket_store: Arc<dyn UserBucketStore>) -> Arc<UserBucketObjectStore> {
        Arc::new(UserBucketObjectStore::new(bucket_store))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_bucket::MemoryUserBucketStore;

    #[tokio::test]
    async fn test_user_bucket_object_store() {
        let bucket_store: Arc<dyn UserBucketStore> = Arc::new(MemoryUserBucketStore::new());
        let store = UserBucketObjectStore::new(bucket_store);
        
        let user_id = uuid::Uuid::new_v4();
        let hash = "abc123def456";
        let key = format!("{}/blobs/{}", user_id, hash);
        let data = Bytes::from("test blob content");
        
        // Test put
        store.put(&key, data.clone()).await.unwrap();
        
        // Test exists
        assert!(store.exists(&key).await.unwrap());
        
        // Test get
        let retrieved = store.get(&key).await.unwrap();
        assert_eq!(retrieved, data);
        
        // Test delete
        store.delete(&key).await.unwrap();
        assert!(!store.exists(&key).await.unwrap());
    }

    #[test]
    fn test_parse_key_valid() {
        let bucket_store: Arc<dyn UserBucketStore> = Arc::new(MemoryUserBucketStore::new());
        let store = UserBucketObjectStore::new(bucket_store);
        
        let user_id = uuid::Uuid::new_v4();
        let hash = "abc123";
        let key = format!("{}/blobs/{}", user_id, hash);
        
        let (parsed_user_id, parsed_hash) = store.parse_key(&key).unwrap();
        assert_eq!(parsed_user_id, user_id);
        assert_eq!(parsed_hash, hash);
    }

    #[test]
    fn test_parse_key_invalid() {
        let bucket_store: Arc<dyn UserBucketStore> = Arc::new(MemoryUserBucketStore::new());
        let store = UserBucketObjectStore::new(bucket_store);
        
        // Missing user_id
        assert!(store.parse_key("blobs/abc123").is_err());
        
        // Wrong format
        assert!(store.parse_key("invalid/key/format").is_err());
        
        // Invalid UUID
        assert!(store.parse_key("not-a-uuid/blobs/abc123").is_err());
    }
}
