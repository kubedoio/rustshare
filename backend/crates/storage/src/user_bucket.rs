//! User Bucket Store Implementation
//!
//! Provides per-user isolated storage buckets for the RustShare V2 architecture.
//! Each user has their own S3-compatible bucket for canonical state.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use aws_sdk_s3::Client as S3Client;
use bytes::Bytes;
use uuid::Uuid;

/// User ID type
pub type UserId = Uuid;

/// Configuration for user bucket storage
#[derive(Debug, Clone)]
pub struct UserBucketConfig {
    /// S3 endpoint URL
    pub endpoint: String,
    /// S3 region
    pub region: String,
    /// Bucket name prefix
    pub bucket_prefix: String,
    /// Base path/prefix within buckets
    pub base_prefix: String,
}

impl Default for UserBucketConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9000".to_string(),
            region: "us-east-1".to_string(),
            bucket_prefix: "rustshare-user-".to_string(),
            base_prefix: "".to_string(),
        }
    }
}

impl UserBucketConfig {
    /// Create from environment variables
    pub fn from_env() -> Self {
        Self {
            endpoint: std::env::var("S3_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:9000".to_string()),
            region: std::env::var("S3_REGION")
                .unwrap_or_else(|_| "us-east-1".to_string()),
            bucket_prefix: std::env::var("USER_BUCKET_PREFIX")
                .unwrap_or_else(|_| "rustshare-user-".to_string()),
            base_prefix: std::env::var("USER_BUCKET_BASE_PREFIX")
                .unwrap_or_else(|_| "".to_string()),
        }
    }

    /// Get bucket name for a user
    pub fn bucket_for_user(&self, user_id: UserId) -> String {
        format!("{}{}", self.bucket_prefix, user_id)
    }
}

/// Store for per-user bucket operations
#[async_trait]
pub trait UserBucketStore: Send + Sync {
    /// Get the bucket name for a user
    fn bucket_for_user(&self, user_id: UserId) -> String;

    /// Check if user's bucket exists
    async fn bucket_exists(&self, user_id: UserId) -> Result<bool>;

    /// Create user's bucket
    async fn create_bucket(&self, user_id: UserId) -> Result<()>;

    /// Get object from user's bucket
    async fn get_object(&self, user_id: UserId, key: &str) -> Result<Option<Bytes>>;

    /// Put object to user's bucket
    async fn put_object(&self, user_id: UserId, key: &str, data: Bytes) -> Result<()>;

    /// Delete object from user's bucket
    async fn delete_object(&self, user_id: UserId, key: &str) -> Result<()>;

    /// List objects with prefix in user's bucket
    async fn list_objects(&self, user_id: UserId, prefix: &str) -> Result<Vec<String>>;

    /// Check if object exists
    async fn object_exists(&self, user_id: UserId, key: &str) -> Result<bool>;
}

/// S3-based user bucket store
pub struct S3UserBucketStore {
    client: S3Client,
    config: UserBucketConfig,
}

impl S3UserBucketStore {
    /// Create a new S3 user bucket store
    pub fn new(client: S3Client, config: UserBucketConfig) -> Self {
        Self { client, config }
    }

    /// Build full key with base prefix
    fn build_key(&self, key: &str) -> String {
        if self.config.base_prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}/{}", self.config.base_prefix, key)
        }
    }
}

#[async_trait]
impl UserBucketStore for S3UserBucketStore {
    fn bucket_for_user(&self, user_id: UserId) -> String {
        self.config.bucket_for_user(user_id)
    }

    async fn bucket_exists(&self, user_id: UserId) -> Result<bool> {
        let bucket = self.bucket_for_user(user_id);

        match self.client.head_bucket().bucket(&bucket).send().await {
            Ok(_) => Ok(true),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("NotFound") || err_str.contains("NoSuchBucket") {
                    Ok(false)
                } else {
                    Err(e.into())
                }
            }
        }
    }

    async fn create_bucket(&self, user_id: UserId) -> Result<()> {
        let bucket = self.bucket_for_user(user_id);

        // Check if already exists
        if self.bucket_exists(user_id).await? {
            return Ok(());
        }

        // Create bucket
        self.client
            .create_bucket()
            .bucket(&bucket)
            .send()
            .await?;

        tracing::info!(bucket = %bucket, "Created user bucket");
        Ok(())
    }

    async fn get_object(&self, user_id: UserId, key: &str) -> Result<Option<Bytes>> {
        let bucket = self.bucket_for_user(user_id);
        let key = self.build_key(key);

        match self
            .client
            .get_object()
            .bucket(&bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(output) => {
                let data = output.body.collect().await?;
                Ok(Some(data.into_bytes()))
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("NoSuchKey") || err_str.contains("NotFound") {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }

    async fn put_object(&self, user_id: UserId, key: &str, data: Bytes) -> Result<()> {
        let bucket = self.bucket_for_user(user_id);
        let key = self.build_key(key);

        self.client
            .put_object()
            .bucket(&bucket)
            .key(&key)
            .body(data.into())
            .send()
            .await?;

        Ok(())
    }

    async fn delete_object(&self, user_id: UserId, key: &str) -> Result<()> {
        let bucket = self.bucket_for_user(user_id);
        let key = self.build_key(key);

        self.client
            .delete_object()
            .bucket(&bucket)
            .key(&key)
            .send()
            .await?;

        Ok(())
    }

    async fn list_objects(&self, user_id: UserId, prefix: &str) -> Result<Vec<String>> {
        let bucket = self.bucket_for_user(user_id);
        let prefix = self.build_key(prefix);

        let mut keys = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&bucket)
                .prefix(&prefix);

            if let Some(token) = &continuation_token {
                request = request.continuation_token(token);
            }

            let output = request.send().await?;

            if let Some(contents) = output.contents {
                for obj in contents {
                    if let Some(key) = obj.key {
                        // Remove base prefix from returned keys
                        let relative_key = if self.config.base_prefix.is_empty() {
                            key
                        } else {
                            key.strip_prefix(&format!("{}/", self.config.base_prefix))
                                .unwrap_or(&key)
                                .to_string()
                        };
                        keys.push(relative_key);
                    }
                }
            }

            if output.is_truncated.unwrap_or(false) {
                continuation_token = output.next_continuation_token.map(|s| s.to_string());
            } else {
                break;
            }
        }

        Ok(keys)
    }

    async fn object_exists(&self, user_id: UserId, key: &str) -> Result<bool> {
        let bucket = self.bucket_for_user(user_id);
        let key = self.build_key(key);

        match self
            .client
            .head_object()
            .bucket(&bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("NoSuchKey") || err_str.contains("NotFound") {
                    Ok(false)
                } else {
                    Err(e.into())
                }
            }
        }
    }
}

/// In-memory user bucket store for testing
pub struct MemoryUserBucketStore {
    buckets: std::sync::Mutex<std::collections::HashMap<UserId, std::collections::HashMap<String, Bytes>>>,
    config: UserBucketConfig,
}

impl MemoryUserBucketStore {
    /// Create a new memory-based user bucket store
    pub fn new() -> Self {
        Self {
            buckets: std::sync::Mutex::new(std::collections::HashMap::new()),
            config: UserBucketConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: UserBucketConfig) -> Self {
        Self {
            buckets: std::sync::Mutex::new(std::collections::HashMap::new()),
            config,
        }
    }
}

impl Default for MemoryUserBucketStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UserBucketStore for MemoryUserBucketStore {
    fn bucket_for_user(&self, user_id: UserId) -> String {
        self.config.bucket_for_user(user_id)
    }

    async fn bucket_exists(&self, user_id: UserId) -> Result<bool> {
        let buckets = self.buckets.lock().unwrap();
        Ok(buckets.contains_key(&user_id))
    }

    async fn create_bucket(&self, user_id: UserId) -> Result<()> {
        let mut buckets = self.buckets.lock().unwrap();
        buckets.entry(user_id).or_insert_with(std::collections::HashMap::new);
        Ok(())
    }

    async fn get_object(&self, user_id: UserId, key: &str) -> Result<Option<Bytes>> {
        let buckets = self.buckets.lock().unwrap();
        Ok(buckets
            .get(&user_id)
            .and_then(|bucket| bucket.get(key).cloned()))
    }

    async fn put_object(&self, user_id: UserId, key: &str, data: Bytes) -> Result<()> {
        let mut buckets = self.buckets.lock().unwrap();
        buckets
            .entry(user_id)
            .or_insert_with(std::collections::HashMap::new)
            .insert(key.to_string(), data);
        Ok(())
    }

    async fn delete_object(&self, user_id: UserId, key: &str) -> Result<()> {
        let mut buckets = self.buckets.lock().unwrap();
        if let Some(bucket) = buckets.get_mut(&user_id) {
            bucket.remove(key);
        }
        Ok(())
    }

    async fn list_objects(&self, user_id: UserId, prefix: &str) -> Result<Vec<String>> {
        let buckets = self.buckets.lock().unwrap();
        Ok(buckets
            .get(&user_id)
            .map(|bucket| {
                bucket
                    .keys()
                    .filter(|k| k.starts_with(prefix))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn object_exists(&self, user_id: UserId, key: &str) -> Result<bool> {
        let buckets = self.buckets.lock().unwrap();
        Ok(buckets
            .get(&user_id)
            .map(|bucket| bucket.contains_key(key))
            .unwrap_or(false))
    }
}

/// Factory for creating user bucket stores
pub struct UserBucketStoreFactory;

impl UserBucketStoreFactory {
    /// Create an S3-based store from environment configuration
    pub async fn create_s3(config: UserBucketConfig) -> Result<Arc<dyn UserBucketStore>> {
        Self::create_s3_with_config(config).await
    }
    
    /// Create an S3-based store with explicit configuration
    pub async fn create_s3_with_config(config: UserBucketConfig) -> Result<Arc<dyn UserBucketStore>> {
        use aws_config::BehaviorVersion;
        use aws_credential_types::Credentials;
        
        // Load credentials from environment
        let access_key = std::env::var("AWS_ACCESS_KEY_ID")
            .map_err(|e| anyhow::anyhow!("AWS_ACCESS_KEY_ID not set: {}", e))?;
        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY")
            .map_err(|e| anyhow::anyhow!("AWS_SECRET_ACCESS_KEY not set: {}", e))?;
        
        let credentials = Credentials::new(
            access_key,
            secret_key,
            None,  // session token
            None,  // expiration
            "env",
        );
        
        let sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&config.endpoint)
            .region(aws_config::Region::new(config.region.clone()))
            .credentials_provider(credentials)
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
            .force_path_style(true)
            .build();

        let client = S3Client::from_conf(s3_config);

        Ok(Arc::new(S3UserBucketStore::new(client, config)))
    }

    /// Create a memory-based store for testing
    pub fn create_memory() -> Arc<dyn UserBucketStore> {
        Arc::new(MemoryUserBucketStore::new())
    }

    /// Create a memory-based store with custom config
    pub fn create_memory_with_config(config: UserBucketConfig) -> Arc<dyn UserBucketStore> {
        Arc::new(MemoryUserBucketStore::with_config(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_bucket_store() {
        let store = MemoryUserBucketStore::new();
        let user_id = Uuid::new_v4();

        // Bucket doesn't exist initially
        assert!(!store.bucket_exists(user_id).await.unwrap());

        // Create bucket
        store.create_bucket(user_id).await.unwrap();
        assert!(store.bucket_exists(user_id).await.unwrap());

        // Put object
        store
            .put_object(user_id, "test/key.json", Bytes::from("data"))
            .await
            .unwrap();

        // Get object
        let data = store.get_object(user_id, "test/key.json").await.unwrap();
        assert_eq!(data, Some(Bytes::from("data")));

        // List objects
        let keys = store.list_objects(user_id, "test/").await.unwrap();
        assert_eq!(keys, vec!["test/key.json"]);

        // Object exists
        assert!(store.object_exists(user_id, "test/key.json").await.unwrap());
        assert!(!store.object_exists(user_id, "nonexistent").await.unwrap());

        // Delete object
        store.delete_object(user_id, "test/key.json").await.unwrap();
        assert!(!store.object_exists(user_id, "test/key.json").await.unwrap());
    }

    #[test]
    fn test_bucket_name_generation() {
        let config = UserBucketConfig::default();
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();

        assert_eq!(
            config.bucket_for_user(user_id),
            "rustshare-user-550e8400-e29b-41d4-a716-446655440000"
        );
    }
}
