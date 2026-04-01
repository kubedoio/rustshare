//! Storage backend implementations

use super::*;
use aws_sdk_s3::Client as S3Client;
use bytes::Bytes;
use chrono::Datelike;
use std::sync::Arc;
use tracing::warn;

/// RustFS/S3-backed metadata document store
pub struct RustFsDocumentStore {
    client: S3Client,
    bucket: String,
    config: MetadataBackendConfig,
}

impl RustFsDocumentStore {
    pub fn new(client: S3Client, bucket: String, config: MetadataBackendConfig) -> Self {
        Self {
            client,
            bucket,
            config,
        }
    }
    
    /// Build the full object key from a document key
    fn build_key(&self, key: &str) -> String {
        format!(
            "{}/{}/{}/{}",
            self.config.base_prefix,
            self.config.namespace,
            "meta",
            key
        )
    }
}

#[async_trait]
impl MetadataDocumentStore for RustFsDocumentStore {
    async fn get_raw(&self, key: &str) -> Result<Option<(Vec<u8>, ObjectMetadata)>> {
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
                // Convert AWS DateTime to chrono DateTime
                let last_modified = output
                    .last_modified()
                    .and_then(|dt| {
                        let secs = dt.secs();
                        let nanos = dt.subsec_nanos();
                        chrono::DateTime::from_timestamp(secs, nanos)
                    })
                    .unwrap_or_else(Utc::now);
                let content_length = output.content_length.unwrap_or(0) as u64;
                let version_id = output.version_id().map(|s| s.to_string());
                
                let data = output.body.collect().await?;
                let bytes = data.into_bytes();
                
                let metadata = ObjectMetadata {
                    etag,
                    last_modified,
                    content_length,
                    version_id,
                };
                
                Ok(Some((bytes.to_vec(), metadata)))
            }
            Err(e) => {
                // Check if it's a not found error
                let err_str = e.to_string();
                if err_str.contains("NoSuchKey") || err_str.contains("NotFound") {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }
    
    async fn get_multi_raw(&self, keys: &[&str]) -> Result<Vec<(String, Vec<u8>, ObjectMetadata)>> {
        let futures: Vec<_> = keys.iter().map(|key| self.get_raw(key)).collect();
        let results = futures::future::join_all(futures).await;
        
        let mut out = Vec::new();
        for (i, result) in results.into_iter().enumerate() {
            if let Some((data, meta)) = result? {
                out.push((keys[i].to_string(), data, meta));
            }
        }
        Ok(out)
    }
    
    async fn head(&self, key: &str) -> Result<Option<ObjectMetadata>> {
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
                // Convert AWS DateTime to chrono DateTime
                let last_modified = output
                    .last_modified()
                    .and_then(|dt| {
                        let secs = dt.secs();
                        let nanos = dt.subsec_nanos();
                        chrono::DateTime::from_timestamp(secs, nanos)
                    })
                    .unwrap_or_else(Utc::now);
                let content_length = output.content_length.unwrap_or(0) as u64;
                let version_id = output.version_id().map(|s| s.to_string());
                
                Ok(Some(ObjectMetadata {
                    etag,
                    last_modified,
                    content_length,
                    version_id,
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
    ) -> Result<PutResult> {
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
            version_id: output.version_id().map(|s| s.to_string()),
        })
    }
    
    async fn delete(&self, key: &str) -> Result<()> {
        let object_key = self.build_key(key);
        
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&object_key)
            .send()
            .await?;
        
        Ok(())
    }
    
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>> {
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
                        // Strip the prefix to return relative keys
                        let relative_key = key
                            .strip_prefix(&format!("{}/{}/{}/", 
                                self.config.base_prefix,
                                self.config.namespace,
                                "meta"
                            ))
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

/// Local filesystem-backed metadata document store
pub struct LocalFsDocumentStore {
    base_path: std::path::PathBuf,
    config: MetadataBackendConfig,
}

impl LocalFsDocumentStore {
    pub fn new(base_path: std::path::PathBuf, config: MetadataBackendConfig) -> Self {
        Self {
            base_path,
            config,
        }
    }
    
    fn build_path(&self, key: &str) -> std::path::PathBuf {
        self.base_path
            .join(&self.config.base_prefix)
            .join(&self.config.namespace)
            .join("meta")
            .join(key)
    }
}

#[async_trait]
impl MetadataDocumentStore for LocalFsDocumentStore {
    async fn get_raw(&self, key: &str) -> Result<Option<(Vec<u8>, ObjectMetadata)>> {
        let path = self.build_path(key);
        
        if !path.exists() {
            return Ok(None);
        }
        
        let data = tokio::fs::read(&path).await?;
        
        let metadata = tokio::fs::metadata(&path).await?;
        let modified = metadata.modified()?;
        let modified_dt = DateTime::<Utc>::from(modified);
        
        // Compute ETag from content hash
        let etag = format!("\"{:x}\"", md5::compute(&data));
        
        let object_metadata = ObjectMetadata {
            etag,
            last_modified: modified_dt,
            content_length: metadata.len(),
            version_id: None,
        };
        
        Ok(Some((data, object_metadata)))
    }
    
    async fn get_multi_raw(&self, keys: &[&str]) -> Result<Vec<(String, Vec<u8>, ObjectMetadata)>> {
        let futures: Vec<_> = keys.iter().map(|key| self.get_raw(key)).collect();
        let results = futures::future::join_all(futures).await;
        
        let mut out = Vec::new();
        for (i, result) in results.into_iter().enumerate() {
            if let Some((data, meta)) = result? {
                out.push((keys[i].to_string(), data, meta));
            }
        }
        Ok(out)
    }
    
    async fn head(&self, key: &str) -> Result<Option<ObjectMetadata>> {
        let path = self.build_path(key);
        
        if !path.exists() {
            return Ok(None);
        }
        
        let metadata = tokio::fs::metadata(&path).await?;
        let modified = metadata.modified()?;
        let modified_dt = DateTime::<Utc>::from(modified);
        
        // Read content to compute ETag
        let data = tokio::fs::read(&path).await?;
        let etag = format!("\"{:x}\"", md5::compute(&data));
        
        Ok(Some(ObjectMetadata {
            etag,
            last_modified: modified_dt,
            content_length: metadata.len(),
            version_id: None,
        }))
    }
    
    async fn put_raw(
        &self,
        key: &str,
        data: &[u8],
        opts: PutOptions,
    ) -> Result<PutResult> {
        let path = self.build_path(key);
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        // Check conditional writes
        if opts.if_match.is_some() || opts.if_none_match.is_some() {
            if let Some(expected_etag) = opts.if_match {
                if path.exists() {
                    let existing = tokio::fs::read(&path).await?;
                    let actual_etag = format!("\"{:x}\"", md5::compute(&existing));
                    if actual_etag != expected_etag {
                        return Err(anyhow::anyhow!(
                            "Precondition failed: ETag mismatch (expected {}, got {})",
                            expected_etag,
                            actual_etag
                        ));
                    }
                } else {
                    return Err(anyhow::anyhow!(
                        "Precondition failed: document does not exist"
                    ));
                }
            }
            
            if let Some(expected_etag) = opts.if_none_match {
                if path.exists() {
                    let existing = tokio::fs::read(&path).await?;
                    let actual_etag = format!("\"{:x}\"", md5::compute(&existing));
                    if actual_etag == expected_etag {
                        return Err(anyhow::anyhow!(
                            "Precondition failed: document already exists with matching ETag"
                        ));
                    }
                }
            }
        }
        
        let etag = format!("\"{:x}\"", md5::compute(data));
        
        // Write atomically using temp file + rename
        let temp_path = path.with_extension("tmp");
        tokio::fs::write(&temp_path, data).await?;
        tokio::fs::rename(&temp_path, &path).await?;
        
        Ok(PutResult {
            etag,
            version_id: None,
        })
    }
    
    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.build_path(key);
        
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        
        Ok(())
    }
    
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let path = self.build_path(prefix);
        
        let mut keys = Vec::new();
        
        if !path.exists() {
            return Ok(keys);
        }
        
        let base_meta_path = self
            .base_path
            .join(&self.config.base_prefix)
            .join(&self.config.namespace)
            .join("meta");
        
        let mut entries = tokio::fs::read_dir(&path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let entry_path = entry.path();
            
            if entry.file_type().await?.is_file() {
                if let Ok(relative) = entry_path.strip_prefix(&base_meta_path) {
                    keys.push(relative.to_string_lossy().to_string());
                }
            } else if entry.file_type().await?.is_dir() {
                // Recurse into subdirectories
                let sub_prefix = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
                let sub_keys = self.list_prefix(&sub_prefix).await?;
                keys.extend(sub_keys);
            }
        }
        
        Ok(keys)
    }
}

/// RustFS/S3-backed blob store
pub struct RustFsBlobStore {
    client: S3Client,
    bucket: String,
}

impl RustFsBlobStore {
    pub fn new(client: S3Client, bucket: String) -> Self {
        Self { client, bucket }
    }
}

#[async_trait]
impl BlobStore for RustFsBlobStore {
    async fn put(&self, key: &str, data: Bytes) -> Result<PutResult> {
        let output = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(data.into())
            .send()
            .await?;
        
        Ok(PutResult {
            etag: output.e_tag().unwrap_or("").to_string(),
            version_id: output.version_id().map(|s| s.to_string()),
        })
    }
    
    async fn get(&self, key: &str) -> Result<Option<Bytes>> {
        match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
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
    
    async fn exists(&self, key: &str) -> Result<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
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
    
    async fn delete(&self, key: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;
        Ok(())
    }
    
    fn content_key(&self, hash: &str) -> String {
        format!("shared/blobs/sha256/{}/{}/{}", &hash[0..2], &hash[2..4], hash)
    }
}

/// Local filesystem-backed blob store
pub struct LocalFsBlobStore {
    base_path: std::path::PathBuf,
}

impl LocalFsBlobStore {
    pub fn new(base_path: std::path::PathBuf) -> Self {
        Self { base_path }
    }
}

#[async_trait]
impl BlobStore for LocalFsBlobStore {
    async fn put(&self, key: &str, data: Bytes) -> Result<PutResult> {
        let path = self.base_path.join(key);
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        // Write atomically
        let temp_path = path.with_extension("tmp");
        tokio::fs::write(&temp_path, &data).await?;
        tokio::fs::rename(&temp_path, &path).await?;
        
        let etag = format!("\"{:x}\"", md5::compute(&data));
        
        Ok(PutResult {
            etag,
            version_id: None,
        })
    }
    
    async fn get(&self, key: &str) -> Result<Option<Bytes>> {
        let path = self.base_path.join(key);
        
        if !path.exists() {
            return Ok(None);
        }
        
        let data = tokio::fs::read(&path).await?;
        Ok(Some(Bytes::from(data)))
    }
    
    async fn exists(&self, key: &str) -> Result<bool> {
        let path = self.base_path.join(key);
        Ok(path.exists())
    }
    
    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.base_path.join(key);
        
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        
        Ok(())
    }
    
    fn content_key(&self, hash: &str) -> String {
        format!("shared/blobs/sha256/{}/{}/{}", &hash[0..2], &hash[2..4], hash)
    }
}

/// RustFS/S3-backed event log store
pub struct RustFsEventStore {
    doc_store: Arc<dyn MetadataDocumentStore>,
}

impl RustFsEventStore {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>) -> Self {
        Self { doc_store }
    }
    
    fn build_event_key(&self, event: &EventDocument) -> String {
        let date = event.occurred_at;
        format!(
            "events/{:04}/{:02}/{:02}/{}.json",
            date.year(),
            date.month(),
            date.day(),
            event.id
        )
    }
}

#[async_trait]
impl EventLogStore for RustFsEventStore {
    async fn append(&self, event: &EventDocument) -> Result<()> {
        let key = self.build_event_key(event);
        
        self.doc_store
            .put(&key, event, PutOptions::default())
            .await?;
        
        Ok(())
    }
    
    async fn read_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<EventDocument>> {
        // List events by date prefix
        let mut events = Vec::new();
        
        let mut current = start.date_naive();
        let end_date = end.date_naive();
        
        while current <= end_date && events.len() < limit {
            let prefix = format!(
                "events/{:04}/{:02}/{:02}/",
                current.year(),
                current.month(),
                current.day()
            );
            
            let keys = self.doc_store.list_prefix(&prefix).await?;
            
            for key in keys {
                if let Some((event, _)) = self.doc_store.get::<EventDocument>(&key).await? {
                    if event.occurred_at >= start && event.occurred_at <= end {
                        events.push(event);
                    }
                }
            }
            
            current = current.succ_opt().unwrap_or(current);
        }
        
        // Sort by occurred_at
        events.sort_by(|a, b| a.occurred_at.cmp(&b.occurred_at));
        events.truncate(limit);
        
        Ok(events)
    }
    
    async fn read_for_resource(
        &self,
        resource_type: &str,
        resource_id: &str,
        limit: usize,
    ) -> Result<Vec<EventDocument>> {
        // This requires scanning all events - use sparingly
        // In production, maintain a secondary index
        tracing::warn!(
            "Scanning all events for resource {}/{} - consider using an index",
            resource_type,
            resource_id
        );
        
        let now = Utc::now();
        let start = now - chrono::Duration::days(30); // Limit to recent events
        
        let events = self.read_range(start, now, 10000).await?;
        
        let filtered: Vec<_> = events
            .into_iter()
            .filter(|e| e.resource_type == resource_type && e.resource_id.to_string() == resource_id)
            .take(limit)
            .collect();
        
        Ok(filtered)
    }
    
    async fn read_since(
        &self,
        since: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<EventDocument>> {
        // Read events from the timestamp up to now
        let now = Utc::now();
        let mut events = self.read_range(since, now, limit).await?;
        
        // Filter to only events strictly after 'since'
        events.retain(|e| e.occurred_at > since);
        
        Ok(events)
    }
}

/// Simple index store wrapper around document store
pub struct DocumentIndexStore {
    doc_store: Arc<dyn MetadataDocumentStore>,
    prefix: String,
}

impl DocumentIndexStore {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, prefix: String) -> Self {
        Self { doc_store, prefix }
    }
    
    fn build_key(&self, key: &str) -> String {
        format!("{}/{}", self.prefix, key)
    }
}

#[async_trait]
impl IndexStore for DocumentIndexStore {
    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let full_key = self.build_key(key);
        Ok(self.doc_store.get::<T>(&full_key).await?.map(|(doc, _)| doc))
    }
    
    async fn put<T: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
        opts: PutOptions,
    ) -> Result<PutResult> {
        let full_key = self.build_key(key);
        self.doc_store.put(&full_key, value, opts).await
    }
    
    async fn delete(&self, key: &str) -> Result<()> {
        let full_key = self.build_key(key);
        self.doc_store.delete(&full_key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestDoc {
        pub id: String,
        pub value: i32,
    }

    #[tokio::test]
    async fn test_localfs_get_multi_returns_docs_and_omits_missing() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let config = MetadataBackendConfig {
            base_prefix: "test".to_string(),
            namespace: "default".to_string(),
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        };
        let store = LocalFsDocumentStore::new(temp_dir.path().to_path_buf(), config);

        // Store two documents
        let doc1 = TestDoc {
            id: "doc1".to_string(),
            value: 1,
        };
        let doc2 = TestDoc {
            id: "doc2".to_string(),
            value: 2,
        };
        store.put("doc1", &doc1, PutOptions::default()).await.unwrap();
        store.put("doc2", &doc2, PutOptions::default()).await.unwrap();

        // Fetch multiple including a missing key
        let keys = vec!["doc1", "missing", "doc2"];
        let results = store.get_multi::<TestDoc>(&keys).await.unwrap();

        // Should return 2 results, omitting missing
        assert_eq!(results.len(), 2);

        // Order should match input order for existing keys
        assert_eq!(results[0].0, "doc1");
        assert_eq!(results[0].1, doc1);
        assert_eq!(results[1].0, "doc2");
        assert_eq!(results[1].1, doc2);
    }

    #[tokio::test]
    async fn test_localfs_get_multi_empty_input() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let config = MetadataBackendConfig {
            base_prefix: "test".to_string(),
            namespace: "default".to_string(),
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        };
        let store = LocalFsDocumentStore::new(temp_dir.path().to_path_buf(), config);

        let results = store.get_multi::<TestDoc>(&[]).await.unwrap();
        assert!(results.is_empty());
    }
}
