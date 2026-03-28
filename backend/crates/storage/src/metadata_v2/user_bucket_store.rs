//! User Bucket backed Metadata Document Store
//!
//! This implementation stores all metadata documents in per-user S3 buckets,
//! providing complete data isolation and enabling per-user backup/restore.

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Datelike, Utc};
use std::sync::Arc;

use crate::metadata_v2::{MetadataDocumentStore, ObjectMetadata, PutOptions, PutResult};
use crate::user_bucket::{UserBucketStore, UserId};

/// User-scoped document store that binds a UserBucketStore to a specific user
/// 
/// This wraps UserBucketStore to implement the MetadataDocumentStore trait,
/// automatically scoping all operations to a specific user's bucket.
pub struct UserScopedDocumentStore {
    bucket_store: Arc<dyn UserBucketStore>,
    user_id: UserId,
}

impl UserScopedDocumentStore {
    /// Create a new user-scoped document store
    pub fn new(bucket_store: Arc<dyn UserBucketStore>, user_id: UserId) -> Self {
        Self {
            bucket_store,
            user_id,
        }
    }

    /// Get the user ID for this scope
    pub fn user_id(&self) -> UserId {
        self.user_id
    }

    /// Build the full key path for a metadata document
    fn build_key(&self, key: &str) -> String {
        // Keys are structured as: meta/{type}/{id}.json
        format!("meta/{}", key)
    }

    /// Ensure the user's bucket exists
    pub async fn ensure_bucket(&self) -> anyhow::Result<()> {
        if !self.bucket_store.bucket_exists(self.user_id).await? {
            self.bucket_store.create_bucket(self.user_id).await?;
            tracing::info!(user_id = %self.user_id, "Created user bucket");
        }
        Ok(())
    }
}

#[async_trait]
impl MetadataDocumentStore for UserScopedDocumentStore {
    async fn get_raw(&self, key: &str) -> anyhow::Result<Option<(Vec<u8>, ObjectMetadata)>> {
        let full_key = self.build_key(key);
        
        match self.bucket_store.get_object(self.user_id, &full_key).await? {
            Some(data) => {
                // Compute ETag from content
                let etag = format!("\"{:x}\"", md5::compute(&data));
                
                // For user bucket store, we use content-derived metadata
                let metadata = ObjectMetadata {
                    etag,
                    last_modified: Utc::now(), // In production, get from S3 HeadObject
                    content_length: data.len() as u64,
                    version_id: None,
                };
                
                Ok(Some((data.to_vec(), metadata)))
            }
            None => Ok(None),
        }
    }

    async fn head(&self, key: &str) -> anyhow::Result<Option<ObjectMetadata>> {
        // For efficiency, we check object existence via get
        // In production with S3, this would use HeadObject
        let full_key = self.build_key(key);
        
        match self.bucket_store.get_object(self.user_id, &full_key).await? {
            Some(data) => {
                let etag = format!("\"{:x}\"", md5::compute(&data));
                Ok(Some(ObjectMetadata {
                    etag,
                    last_modified: Utc::now(),
                    content_length: data.len() as u64,
                    version_id: None,
                }))
            }
            None => Ok(None),
        }
    }

    async fn put_raw(
        &self,
        key: &str,
        data: &[u8],
        _opts: PutOptions,
    ) -> anyhow::Result<PutResult> {
        let full_key = self.build_key(key);
        
        // Ensure user bucket exists
        self.ensure_bucket().await?;
        
        // Store the object
        self.bucket_store
            .put_object(self.user_id, &full_key, Bytes::from(data.to_vec()))
            .await?;
        
        let etag = format!("\"{:x}\"", md5::compute(data));
        
        Ok(PutResult {
            etag,
            version_id: None,
        })
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let full_key = self.build_key(key);
        self.bucket_store.delete_object(self.user_id, &full_key).await
    }

    async fn list_prefix(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        let full_prefix = self.build_key(prefix);
        let keys = self.bucket_store.list_objects(self.user_id, &full_prefix).await?;
        
        // Strip the "meta/" prefix from returned keys
        Ok(keys
            .into_iter()
            .map(|k| k.strip_prefix("meta/").unwrap_or(&k).to_string())
            .collect())
    }
}

/// Factory for creating user-scoped document stores
pub struct UserScopedStoreFactory {
    bucket_store: Arc<dyn UserBucketStore>,
}

impl UserScopedStoreFactory {
    /// Create a new factory
    pub fn new(bucket_store: Arc<dyn UserBucketStore>) -> Self {
        Self { bucket_store }
    }

    /// Create a scoped store for a specific user
    pub fn for_user(&self, user_id: UserId) -> Arc<UserScopedDocumentStore> {
        Arc::new(UserScopedDocumentStore::new(self.bucket_store.clone(), user_id))
    }

    /// Get the underlying bucket store
    pub fn bucket_store(&self) -> Arc<dyn UserBucketStore> {
        self.bucket_store.clone()
    }
}

/// Blob store implementation using per-user buckets
pub struct UserBucketBlobStore {
    bucket_store: Arc<dyn UserBucketStore>,
}

impl UserBucketBlobStore {
    /// Create a new user bucket blob store
    pub fn new(bucket_store: Arc<dyn UserBucketStore>) -> Self {
        Self { bucket_store }
    }

    /// Store a blob for a user
    pub async fn put(&self, user_id: UserId, hash: &str, data: Bytes) -> anyhow::Result<()> {
        // Ensure user bucket exists
        if !self.bucket_store.bucket_exists(user_id).await? {
            self.bucket_store.create_bucket(user_id).await?;
        }
        
        let key = format!("blobs/{}", hash);
        self.bucket_store.put_object(user_id, &key, data).await
    }

    /// Get a blob for a user
    pub async fn get(&self, user_id: UserId, hash: &str) -> anyhow::Result<Option<Bytes>> {
        let key = format!("blobs/{}", hash);
        self.bucket_store.get_object(user_id, &key).await
    }

    /// Check if a blob exists
    pub async fn exists(&self, user_id: UserId, hash: &str) -> anyhow::Result<bool> {
        let key = format!("blobs/{}", hash);
        self.bucket_store.object_exists(user_id, &key).await
    }

    /// Delete a blob
    pub async fn delete(&self, user_id: UserId, hash: &str) -> anyhow::Result<()> {
        let key = format!("blobs/{}", hash);
        self.bucket_store.delete_object(user_id, &key).await
    }
}

/// Event log store using per-user buckets
pub struct UserBucketEventStore {
    bucket_store: Arc<dyn UserBucketStore>,
}

impl UserBucketEventStore {
    /// Create a new user bucket event store
    pub fn new(bucket_store: Arc<dyn UserBucketStore>) -> Self {
        Self { bucket_store }
    }

    /// Build the key for an event document
    fn build_event_key(&self, occurred_at: DateTime<Utc>, event_id: &str) -> String {
        format!(
            "events/{:04}/{:02}/{:02}/{}.json",
            occurred_at.year(),
            occurred_at.month(),
            occurred_at.day(),
            event_id
        )
    }

    /// Append an event to a user's event log
    pub async fn append(
        &self,
        user_id: UserId,
        event: &crate::metadata_v2::schemas::EventDocument,
    ) -> anyhow::Result<()> {
        // Ensure user bucket exists
        if !self.bucket_store.bucket_exists(user_id).await? {
            self.bucket_store.create_bucket(user_id).await?;
        }
        
        let key = self.build_event_key(event.occurred_at, &event.id.to_string());
        let data = serde_json::to_vec(event)?;
        
        self.bucket_store
            .put_object(user_id, &key, Bytes::from(data))
            .await
    }

    /// Read events for a resource
    pub async fn read_for_resource(
        &self,
        user_id: UserId,
        resource_type: &str,
        resource_id: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<crate::metadata_v2::schemas::EventDocument>> {
        let resource_uuid = uuid::Uuid::parse_str(resource_id)?;
        
        // This requires scanning all events - in production, maintain an index
        let prefix = "events/";
        let keys = self.bucket_store.list_objects(user_id, prefix).await?;
        
        let mut events = Vec::new();
        
        for key in keys {
            if let Some(data) = self.bucket_store.get_object(user_id, &key).await? {
                if let Ok(event) = serde_json::from_slice::<crate::metadata_v2::schemas::EventDocument>(&data) {
                    if event.resource_type == resource_type && event.resource_id == resource_uuid {
                        events.push(event);
                        if events.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }
        
        // Sort by occurred_at descending
        events.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));
        
        Ok(events)
    }

    /// Read events by time range
    pub async fn read_range(
        &self,
        user_id: UserId,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: usize,
    ) -> anyhow::Result<Vec<crate::metadata_v2::schemas::EventDocument>> {
        let prefix = format!("events/{:04}/{:02}/", start.year(), start.month());
        let keys = self.bucket_store.list_objects(user_id, &prefix).await?;
        
        let mut events = Vec::new();
        
        for key in keys {
            if let Some(data) = self.bucket_store.get_object(user_id, &key).await? {
                if let Ok(event) = serde_json::from_slice::<crate::metadata_v2::schemas::EventDocument>(&data) {
                    if event.occurred_at >= start && event.occurred_at <= end {
                        events.push(event);
                        if events.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }
        
        // Sort by occurred_at
        events.sort_by(|a, b| a.occurred_at.cmp(&b.occurred_at));
        
        Ok(events)
    }
}

/// Unified storage system using per-user buckets
/// 
/// This is the main entry point for the RustFS-only architecture.
/// All durable data (metadata, blobs, events) is stored in per-user buckets.
pub struct UserBucketStorageSystem {
    bucket_store: Arc<dyn UserBucketStore>,
    blob_store: Arc<UserBucketBlobStore>,
    event_store: Arc<UserBucketEventStore>,
    store_factory: Arc<UserScopedStoreFactory>,
}

impl UserBucketStorageSystem {
    /// Create a new unified storage system
    pub fn new(bucket_store: Arc<dyn UserBucketStore>) -> Self {
        let blob_store = Arc::new(UserBucketBlobStore::new(bucket_store.clone()));
        let event_store = Arc::new(UserBucketEventStore::new(bucket_store.clone()));
        let store_factory = Arc::new(UserScopedStoreFactory::new(bucket_store.clone()));
        
        Self {
            bucket_store,
            blob_store,
            event_store,
            store_factory,
        }
    }

    /// Create an S3-based storage system
    pub async fn create_s3(
        endpoint: &str,
        region: &str,
        bucket_prefix: &str,
    ) -> anyhow::Result<Self> {
        use aws_config::BehaviorVersion;
        use aws_sdk_s3::Client as S3Client;

        let config = crate::user_bucket::UserBucketConfig {
            endpoint: endpoint.to_string(),
            region: region.to_string(),
            bucket_prefix: bucket_prefix.to_string(),
            base_prefix: "".to_string(),
        };

        let sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&config.endpoint)
            .region(aws_config::Region::new(config.region.clone()))
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
            .force_path_style(true)
            .build();

        let client = S3Client::from_conf(s3_config);
        let bucket_store: Arc<dyn UserBucketStore> = 
            Arc::new(crate::user_bucket::S3UserBucketStore::new(client, config));

        Ok(Self::new(bucket_store))
    }

    /// Get the store factory for creating user-scoped stores
    pub fn store_factory(&self) -> Arc<UserScopedStoreFactory> {
        self.store_factory.clone()
    }

    /// Get a scoped document store for a specific user
    pub fn doc_store_for(&self, user_id: UserId) -> Arc<UserScopedDocumentStore> {
        self.store_factory.for_user(user_id)
    }

    /// Get the blob store
    pub fn blob_store(&self) -> Arc<UserBucketBlobStore> {
        self.blob_store.clone()
    }

    /// Get the event store
    pub fn event_store(&self) -> Arc<UserBucketEventStore> {
        self.event_store.clone()
    }

    /// Get the underlying bucket store
    pub fn bucket_store(&self) -> Arc<dyn UserBucketStore> {
        self.bucket_store.clone()
    }

    /// Ensure a user's bucket exists
    pub async fn ensure_user_bucket(&self, user_id: UserId) -> anyhow::Result<()> {
        if !self.bucket_store.bucket_exists(user_id).await? {
            self.bucket_store.create_bucket(user_id).await?;
            tracing::info!(user_id = %user_id, "Created user bucket");
        }
        Ok(())
    }

    /// Check if a user has a bucket
    pub async fn user_has_bucket(&self, user_id: UserId) -> anyhow::Result<bool> {
        self.bucket_store.bucket_exists(user_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user_bucket::MemoryUserBucketStore;

    #[tokio::test]
    async fn test_user_scoped_document_store() {
        let bucket_store: Arc<dyn UserBucketStore> = Arc::new(MemoryUserBucketStore::new());
        let user_id = UserId::new_v4();
        
        let store = UserScopedDocumentStore::new(bucket_store, user_id);
        
        // Initially nothing exists
        assert!(store.get_raw("folders/test.json").await.unwrap().is_none());
        
        // Put a document
        let data = br#"{"id": "test", "name": "Test Folder"}"#;
        store.put_raw("folders/test.json", data, PutOptions::default()).await.unwrap();
        
        // Get it back
        let result = store.get_raw("folders/test.json").await.unwrap();
        assert!(result.is_some());
        let (bytes, metadata) = result.unwrap();
        assert_eq!(bytes, data.to_vec());
        assert_eq!(metadata.content_length, data.len() as u64);
        
        // List prefix
        let keys = store.list_prefix("folders/").await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], "folders/test.json");
        
        // Delete
        store.delete("folders/test.json").await.unwrap();
        assert!(store.get_raw("folders/test.json").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_user_bucket_blob_store() {
        let bucket_store: Arc<dyn UserBucketStore> = Arc::new(MemoryUserBucketStore::new());
        let user_id = UserId::new_v4();
        
        let blob_store = UserBucketBlobStore::new(bucket_store);
        
        // Store a blob
        let hash = "abc123";
        let data = Bytes::from("test blob content");
        blob_store.put(user_id, hash, data.clone()).await.unwrap();
        
        // Check existence
        assert!(blob_store.exists(user_id, hash).await.unwrap());
        assert!(!blob_store.exists(user_id, "nonexistent").await.unwrap());
        
        // Get it back
        let result = blob_store.get(user_id, hash).await.unwrap();
        assert_eq!(result, Some(data));
        
        // Delete
        blob_store.delete(user_id, hash).await.unwrap();
        assert!(!blob_store.exists(user_id, hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_storage_system() {
        let bucket_store: Arc<dyn UserBucketStore> = Arc::new(MemoryUserBucketStore::new());
        let system = UserBucketStorageSystem::new(bucket_store);
        
        let user_id = UserId::new_v4();
        
        // Ensure bucket exists
        system.ensure_user_bucket(user_id).await.unwrap();
        assert!(system.user_has_bucket(user_id).await.unwrap());
        
        // Get scoped store
        let doc_store = system.doc_store_for(user_id);
        
        // Store something
        doc_store.put_raw("test.json", b"{}", PutOptions::default()).await.unwrap();
        
        // Verify
        assert!(doc_store.get_raw("test.json").await.unwrap().is_some());
    }
}
