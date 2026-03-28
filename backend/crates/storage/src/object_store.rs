use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::{primitives::ByteStream, Client as S3Client};
use bytes::Bytes;

/// Object storage abstraction for RustFS/S3
pub struct ObjectStore {
    client: S3Client,
    bucket: String,
    public_endpoint: Option<S3Client>,
}

impl ObjectStore {
    /// Create new object store
    pub async fn new(endpoint: String, region: String, bucket: String) -> Result<Self> {
        use aws_credential_types::Credentials;
        
        // Check if there's a public endpoint for presigned URLs
        let public_endpoint = std::env::var("RUSTFS_PUBLIC_ENDPOINT").ok();

        // Use public endpoint for presigned URLs if configured
        let presign_endpoint = public_endpoint.clone().unwrap_or_else(|| endpoint.clone());

        // Load credentials from environment
        let access_key = std::env::var("AWS_ACCESS_KEY_ID")
            .map_err(|e| anyhow::anyhow!("AWS_ACCESS_KEY_ID not set: {}", e))?;
        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY")
            .map_err(|e| anyhow::anyhow!("AWS_SECRET_ACCESS_KEY not set: {}", e))?;
        
        let credentials = Credentials::new(
            access_key.clone(),
            secret_key.clone(),
            None,  // session token
            None,  // expiration
            "env",
        );

        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .region(aws_config::Region::new(region.clone()))
            .credentials_provider(credentials)
            .load()
            .await;

        // Use path-style addressing for MinIO compatibility
        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .force_path_style(true)
            .build();

        let client = S3Client::from_conf(s3_config);

        ensure_bucket_exists(&client, &bucket).await?;

        // Create a second client for presigned URLs with public endpoint
        let credentials2 = Credentials::new(
            access_key,
            secret_key,
            None,  // session token
            None,  // expiration
            "env",
        );
        
        let presign_config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&presign_endpoint)
            .region(aws_config::Region::new(region))
            .credentials_provider(credentials2)
            .load()
            .await;

        let presign_s3_config = aws_sdk_s3::config::Builder::from(&presign_config)
            .force_path_style(true)
            .build();

        let presign_client = S3Client::from_conf(presign_s3_config);

        Ok(Self {
            client,
            bucket,
            public_endpoint: Some(presign_client),
        })
    }

    /// Put object in storage
    pub async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(data))
            .send()
            .await?;

        Ok(())
    }

    /// Get object from storage
    pub async fn get(&self, key: &str) -> Result<Bytes> {
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        let data = output.body.collect().await?;
        Ok(data.into_bytes())
    }

    /// Delete object from storage
    pub async fn delete(&self, key: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        Ok(())
    }

    /// Check if object exists
    pub async fn exists(&self, key: &str) -> Result<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Generate a presigned URL for downloading an object
    pub async fn get_presigned_url(&self, key: &str, expires_in_secs: u64) -> Result<String> {
        use aws_sdk_s3::presigning::PresigningConfig;
        use std::time::Duration;

        let presigning_config = PresigningConfig::builder()
            .expires_in(Duration::from_secs(expires_in_secs))
            .build()?;

        // Use the presign client if available (with public endpoint)
        let client = self.public_endpoint.as_ref().unwrap_or(&self.client);

        let presigned_request = client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presigning_config)
            .await?;

        Ok(presigned_request.uri().to_string())
    }
}

async fn ensure_bucket_exists(client: &S3Client, bucket: &str) -> Result<()> {
    match client.head_bucket().bucket(bucket).send().await {
        Ok(_) => {
            tracing::info!(bucket = %bucket, "Object storage bucket is ready");
            Ok(())
        }
        Err(error) => {
            tracing::warn!(
                bucket = %bucket,
                error = %error,
                "Object storage bucket missing or inaccessible, attempting to create it"
            );

            match client
                .create_bucket()
                .bucket(bucket)
                .send()
                .await {
                Ok(_) => {
                    tracing::info!(bucket = %bucket, "Created object storage bucket");
                    Ok(())
                }
                Err(create_err) => {
                    // If bucket creation fails (e.g., signature error with RustFS),
                    // just warn and continue - the bucket might already exist
                    // or RustFS might auto-create buckets
                    tracing::warn!(
                        bucket = %bucket,
                        error = %create_err,
                        "Failed to create bucket, continuing anyway - operations may fail if bucket doesn't exist"
                    );
                    Ok(())
                }
            }
        }
    }
}
