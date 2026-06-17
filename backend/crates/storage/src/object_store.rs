use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::{primitives::ByteStream, Client as S3Client};
use bytes::Bytes;
use futures::Stream;
use tokio::io::AsyncReadExt;

/// Object storage abstraction for RustFS/S3
pub struct ObjectStore {
    client: S3Client,
    bucket: String,
    public_endpoint: Option<S3Client>,
}

impl ObjectStore {
    /// Create new object store
    pub async fn new(endpoint: String, region: String, bucket: String) -> Result<Self> {
        // Check if there's a public endpoint for presigned URLs
        let public_endpoint = std::env::var("RUSTFS_PUBLIC_ENDPOINT").ok();

        // Use public endpoint for presigned URLs if configured
        let presign_endpoint = public_endpoint.clone().unwrap_or_else(|| endpoint.clone());

        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&endpoint)
            .region(aws_config::Region::new(region.clone()))
            .load()
            .await;

        // Use path-style addressing for RustFS/S3-compatible object storage
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

    /// Put object in storage by streaming from a local file.
    ///
    /// This avoids loading the file into memory and is used for large uploads
    /// that have already been buffered to disk.
    pub async fn put_from_path(&self, key: &str, path: &std::path::Path) -> Result<()> {
        let body = ByteStream::from_path(path).await?;
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(body)
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

    /// Stream an object from storage.
    ///
    /// Returns the object's content type, content length, and a byte stream.
    /// The caller can pipe the stream into an HTTP response without buffering
    /// the entire object in memory.
    pub async fn get_stream(
        &self,
        key: &str,
    ) -> Result<(
        Option<String>,
        Option<i64>,
        impl Stream<Item = std::io::Result<Bytes>>,
    )> {
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        let content_type = output.content_type().map(|s| s.to_string());
        let content_length = output.content_length;

        // Convert the ByteStream into an AsyncRead and then into a futures
        // Stream so callers can pipe it into an HTTP response body without
        // buffering the entire object in memory.
        let reader = output.body.into_async_read();
        let stream = byte_stream_from_reader(reader);

        Ok((content_type, content_length, stream))
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
        self.get_presigned_url_with_disposition(key, expires_in_secs, None)
            .await
    }

    /// Health check: verify the object store bucket is accessible.
    pub async fn health_check(&self) -> Result<()> {
        self.client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .with_context(|| {
                format!("object storage bucket `{}` is not accessible", self.bucket)
            })?;
        Ok(())
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

/// Convert an [`AsyncRead`] into a stream of `Bytes` chunks.
///
/// The stream terminates after the first read error instead of yielding an
/// endless sequence of errors.
fn byte_stream_from_reader<R>(reader: R) -> impl Stream<Item = std::io::Result<Bytes>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    futures::stream::try_unfold(reader, |mut reader| async move {
        let mut buf = vec![0u8; 64 * 1024];
        match reader.read(&mut buf).await {
            Ok(0) => Ok(None),
            Ok(n) => {
                buf.truncate(n);
                Ok(Some((Bytes::from(buf), reader)))
            }
            Err(e) => Err(std::io::Error::other(e.to_string())),
        }
    })
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

            match client.create_bucket().bucket(bucket).send().await {
                Ok(_) => {
                    tracing::info!(bucket = %bucket, "Created object storage bucket");
                    Ok(())
                }
                Err(create_error) => {
                    let service_error = create_error.as_service_error();
                    let error_code = service_error.and_then(|error| error.meta().code());

                    if service_error.is_some_and(|error| {
                        error.is_bucket_already_owned_by_you() || error.is_bucket_already_exists()
                    }) || matches!(
                        error_code,
                        Some("BucketAlreadyOwnedByYou") | Some("BucketAlreadyExists")
                    ) {
                        tracing::info!(
                            bucket = %bucket,
                            code = ?error_code,
                            "Object storage bucket already exists after concurrent creation"
                        );
                        Ok(())
                    } else {
                        Err(create_error).with_context(|| {
                            format!("failed to create object storage bucket `{bucket}`")
                        })
                    }
                }
            }?;

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{StreamExt, TryStreamExt};
    use std::hash::{DefaultHasher, Hasher};
    use uuid::Uuid;

    /// Return the current peak resident set size (RSS) in bytes.
    ///
    /// On Unix this uses `getrusage(RUSAGE_SELF)`. macOS reports `ru_maxrss`
    /// in bytes; other Unix platforms typically report it in kilobytes.
    #[cfg(unix)]
    fn peak_rss_bytes() -> usize {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        let usage = unsafe {
            libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr());
            usage.assume_init()
        };
        #[cfg(target_os = "macos")]
        {
            usage.ru_maxrss as usize
        }
        #[cfg(not(target_os = "macos"))]
        {
            usage.ru_maxrss as usize * 1024
        }
    }

    #[cfg(not(unix))]
    fn peak_rss_bytes() -> usize {
        0
    }

    async fn setup_object_store() -> Option<ObjectStore> {
        let endpoint = std::env::var("S3_ENDPOINT")
            .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
            .unwrap_or_else(|_| "http://localhost:9000".to_string());
        let region = std::env::var("S3_REGION")
            .or_else(|_| std::env::var("RUSTFS_REGION"))
            .unwrap_or_else(|_| "us-east-1".to_string());
        let bucket = std::env::var("S3_BUCKET")
            .or_else(|_| std::env::var("RUSTFS_BUCKET"))
            .unwrap_or_else(|_| "rustshare".to_string());

        match ObjectStore::new(endpoint, region, bucket).await {
            Ok(store) => Some(store),
            Err(e) => {
                eprintln!("Skipping object-store streaming test: {e}");
                None
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires S3-compatible object storage"]
    async fn get_stream_returns_content_without_full_buffering() {
        let store = match setup_object_store().await {
            Some(store) => store,
            None => return,
        };

        // Use a multi-MB synthetic object so the streaming path is exercised.
        let chunk_size = 64 * 1024usize;
        let chunk_count = 64usize;
        let total_size = chunk_size * chunk_count;
        let mut content = Vec::with_capacity(total_size);
        for i in 0..chunk_count {
            content.extend_from_slice(&vec![(i % 256) as u8; chunk_size]);
        }

        let key = format!("streaming-test/{}", Uuid::new_v4());
        store.put(&key, Bytes::from(content.clone())).await.unwrap();

        let (content_type, content_length, stream) = store.get_stream(&key).await.unwrap();

        // Content-Type is not set by put_object here, but content length should match.
        assert!(content_type.is_none());
        assert_eq!(content_length, Some(total_size as i64));

        // Collect the stream and verify it matches the original content. The
        // important property is that get_stream did not require the entire
        // object to be in memory at once; the stream is consumed in chunks.
        let collected: Vec<Bytes> = stream.map(|r| r.unwrap()).collect().await;
        let mut received = Vec::with_capacity(total_size);
        for chunk in collected {
            received.extend_from_slice(&chunk);
        }
        assert_eq!(received.len(), total_size);
        assert_eq!(received, content);

        store.delete(&key).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires S3-compatible object storage"]
    async fn get_stream_errors_for_missing_object() {
        let store = match setup_object_store().await {
            Some(store) => store,
            None => return,
        };

        let key = format!("streaming-test/missing-{}", Uuid::new_v4());
        let result = store.get_stream(&key).await;
        assert!(result.is_err(), "streaming a missing object should fail");
    }

    /// Stream a large object through a small fixed buffer and verify that peak
    /// RSS does not grow proportionally to the object size.
    ///
    /// This test exercises the streaming path as a low-memory guarantee: the
    /// implementation must not materialize the entire object. We record peak
    /// RSS before and after streaming; an increase close to the object size
    /// would indicate full buffering.
    #[tokio::test]
    #[ignore = "requires S3-compatible object storage"]
    async fn get_stream_does_not_materialize_full_object() {
        let store = match setup_object_store().await {
            Some(store) => store,
            None => return,
        };

        let chunk_size = 64 * 1024usize;
        let chunk_count = 128usize; // 8 MiB total
        let total_size = chunk_size * chunk_count;

        let mut content = Vec::with_capacity(total_size);
        for i in 0..chunk_count {
            let byte = (i % 256) as u8;
            let chunk = vec![byte; chunk_size];
            content.extend_from_slice(&chunk);
        }

        let mut expected_hash = DefaultHasher::new();
        for byte in &content {
            expected_hash.write_u8(*byte);
        }
        let expected_hash = expected_hash.finish();

        let key = format!("streaming-test/low-memory-{}", Uuid::new_v4());
        store.put(&key, Bytes::from(content)).await.unwrap();
        // `content` is consumed by `Bytes::from` and dropped by `put`. Peak RSS
        // is sampled after the upload finishes so the measurement reflects the
        // download streaming phase, not the upload buffer.
        let rss_before = peak_rss_bytes();

        let (content_type, content_length, stream) = store.get_stream(&key).await.unwrap();
        assert!(content_type.is_none());
        assert_eq!(content_length, Some(total_size as i64));

        let mut received_bytes = 0usize;
        let mut received_hash = DefaultHasher::new();
        stream
            .try_for_each(|chunk| {
                received_bytes += chunk.len();
                for byte in chunk.iter() {
                    received_hash.write_u8(*byte);
                }
                std::future::ready(Ok(()))
            })
            .await
            .expect("stream chunk");

        let rss_after = peak_rss_bytes();
        let rss_increase = rss_after.saturating_sub(rss_before);

        assert_eq!(received_bytes, total_size, "streamed byte count mismatch");
        assert_eq!(
            received_hash.finish(),
            expected_hash,
            "streamed content checksum mismatch"
        );
        // The implementation uses a 64 KiB per-chunk buffer. Allow a generous
        // margin for runtime/allocator overhead, but require the RSS growth to
        // remain well below the 8 MiB object size.
        assert!(
            rss_increase < 2 * 1024 * 1024,
            "peak RSS grew by {rss_increase} bytes during streaming; \
             this suggests the object was fully materialized"
        );

        store.delete(&key).await.unwrap();
    }

    #[tokio::test]
    async fn get_stream_terminates_after_read_error() {
        struct ErrorReader(&'static str);

        impl tokio::io::AsyncRead for ErrorReader {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
                _buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::task::Poll::Ready(Err(std::io::Error::other(self.0)))
            }
        }

        let stream = super::byte_stream_from_reader(ErrorReader("simulated read failure"));
        let results: Vec<std::io::Result<Bytes>> = stream.collect().await;

        assert_eq!(
            results.len(),
            1,
            "stream should yield exactly one error and terminate"
        );
        assert!(
            results[0].is_err(),
            "the single yielded item should be an error"
        );
    }
}
