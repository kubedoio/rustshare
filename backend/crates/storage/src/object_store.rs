use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::{primitives::ByteStream, Client as S3Client};
use bytes::Bytes;
use futures::{Stream, StreamExt};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::pin::Pin;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const BLOB_KEY_PREFIX: &str = "blobs/";
const SHA256_HEX_LEN: usize = 64;
const STREAM_BUFFER_SIZE: usize = 64 * 1024;
const OBJECT_STORE_AUTO_CREATE_BUCKET_ENV: &str = "RUSTSHARE_OBJECT_STORE_AUTO_CREATE_BUCKET";

/// Object storage abstraction for RustFS/S3
pub struct ObjectStore {
    client: S3Client,
    bucket: String,
    public_endpoint: Option<S3Client>,
    blob_lock_pool: Option<PgPool>,
}

/// Object store startup options.
#[derive(Debug, Clone, Copy, Default)]
pub struct ObjectStoreOptions {
    /// Create the bucket on startup when it is missing.
    ///
    /// Production deployments should provision buckets outside the application
    /// and leave this disabled.
    pub auto_create_bucket: bool,
}

impl ObjectStore {
    /// Create new object store
    pub async fn new(endpoint: String, region: String, bucket: String) -> Result<Self> {
        Self::new_with_options(endpoint, region, bucket, ObjectStoreOptions::default()).await
    }

    /// Create new object store with explicit startup options.
    pub async fn new_with_options(
        endpoint: String,
        region: String,
        bucket: String,
        options: ObjectStoreOptions,
    ) -> Result<Self> {
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

        ensure_bucket_exists(&client, &bucket, options.auto_create_bucket).await?;

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
            blob_lock_pool: None,
        })
    }

    /// Attach the database pool used for cross-process blob writer/GC locks.
    pub fn with_blob_lock_pool(mut self, pool: PgPool) -> Self {
        self.blob_lock_pool = Some(pool);
        self
    }

    /// Hold the shared per-key lock until the returned transaction is dropped.
    pub async fn acquire_blob_lock(
        &self,
        key: &str,
    ) -> Result<Option<Transaction<'static, Postgres>>> {
        if expected_blob_sha256(key).is_none() {
            return Ok(None);
        }
        let pool = self
            .blob_lock_pool
            .as_ref()
            .context("content-addressed blob locking is not configured")?;
        let mut transaction = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 7277856))")
            .bind(key)
            .execute(&mut *transaction)
            .await?;
        Ok(Some(transaction))
    }

    /// Put object in storage
    pub async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        verify_blob_bytes(key, &data)?;
        self.put_body(key, ByteStream::from(data), false).await
    }

    /// Put object only if the key does not already exist.
    pub async fn put_if_absent(&self, key: &str, data: Bytes) -> Result<()> {
        verify_blob_bytes(key, &data)?;
        self.put_body(key, ByteStream::from(data), true).await
    }

    /// Put object in storage by streaming from a local file.
    ///
    /// This avoids loading the file into memory and is used for large uploads
    /// that have already been buffered to disk.
    pub async fn put_from_path(&self, key: &str, path: &std::path::Path) -> Result<()> {
        let body = verified_byte_stream_from_path(key, path).await?;
        self.put_body(key, body, false).await
    }

    /// Put object from a local file only if the key does not already exist.
    pub async fn put_from_path_if_absent(&self, key: &str, path: &std::path::Path) -> Result<()> {
        let body = verified_byte_stream_from_path(key, path).await?;
        self.put_body(key, body, true).await
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
        let bytes = data.into_bytes();
        verify_blob_bytes(key, &bytes)?;
        Ok(bytes)
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
        let mut content_length = output.content_length;

        // Convert the ByteStream into an AsyncRead and then into a futures
        // Stream so callers can pipe it into an HTTP response body without
        // buffering the entire object in memory.
        let reader = output.body.into_async_read();
        let stream = verify_blob_stream(key, byte_stream_from_reader(reader));
        if expected_blob_sha256(key).is_some() {
            // Blob streams can only prove integrity after EOF. Avoid a fixed
            // length response so callers can surface a final stream error
            // instead of advertising a complete byte count up front.
            content_length = None;
        }

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
            Err(error) => {
                let code = error
                    .as_service_error()
                    .and_then(|service_error| service_error.meta().code());
                if matches!(code, Some("NoSuchKey") | Some("NotFound")) {
                    Ok(false)
                } else {
                    Err(error.into())
                }
            }
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

    async fn put_body(&self, key: &str, body: ByteStream, if_absent: bool) -> Result<()> {
        let mut request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(body);

        if if_absent {
            request = request.if_none_match("*");
        }

        request.send().await?;
        Ok(())
    }
}

/// Whether an error returned by [`ObjectStore::get`] or
/// [`ObjectStore::get_stream`] means the object is confirmed missing (S3
/// `NoSuchKey` / 404), as opposed to an infrastructure failure such as an
/// unreachable store, access denied, or a failed integrity check.
///
/// Only a confirmed-missing error may be treated as "not found"; anything
/// else must be surfaced as a storage failure. Mirrors the code matching in
/// [`ObjectStore::exists`].
pub fn is_missing_object_error(error: &anyhow::Error) -> bool {
    let Some(sdk_error) = error.downcast_ref::<SdkError<GetObjectError>>() else {
        return false;
    };
    sdk_error
        .as_service_error()
        .is_some_and(is_missing_get_object_error)
}

fn is_missing_get_object_error(error: &GetObjectError) -> bool {
    error.is_no_such_key() || matches!(error.meta().code(), Some("NoSuchKey") | Some("NotFound"))
}

fn expected_blob_sha256(key: &str) -> Option<&str> {
    let hash = key.strip_prefix(BLOB_KEY_PREFIX)?;
    (hash.len() == SHA256_HEX_LEN && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hash)
}

fn calculate_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn verify_blob_bytes(key: &str, data: &[u8]) -> Result<()> {
    if let Some(expected) = expected_blob_sha256(key) {
        let actual = calculate_sha256(data);
        anyhow::ensure!(
            actual.eq_ignore_ascii_case(expected),
            "object integrity check failed for `{key}`: expected sha256 {expected}, got {actual}"
        );
    }

    Ok(())
}

async fn verified_byte_stream_from_path(key: &str, path: &std::path::Path) -> Result<ByteStream> {
    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("failed to open object source file `{}`", path.display()))?;

    let Some(expected) = expected_blob_sha256(key) else {
        return ByteStream::read_from()
            .file(file)
            .buffer_size(STREAM_BUFFER_SIZE)
            .build()
            .await
            .with_context(|| {
                format!(
                    "failed to build object source stream from `{}`",
                    path.display()
                )
            });
    };

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];
    let mut size = 0_u64;
    let temp_file = tempfile::tempfile()
        .with_context(|| "failed to create verified object source temp file")?;
    let mut verified_file = tokio::fs::File::from_std(temp_file);

    loop {
        let read = file
            .read(&mut buffer)
            .await
            .with_context(|| format!("failed to read object source file `{}`", path.display()))?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
        verified_file
            .write_all(&buffer[..read])
            .await
            .with_context(|| "failed to write verified object source temp file")?;
    }

    let actual = hex::encode(hasher.finalize());
    anyhow::ensure!(
        actual.eq_ignore_ascii_case(expected),
        "object integrity check failed for `{key}`: expected sha256 {expected}, got {actual}"
    );

    verified_file
        .flush()
        .await
        .with_context(|| "failed to flush verified object source temp file")?;
    verified_file
        .seek(std::io::SeekFrom::Start(0))
        .await
        .with_context(|| "failed to rewind verified object source temp file")?;

    ByteStream::read_from()
        .file(verified_file)
        .length(aws_smithy_types::byte_stream::Length::Exact(size))
        .buffer_size(STREAM_BUFFER_SIZE)
        .build()
        .await
        .with_context(|| {
            format!(
                "failed to build object source stream from `{}`",
                path.display()
            )
        })
}

fn verify_blob_stream<S>(key: &str, stream: S) -> impl Stream<Item = std::io::Result<Bytes>>
where
    S: Stream<Item = std::io::Result<Bytes>> + Send + 'static,
{
    let Some(expected) = expected_blob_sha256(key).map(str::to_ascii_lowercase) else {
        return futures::future::Either::Left(stream);
    };

    struct VerifyState<S> {
        stream: Pin<Box<S>>,
        hasher: Sha256,
        expected: String,
    }

    let state = VerifyState {
        stream: Box::pin(stream),
        hasher: Sha256::new(),
        expected,
    };

    let stream = futures::stream::try_unfold(state, |mut state| async move {
        match state.stream.next().await {
            Some(Ok(chunk)) => {
                state.hasher.update(&chunk);
                Ok(Some((chunk, state)))
            }
            Some(Err(error)) => Err(error),
            None => {
                let actual = hex::encode(state.hasher.finalize());
                if actual == state.expected {
                    Ok(None)
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "object integrity check failed: expected sha256 {}, got {}",
                            state.expected, actual
                        ),
                    ))
                }
            }
        }
    });

    futures::future::Either::Right(stream)
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
        let mut buf = vec![0u8; STREAM_BUFFER_SIZE];
        match reader.read(&mut buf).await {
            Ok(0) => Ok(None),
            Ok(n) => {
                buf.truncate(n);
                Ok(Some((Bytes::from(buf), reader)))
            }
            Err(e) => Err(e),
        }
    })
}

/// Classify a `head_bucket` failure. Only a confirmed "bucket does not exist"
/// (S3 `NotFound`/`NoSuchBucket`, or a bare HTTP 404 from S3-compatible
/// stores like RustFS/MinIO) means the bucket is missing. Everything else —
/// unreachable endpoint, 403 Forbidden from bad credentials, 5xx — means
/// "inaccessible", and must not fall into the bucket-creation path.
fn head_bucket_error_is_not_found(error_code: Option<&str>, http_status: Option<u16>) -> bool {
    matches!(
        error_code,
        Some("NotFound") | Some("NoSuchBucket") | Some("404")
    ) || http_status == Some(404)
}

async fn ensure_bucket_exists(client: &S3Client, bucket: &str, auto_create: bool) -> Result<()> {
    match client.head_bucket().bucket(bucket).send().await {
        Ok(_) => {
            tracing::info!(bucket = %bucket, "Object storage bucket is ready");
            Ok(())
        }
        Err(error) => {
            let error_code = error
                .as_service_error()
                .and_then(|service_error| service_error.meta().code());
            let http_status = error
                .raw_response()
                .map(|response| response.status().as_u16());

            if !head_bucket_error_is_not_found(error_code, http_status) {
                // Anything but a confirmed "bucket does not exist" means the
                // endpoint is unreachable or the credentials are rejected;
                // attempting to create the bucket would only mask the cause.
                return Err(error).with_context(|| {
                    format!(
                        "object storage bucket `{bucket}` could not be checked; \
                         verify the object storage endpoint (RUSTFS_ENDPOINT) is reachable \
                         and the credentials (AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY) are valid"
                    )
                });
            }

            if !auto_create {
                return Err(error).with_context(|| {
                    format!(
                        "object storage bucket `{bucket}` is missing or inaccessible; \
                         provision it before startup or set {OBJECT_STORE_AUTO_CREATE_BUCKET_ENV}=true"
                    )
                });
            }

            tracing::warn!(
                bucket = %bucket,
                error = %error,
                "Object storage bucket missing, attempting to create it"
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

// Service-layer object-store trait bridge.
// This lives next to the concrete type so the storage crate root stays small.
#[allow(async_fn_in_trait)]
impl rustshare_core::services::ObjectStoreOps for ObjectStore {
    async fn acquire_blob_write_lock(&self, key: &str) -> anyhow::Result<Box<dyn Send>> {
        Ok(Box::new(self.acquire_blob_lock(key).await?))
    }

    async fn put(&self, key: &str, data: bytes::Bytes) -> anyhow::Result<()> {
        self.put(key, data).await
    }

    async fn put_from_path(&self, key: &str, path: &std::path::Path) -> anyhow::Result<()> {
        self.put_from_path(key, path).await
    }

    async fn exists(&self, key: &str) -> anyhow::Result<bool> {
        self.exists(key).await
    }

    async fn get(&self, key: &str) -> anyhow::Result<bytes::Bytes> {
        self.get(key).await
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.delete(key).await
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

    #[test]
    fn head_bucket_error_is_not_found_only_for_confirmed_missing_bucket() {
        // Confirmed missing: S3 service codes, or a bare 404 from
        // S3-compatible stores that do not set a distinct error code.
        assert!(head_bucket_error_is_not_found(Some("NotFound"), Some(404)));
        assert!(head_bucket_error_is_not_found(
            Some("NoSuchBucket"),
            Some(404)
        ));
        assert!(head_bucket_error_is_not_found(Some("NoSuchBucket"), None));
        assert!(head_bucket_error_is_not_found(Some("404"), Some(404)));
        assert!(head_bucket_error_is_not_found(None, Some(404)));

        // Not missing: unreachable endpoint (no response), rejected
        // credentials, throttling, or server errors must not be treated as
        // "bucket does not exist".
        assert!(!head_bucket_error_is_not_found(None, None));
        assert!(!head_bucket_error_is_not_found(
            Some("Forbidden"),
            Some(403)
        ));
        assert!(!head_bucket_error_is_not_found(None, Some(403)));
        assert!(!head_bucket_error_is_not_found(
            Some("AccessDenied"),
            Some(403)
        ));
        assert!(!head_bucket_error_is_not_found(None, Some(500)));
        assert!(!head_bucket_error_is_not_found(Some("SlowDown"), Some(503)));
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

        match ObjectStore::new_with_options(
            endpoint,
            region,
            bucket,
            ObjectStoreOptions {
                auto_create_bucket: true,
            },
        )
        .await
        {
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

        if let Some(content_type) = content_type {
            assert!(
                !content_type.is_empty(),
                "object store returned an empty content type"
            );
        }
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
        if let Some(content_type) = content_type {
            assert!(
                !content_type.is_empty(),
                "object store returned an empty content type"
            );
        }
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

    #[test]
    fn missing_object_error_matches_no_such_key() {
        let error =
            GetObjectError::NoSuchKey(aws_sdk_s3::types::error::NoSuchKey::builder().build());
        assert!(super::is_missing_get_object_error(&error));
    }

    #[test]
    fn missing_object_error_rejects_other_service_errors() {
        let error = GetObjectError::InvalidObjectState(
            aws_sdk_s3::types::error::InvalidObjectState::builder().build(),
        );
        assert!(!super::is_missing_get_object_error(&error));
    }

    #[test]
    fn missing_object_error_rejects_non_sdk_errors() {
        // Infrastructure and integrity failures (e.g. from verify_blob_bytes)
        // are not AWS service errors and must not be treated as "missing".
        let error = anyhow::anyhow!("object integrity check failed");
        assert!(!super::is_missing_object_error(&error));
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

    #[test]
    fn verify_blob_bytes_accepts_matching_sha256_key() {
        let content = b"rustshare";
        let key = format!("blobs/{}", super::calculate_sha256(content));

        super::verify_blob_bytes(&key, content).expect("matching blob hash should pass");
    }

    #[test]
    fn verify_blob_bytes_rejects_mismatched_sha256_key() {
        let key = "blobs/0000000000000000000000000000000000000000000000000000000000000000";

        let result = super::verify_blob_bytes(key, b"rustshare");

        assert!(result.is_err(), "mismatched blob hash should fail");
    }

    #[test]
    fn verify_blob_bytes_ignores_non_content_addressed_keys() {
        super::verify_blob_bytes("meta/notes/file.json", br#"{"ok":true}"#)
            .expect("metadata sidecar keys are not content-addressed blobs");
    }

    #[tokio::test]
    async fn verified_byte_stream_from_path_rejects_mismatched_blob_key() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        tokio::fs::write(temp.path(), b"rustshare")
            .await
            .expect("write temp file");

        let key = "blobs/0000000000000000000000000000000000000000000000000000000000000000";
        let result = super::verified_byte_stream_from_path(key, temp.path()).await;

        assert!(result.is_err(), "mismatched file hash should fail");
    }

    #[tokio::test]
    async fn verified_byte_stream_from_path_uploads_private_verified_bytes() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let source_path = temp_dir.path().join("source");
        let replacement_path = temp_dir.path().join("replacement");

        tokio::fs::write(&source_path, b"verified")
            .await
            .expect("write source file");
        tokio::fs::write(&replacement_path, b"changed")
            .await
            .expect("write replacement file");
        let key = format!("blobs/{}", super::calculate_sha256(b"verified"));

        let stream = super::verified_byte_stream_from_path(&key, &source_path)
            .await
            .expect("verified stream");

        tokio::fs::rename(&replacement_path, &source_path)
            .await
            .expect("replace source path after verification");

        let data = stream.collect().await.expect("collect byte stream");

        assert_eq!(data.into_bytes(), Bytes::from_static(b"verified"));
    }

    #[tokio::test]
    async fn verify_blob_stream_reports_mismatch_at_end() {
        let chunks = futures::stream::iter([
            Ok(Bytes::from_static(b"rust")),
            Ok(Bytes::from_static(b"share")),
        ]);
        let key = "blobs/0000000000000000000000000000000000000000000000000000000000000000";
        let mut stream = Box::pin(super::verify_blob_stream(key, chunks));

        assert!(stream.next().await.expect("first chunk").is_ok());
        assert!(stream.next().await.expect("second chunk").is_ok());
        assert!(
            stream.next().await.expect("integrity result").is_err(),
            "stream should report checksum mismatch after EOF"
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn verify_blob_stream_passes_matching_hash() {
        let expected = super::calculate_sha256(b"rustshare");
        let key = format!("blobs/{expected}");
        let chunks = futures::stream::iter([
            Ok(Bytes::from_static(b"rust")),
            Ok(Bytes::from_static(b"share")),
        ]);
        let mut stream = Box::pin(super::verify_blob_stream(&key, chunks));

        assert_eq!(
            stream.next().await.expect("first chunk").unwrap(),
            Bytes::from_static(b"rust")
        );
        assert_eq!(
            stream.next().await.expect("second chunk").unwrap(),
            Bytes::from_static(b"share")
        );
        assert!(stream.next().await.is_none());
    }
}
