//! Hybrid Storage System
//!
//! This module provides a hybrid storage architecture:
//! - System bucket: Stores shared data like user accounts, groups, system config
//! - Per-user buckets: Store user-owned data like files, folders, shares, events
//!
//! This approach allows:
//! 1. System data to be accessible without knowing which user owns it
//! 2. User data to be isolated in per-user buckets for backup/restore
//! 3. Clean separation between system and user resources

use std::sync::Arc;

use aws_sdk_s3::Client as S3Client;

use crate::metadata_v2::{
    MetadataDocumentStore, ObjectMetadata, PutOptions, PutResult,
};
use crate::user_bucket::{UserBucketConfig, UserBucketStore, UserId};

/// System-level document store using a shared bucket
/// 
/// This is used for data that needs to be accessible globally,
/// such as user accounts, groups, and system configuration.
pub struct SystemDocumentStore {
    client: S3Client,
    bucket: String,
    base_prefix: String,
    namespace: String,
}

impl SystemDocumentStore {
    /// Create a new system document store
    pub fn new(
        client: S3Client,
        bucket: String,
        base_prefix: String,
        namespace: String,
    ) -> Self {
        Self {
            client,
            bucket,
            base_prefix,
            namespace,
        }
    }

    /// Build the full S3 key for a document
    fn build_key(&self, key: &str) -> String {
        format!("{}/{}/meta/{}", self.base_prefix, self.namespace, key)
    }

    /// Ensure the system bucket exists
    pub async fn ensure_bucket(&self) -> anyhow::Result<()> {
        match self.client.head_bucket().bucket(&self.bucket).send().await {
            Ok(_) => {
                tracing::info!(bucket = %self.bucket, "System bucket already exists");
                Ok(())
            }
            Err(head_err) => {
                tracing::info!(
                    bucket = %self.bucket,
                    error = %head_err,
                    "System bucket head check failed, attempting to create"
                );
                // Try to create the bucket
                match self.client.create_bucket().bucket(&self.bucket).send().await {
                    Ok(_) => {
                        tracing::info!(bucket = %self.bucket, "Created system bucket");
                        Ok(())
                    }
                    Err(create_err) => {
                        let err_msg = format!(
                            "Failed to create system bucket '{}': head_error={}, create_error={}",
                            self.bucket, head_err, create_err
                        );
                        tracing::error!("{}", err_msg);
                        Err(anyhow::anyhow!(err_msg))
                    }
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl MetadataDocumentStore for SystemDocumentStore {
    async fn get_raw(&self, key: &str) -> anyhow::Result<Option<(Vec<u8>, ObjectMetadata)>> {
        let object_key = self.build_key(key);
        
        match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await
        {
            Ok(output) => {
                let etag = output.e_tag().unwrap_or("").to_string();
                let last_modified = output
                    .last_modified()
                    .and_then(|dt| {
                        let secs = dt.secs();
                        let nanos = dt.subsec_nanos();
                        chrono::DateTime::from_timestamp(secs, nanos)
                    })
                    .unwrap_or_else(chrono::Utc::now);
                let content_length = output.content_length.unwrap_or(0) as u64;
                
                let data = output.body.collect().await?;
                let bytes = data.into_bytes();
                
                Ok(Some((bytes.to_vec(), ObjectMetadata {
                    etag,
                    last_modified,
                    content_length,
                    version_id: None,
                })))
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("NoSuchKey") || err_str.contains("NotFound") {
                    Ok(None)
                } else {
                    tracing::error!(
                        bucket = %self.bucket,
                        key = %object_key,
                        error = %e,
                        "S3 get_object failed"
                    );
                    Err(anyhow::anyhow!("S3 error accessing bucket '{}', key '{}': {}", self.bucket, object_key, e))
                }
            }
        }
    }

    async fn head(&self, key: &str) -> anyhow::Result<Option<ObjectMetadata>> {
        let object_key = self.build_key(key);
        
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await
        {
            Ok(output) => {
                let etag = output.e_tag().unwrap_or("").to_string();
                let last_modified = output
                    .last_modified()
                    .and_then(|dt| {
                        let secs = dt.secs();
                        let nanos = dt.subsec_nanos();
                        chrono::DateTime::from_timestamp(secs, nanos)
                    })
                    .unwrap_or_else(chrono::Utc::now);
                let content_length = output.content_length.unwrap_or(0) as u64;
                
                Ok(Some(ObjectMetadata {
                    etag,
                    last_modified,
                    content_length,
                    version_id: None,
                }))
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

    async fn put_raw(
        &self,
        key: &str,
        data: &[u8],
        opts: PutOptions,
    ) -> anyhow::Result<PutResult> {
        let object_key = self.build_key(key);
        
        let mut request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .body(data.to_vec().into());
        
        // Add conditional headers if provided
        if let Some(etag) = opts.if_match {
            request = request.if_match(etag);
        }
        if let Some(etag) = opts.if_none_match {
            request = request.if_none_match(etag);
        }
        if let Some(content_type) = opts.content_type {
            request = request.content_type(content_type);
        } else {
            request = request.content_type("application/json");
        }
        
        let output = request.send().await?;
        
        Ok(PutResult {
            etag: output.e_tag().unwrap_or("").to_string(),
            version_id: None,
        })
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let object_key = self.build_key(key);
        
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await?;
        
        Ok(())
    }

    async fn list_prefix(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        let object_prefix = self.build_key(prefix);
        
        let mut keys = Vec::new();
        let mut continuation_token: Option<String> = None;
        
        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&object_prefix);
            
            if let Some(token) = &continuation_token {
                request = request.continuation_token(token);
            }
            
            let output = request.send().await?;
            
            if let Some(contents) = output.contents {
                for obj in contents {
                    if let Some(key) = obj.key {
                        // Strip the base prefix
                        let relative_key = key
                            .strip_prefix(&format!("{}/{}/meta/", self.base_prefix, self.namespace))
                            .unwrap_or(&key)
                            .to_string();
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
}

/// Hybrid storage system that uses:
/// - System bucket for shared data (users, groups, config)
/// - Per-user buckets for user-owned data (files, folders, shares)
pub struct HybridStorageSystem {
    system_store: Arc<SystemDocumentStore>,
    user_bucket_store: Arc<dyn UserBucketStore>,
}

impl HybridStorageSystem {
    /// Create a new hybrid storage system
    pub fn new(
        system_store: Arc<SystemDocumentStore>,
        user_bucket_store: Arc<dyn UserBucketStore>,
    ) -> Self {
        Self {
            system_store,
            user_bucket_store,
        }
    }

    /// Get the system document store
    pub fn system_store(&self) -> Arc<SystemDocumentStore> {
        self.system_store.clone()
    }

    /// Get the user bucket store
    pub fn user_bucket_store(&self) -> Arc<dyn UserBucketStore> {
        self.user_bucket_store.clone()
    }

    /// Ensure system bucket exists
    pub async fn ensure_system_bucket(&self) -> anyhow::Result<()> {
        self.system_store.ensure_bucket().await
    }

    /// Ensure a user bucket exists
    pub async fn ensure_user_bucket(&self, user_id: UserId) -> anyhow::Result<()> {
        if !self.user_bucket_store.bucket_exists(user_id).await? {
            self.user_bucket_store.create_bucket(user_id).await?;
            tracing::info!(user_id = %user_id, "Created user bucket");
        }
        Ok(())
    }
}

/// Factory for creating hybrid storage systems
pub struct HybridStorageFactory;

impl HybridStorageFactory {
    /// Create a hybrid storage system from environment configuration
    pub async fn from_env() -> anyhow::Result<HybridStorageSystem> {
        use aws_config::BehaviorVersion;
        use aws_credential_types::Credentials;

        let endpoint = std::env::var("RUSTFS_ENDPOINT")?;
        let region = std::env::var("RUSTFS_REGION")?;
        let system_bucket = std::env::var("RUSTFS_BUCKET")?;
        let bucket_prefix = std::env::var("USER_BUCKET_PREFIX")
            .unwrap_or_else(|_| "rustshare-user-".to_string());
        let base_prefix = std::env::var("RUSTSHARE_METADATA_PREFIX")
            .unwrap_or_else(|_| "apps/rustshare".to_string());
        let namespace = std::env::var("RUSTSHARE_METADATA_NAMESPACE")
            .unwrap_or_else(|_| "default".to_string());

        // Load credentials from environment
        let access_key = std::env::var("AWS_ACCESS_KEY_ID")
            .map_err(|e| anyhow::anyhow!("AWS_ACCESS_KEY_ID not set: {}", e))?;
        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY")
            .map_err(|e| anyhow::anyhow!("AWS_SECRET_ACCESS_KEY not set: {}", e))?;
        
        tracing::info!("Using S3 credentials - access_key_id starts with: {}", &access_key[..4.min(access_key.len())]);
        
        let credentials = Credentials::new(
            access_key,
            secret_key,
            None,  // session token
            None,  // expiration
            "env",
        );

        // Create S3 client with explicit credentials
        let sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .region(aws_config::Region::new(region.clone()))
            .credentials_provider(credentials)
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
            .force_path_style(true)
            .build();

        let client = S3Client::from_conf(s3_config.clone());

        // Create system document store
        let system_store = Arc::new(SystemDocumentStore::new(
            client.clone(),
            system_bucket.clone(),
            base_prefix.clone(),
            namespace.clone(),
        ));

        // Ensure system bucket exists with retries
        tracing::info!("Ensuring system bucket exists: {}", system_bucket);
        let mut last_error = None;
        for attempt in 1..=5 {
            match system_store.ensure_bucket().await {
                Ok(_) => {
                    tracing::info!("System bucket ready: {}", system_bucket);
                    last_error = None;
                    break;
                }
                Err(e) => {
                    tracing::warn!(
                        "Attempt {}/5: Failed to ensure system bucket {}: {:?}",
                        attempt, system_bucket, e
                    );
                    last_error = Some(e);
                    if attempt < 5 {
                        let delay = std::time::Duration::from_secs(2_u64.pow(attempt - 1));
                        tracing::info!("Waiting {:?} before retry...", delay);
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
        
        if let Some(e) = last_error {
            tracing::error!("Failed to ensure system bucket {} after 5 attempts: {:?}", system_bucket, e);
            return Err(anyhow::anyhow!("Failed to initialize system bucket '{}': {}", system_bucket, e));
        }

        // Create user bucket store - need new credentials instance for the second client
        let credentials2 = Credentials::new(
            std::env::var("AWS_ACCESS_KEY_ID")?,
            std::env::var("AWS_SECRET_ACCESS_KEY")?,
            None,
            None,
            "env",
        );
        let sdk_config2 = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .region(aws_config::Region::new(region.clone()))
            .credentials_provider(credentials2)
            .load()
            .await;
        let s3_config2 = aws_sdk_s3::config::Builder::from(&sdk_config2)
            .force_path_style(true)
            .build();

        let user_config = UserBucketConfig {
            endpoint: endpoint.clone(),
            region: region.clone(),
            bucket_prefix: bucket_prefix.clone(),
            base_prefix: "".to_string(),
        };

        let user_client = S3Client::from_conf(s3_config2);
        let user_bucket_store: Arc<dyn UserBucketStore> = 
            Arc::new(crate::user_bucket::S3UserBucketStore::new(user_client, user_config));

        Ok(HybridStorageSystem::new(system_store, user_bucket_store))
    }
}
