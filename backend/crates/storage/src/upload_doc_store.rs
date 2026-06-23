//! Local filesystem document store used by the live upload-session path.
//!
//! This module extracted the minimal traits/types previously living under the
//! speculative `metadata_v2` scaffolding so upload sessions keep working while
//! that scaffolding is removed.

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;

/// Options for put operations with conditional semantics.
#[derive(Debug, Clone, Default)]
pub struct PutOptions {
    /// Only write if the object's ETag matches.
    pub if_match: Option<String>,
    /// Only write if the object's ETag does not match.
    pub if_none_match: Option<String>,
    /// Content type hint.
    pub content_type: Option<String>,
}

/// Result of a put operation.
#[derive(Debug, Clone)]
pub struct PutResult {
    /// ETag of the written object.
    pub etag: String,
    /// Version ID if supported by backend.
    pub version_id: Option<String>,
}

/// Metadata for an object.
#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    /// Content ETag.
    pub etag: String,
    /// Last modified time.
    pub last_modified: DateTime<Utc>,
    /// Content length.
    pub content_length: u64,
    /// Version ID if supported.
    pub version_id: Option<String>,
}

/// Core trait for metadata document storage.
///
/// This trait uses serialized bytes to be object-safe (dyn-compatible).
/// Callers are responsible for serialization/deserialization.
#[async_trait]
pub trait MetadataDocumentStore: Send + Sync {
    /// Get a document by key, returns raw bytes.
    async fn get_raw(&self, key: &str) -> Result<Option<(Vec<u8>, ObjectMetadata)>>;

    /// Get multiple documents by key in parallel.
    /// Returns (key, data, metadata) for all keys that exist. Missing keys are silently omitted.
    async fn get_multi_raw(&self, keys: &[&str]) -> Result<Vec<(String, Vec<u8>, ObjectMetadata)>>;

    /// Get document metadata without fetching content.
    async fn head(&self, key: &str) -> Result<Option<ObjectMetadata>>;

    /// Store a document from raw bytes.
    async fn put_raw(&self, key: &str, data: &[u8], opts: PutOptions) -> Result<PutResult>;

    /// Delete a document.
    async fn delete(&self, key: &str) -> Result<()>;

    /// List objects with a prefix (for debugging/listing only).
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>>;
}

/// Extension trait for typed operations.
#[async_trait]
pub trait MetadataDocumentStoreExt: MetadataDocumentStore {
    /// Get a document by key.
    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<(T, ObjectMetadata)>> {
        match self.get_raw(key).await? {
            Some((data, meta)) => {
                let doc = serde_json::from_slice(&data)?;
                Ok(Some((doc, meta)))
            }
            None => Ok(None),
        }
    }

    /// Get multiple documents by key in parallel.
    async fn get_multi<T: DeserializeOwned>(
        &self,
        keys: &[&str],
    ) -> Result<Vec<(String, T, ObjectMetadata)>> {
        let raw_results = self.get_multi_raw(keys).await?;
        let mut results = Vec::with_capacity(raw_results.len());
        for (key, data, meta) in raw_results {
            let doc = serde_json::from_slice(&data)?;
            results.push((key, doc, meta));
        }
        Ok(results)
    }

    /// Store a document.
    async fn put<T: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
        opts: PutOptions,
    ) -> Result<PutResult> {
        let data = serde_json::to_vec(value)?;
        self.put_raw(key, &data, opts).await
    }
}

// Auto-implement the extension trait for all MetadataDocumentStore types.
#[async_trait]
impl<T: MetadataDocumentStore + ?Sized> MetadataDocumentStoreExt for T {}

/// Configuration for the metadata backend.
#[derive(Debug, Clone)]
pub struct MetadataBackendConfig {
    /// Base path/prefix for all metadata objects.
    pub base_prefix: String,
    /// Namespace for app isolation.
    pub namespace: String,
    /// Enable optimistic concurrency.
    pub enable_optimistic_concurrency: bool,
    /// Fallback to leases if conditional writes fail.
    pub fallback_to_leases: bool,
}

impl Default for MetadataBackendConfig {
    fn default() -> Self {
        Self {
            base_prefix: "apps/rustshare".to_string(),
            namespace: "default".to_string(),
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        }
    }
}

/// Local filesystem-backed metadata document store.
pub struct LocalFsDocumentStore {
    base_path: PathBuf,
    config: MetadataBackendConfig,
}

impl LocalFsDocumentStore {
    /// Create a new local filesystem document store.
    pub fn new(base_path: PathBuf, config: MetadataBackendConfig) -> Self {
        Self { base_path, config }
    }

    fn build_path(&self, key: &str) -> PathBuf {
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

        // Compute ETag from content hash.
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

        // Read content to compute ETag.
        let data = tokio::fs::read(&path).await?;
        let etag = format!("\"{:x}\"", md5::compute(&data));

        Ok(Some(ObjectMetadata {
            etag,
            last_modified: modified_dt,
            content_length: metadata.len(),
            version_id: None,
        }))
    }

    async fn put_raw(&self, key: &str, data: &[u8], opts: PutOptions) -> Result<PutResult> {
        let path = self.build_path(key);

        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Check conditional writes.
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

        // Write atomically using temp file + rename.
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
                // Recurse into subdirectories.
                let sub_prefix = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
                let sub_keys = self.list_prefix(&sub_prefix).await?;
                keys.extend(sub_keys);
            }
        }

        Ok(keys)
    }
}

/// Local filesystem-backed blob store (kept for API symmetry; currently unused).
pub struct LocalFsBlobStore {
    base_path: PathBuf,
}

impl LocalFsBlobStore {
    /// Create a new local filesystem blob store.
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Generate content-addressed key.
    pub fn content_key(&self, hash: &str) -> String {
        format!(
            "shared/blobs/sha256/{}/{}/{}",
            &hash[0..2],
            &hash[2..4],
            hash
        )
    }

    /// Store blob data.
    pub async fn put(&self, key: &str, data: Bytes) -> Result<PutResult> {
        let path = self.base_path.join(key);

        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Write atomically.
        let temp_path = path.with_extension("tmp");
        tokio::fs::write(&temp_path, &data).await?;
        tokio::fs::rename(&temp_path, &path).await?;

        let etag = format!("\"{:x}\"", md5::compute(&data));

        Ok(PutResult {
            etag,
            version_id: None,
        })
    }

    /// Get blob data.
    pub async fn get(&self, key: &str) -> Result<Option<Bytes>> {
        let path = self.base_path.join(key);

        if !path.exists() {
            return Ok(None);
        }

        let data = tokio::fs::read(&path).await?;
        Ok(Some(Bytes::from(data)))
    }

    /// Check if blob exists.
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let path = self.base_path.join(key);
        Ok(path.exists())
    }

    /// Delete blob.
    pub async fn delete(&self, key: &str) -> Result<()> {
        let path = self.base_path.join(key);

        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestDoc {
        pub id: String,
        pub value: i32,
    }

    #[tokio::test]
    async fn test_localfs_get_multi_returns_docs_and_omits_missing() {
        let temp_dir = TempDir::new().unwrap();
        let config = MetadataBackendConfig {
            base_prefix: "test".to_string(),
            namespace: "default".to_string(),
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        };
        let store = LocalFsDocumentStore::new(temp_dir.path().to_path_buf(), config);

        // Store two documents.
        let doc1 = TestDoc {
            id: "doc1".to_string(),
            value: 1,
        };
        let doc2 = TestDoc {
            id: "doc2".to_string(),
            value: 2,
        };
        store
            .put("doc1", &doc1, PutOptions::default())
            .await
            .unwrap();
        store
            .put("doc2", &doc2, PutOptions::default())
            .await
            .unwrap();

        // Fetch multiple including a missing key.
        let keys = vec!["doc1", "missing", "doc2"];
        let results = store.get_multi::<TestDoc>(&keys).await.unwrap();

        // Should return 2 results, omitting missing.
        assert_eq!(results.len(), 2);

        // Order should match input order for existing keys.
        assert_eq!(results[0].0, "doc1");
        assert_eq!(results[0].1, doc1);
        assert_eq!(results[1].0, "doc2");
        assert_eq!(results[1].1, doc2);
    }

    #[tokio::test]
    async fn test_localfs_get_multi_empty_input() {
        let temp_dir = TempDir::new().unwrap();
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
