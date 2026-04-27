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
        // Use public endpoint for presigned URLs if configured.
        let public_endpoint = std::env::var("RUSTFS_PUBLIC_ENDPOINT").ok();
        let presign_endpoint = presign_endpoint(&endpoint, public_endpoint);

        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .region(aws_config::Region::new(region.clone()))
            .load()
            .await;

        // Use path-style addressing for MinIO compatibility
        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .force_path_style(true)
            .build();

        let client = S3Client::from_conf(s3_config);

        ensure_bucket_exists(&client, &bucket).await?;

        // Create a second client for presigned URLs with public endpoint
        let region_str = config.region().map(|r| r.to_string()).unwrap_or(region);
        let presign_config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&presign_endpoint)
            .region(aws_config::Region::new(region_str))
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
        self.get_presigned_url_with_disposition(key, expires_in_secs, None).await
    }

    /// Generate a presigned URL with optional response-content-disposition
    pub async fn get_presigned_url_with_disposition(
        &self,
        key: &str,
        expires_in_secs: u64,
        content_disposition: Option<&str>,
    ) -> Result<String> {
        use aws_sdk_s3::presigning::PresigningConfig;
        use std::time::Duration;

        let presigning_config = PresigningConfig::builder()
            .expires_in(Duration::from_secs(expires_in_secs))
            .build()?;

        // Use the presign client if available (with public endpoint)
        let client = self.public_endpoint.as_ref().unwrap_or(&self.client);

        let mut req = client.get_object().bucket(&self.bucket).key(key);
        if let Some(cd) = content_disposition {
            req = req.response_content_disposition(cd);
        }
        let presigned_request = req.presigned(presigning_config).await?;

        Ok(presigned_request.uri().to_string())
    }
}

fn presign_endpoint(internal_endpoint: &str, public_endpoint: Option<String>) -> String {
    public_endpoint.unwrap_or_else(|| internal_endpoint.to_string())
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

            client
                .create_bucket()
                .bucket(bucket)
                .send()
                .await
                .with_context(|| format!("failed to create object storage bucket `{bucket}`"))?;

            tracing::info!(bucket = %bucket, "Created object storage bucket");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::presign_endpoint;

    #[test]
    fn presign_endpoint_uses_public_endpoint_when_configured() {
        let endpoint = presign_endpoint(
            "http://rustfs:9000",
            Some("https://files.example.com".to_string()),
        );

        assert_eq!(endpoint, "https://files.example.com");
    }

    #[test]
    fn presign_endpoint_falls_back_to_internal_endpoint() {
        let endpoint = presign_endpoint("http://rustfs:9000", None);

        assert_eq!(endpoint, "http://rustfs:9000");
    }
}
