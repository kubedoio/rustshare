//! Metadata v2 - Object-store-native metadata layer
//!
//! This module provides the new metadata storage abstractions that replace
//! PostgreSQL as the canonical metadata store.

pub mod compat;
pub mod schemas;
pub mod stores;
pub mod coordination;
pub mod runtime_cache;
pub mod user_bucket_store;

pub use compat::*;
pub use schemas::*;
pub use stores::*;
pub use coordination::*;
pub use runtime_cache::*;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};

/// Options for put operations with conditional semantics
#[derive(Debug, Clone, Default)]
pub struct PutOptions {
    /// Only write if the object's ETag matches
    pub if_match: Option<String>,
    /// Only write if the object's ETag does not match
    pub if_none_match: Option<String>,
    /// Content type hint
    pub content_type: Option<String>,
}

/// Result of a put operation
#[derive(Debug, Clone)]
pub struct PutResult {
    /// ETag of the written object
    pub etag: String,
    /// Version ID if supported by backend
    pub version_id: Option<String>,
}

/// Metadata for an object
#[derive(Debug, Clone)]
pub struct ObjectMetadata {
    /// Content ETag
    pub etag: String,
    /// Last modified time
    pub last_modified: DateTime<Utc>,
    /// Content length
    pub content_length: u64,
    /// Version ID if supported
    pub version_id: Option<String>,
}

/// Core trait for metadata document storage
/// 
/// Note: This trait uses serialized bytes to be object-safe (dyn-compatible).
/// Callers are responsible for serialization/deserialization.
#[async_trait]
pub trait MetadataDocumentStore: Send + Sync {
    /// Get a document by key, returns raw bytes
    async fn get_raw(&self, key: &str) -> Result<Option<(Vec<u8>, ObjectMetadata)>>;
    
    /// Get document metadata without fetching content
    async fn head(&self, key: &str) -> Result<Option<ObjectMetadata>>;
    
    /// Store a document from raw bytes
    async fn put_raw(
        &self,
        key: &str,
        data: &[u8],
        opts: PutOptions,
    ) -> Result<PutResult>;
    
    /// Delete a document
    async fn delete(&self, key: &str) -> Result<()>;
    
    /// List objects with a prefix (for debugging/rebuild only)
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>>;
}

/// Extension trait for typed operations
#[async_trait]
pub trait MetadataDocumentStoreExt: MetadataDocumentStore {
    /// Get a document by key
    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<(T, ObjectMetadata)>> {
        match self.get_raw(key).await? {
            Some((data, meta)) => {
                let doc = serde_json::from_slice(&data)?;
                Ok(Some((doc, meta)))
            }
            None => Ok(None),
        }
    }
    
    /// Store a document
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

// Auto-implement the extension trait for all MetadataDocumentStore types
#[async_trait]
impl<T: MetadataDocumentStore + ?Sized> MetadataDocumentStoreExt for T {}

/// Core trait for blob storage (immutable content)
#[async_trait]
pub trait BlobStore: Send + Sync {
    /// Store blob data
    async fn put(&self, key: &str, data: Bytes) -> Result<PutResult>;
    
    /// Get blob data
    async fn get(&self, key: &str) -> Result<Option<Bytes>>;
    
    /// Check if blob exists
    async fn exists(&self, key: &str) -> Result<bool>;
    
    /// Delete blob (only for policy enforcement, not normal operation)
    async fn delete(&self, key: &str) -> Result<()>;
    
    /// Generate content-addressed key
    fn content_key(&self, hash: &str) -> String;
}

/// Core trait for append-only event storage
#[async_trait]
pub trait EventLogStore: Send + Sync {
    /// Append an event
    async fn append(&self, event: &EventDocument) -> Result<()>;
    
    /// Read events by date range
    async fn read_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<EventDocument>>;
    
    /// Read events for a specific resource
    async fn read_for_resource(
        &self,
        resource_type: &str,
        resource_id: &str,
        limit: usize,
    ) -> Result<Vec<EventDocument>>;
}

/// Core trait for index/projection storage
#[async_trait]
pub trait IndexStore: Send + Sync {
    /// Get an index entry
    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>>;
    
    /// Store an index entry
    async fn put<T: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
        opts: PutOptions,
    ) -> Result<PutResult>;
    
    /// Delete an index entry
    async fn delete(&self, key: &str) -> Result<()>;
}

/// Configuration for the metadata backend
#[derive(Debug, Clone)]
pub struct MetadataBackendConfig {
    /// Base path/prefix for all metadata objects
    pub base_prefix: String,
    /// Namespace for app isolation
    pub namespace: String,
    /// Enable optimistic concurrency
    pub enable_optimistic_concurrency: bool,
    /// Fallback to leases if conditional writes fail
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

/// Backend type selector for migration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataBackendType {
    /// PostgreSQL only (legacy)
    Postgres,
    /// RustFS/object store only
    RustFs,
    /// Local filesystem (development)
    LocalFs,
    /// Write to both, read from Postgres
    DualWrite,
    /// Write to both, compare reads
    DualRead,
}

impl std::str::FromStr for MetadataBackendType {
    type Err = anyhow::Error;
    
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "postgres" => Ok(Self::Postgres),
            "rustfs" => Ok(Self::RustFs),
            "localfs" => Ok(Self::LocalFs),
            "dual_write" => Ok(Self::DualWrite),
            "dual_read" => Ok(Self::DualRead),
            _ => Err(anyhow::anyhow!("Unknown backend type: {}", s)),
        }
    }
}
