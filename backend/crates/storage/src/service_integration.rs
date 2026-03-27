//! Service layer integration for metadata_v2
//!
//! This module provides utilities for integrating the new metadata system
//! into the server application.

use std::sync::Arc;

use crate::metadata_v2::*;
use crate::repos::*;

/// Configuration for the metadata backend
#[derive(Debug, Clone)]
pub struct MetadataConfig {
    /// Backend type
    pub backend_type: MetadataBackendType,
    /// S3/RustFS endpoint
    pub rustfs_endpoint: String,
    /// S3/RustFS region
    pub rustfs_region: String,
    /// S3/RustFS bucket
    pub rustfs_bucket: String,
    /// Base prefix for metadata objects
    pub metadata_prefix: String,
    /// Namespace for isolation
    pub namespace: String,
    /// Enable runtime cache
    pub enable_cache: bool,
}

impl MetadataConfig {
    /// Load configuration from environment
    pub fn from_env() -> anyhow::Result<Self> {
        let backend_type = std::env::var("RUSTSHARE_METADATA_BACKEND")
            .unwrap_or_else(|_| "rustfs".to_string())
            .parse()?;

        Ok(Self {
            backend_type,
            rustfs_endpoint: std::env::var("RUSTFS_ENDPOINT")?,
            rustfs_region: std::env::var("RUSTFS_REGION")?,
            rustfs_bucket: std::env::var("RUSTFS_BUCKET")?,
            metadata_prefix: std::env::var("RUSTSHARE_METADATA_PREFIX")
                .unwrap_or_else(|_| "apps/rustshare".to_string()),
            namespace: std::env::var("RUSTSHARE_METADATA_NAMESPACE")
                .unwrap_or_else(|_| "default".to_string()),
            enable_cache: std::env::var("RUSTSHARE_METADATA_CACHE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),
        })
    }
}

/// Metadata system builder
pub struct MetadataSystemBuilder {
    config: MetadataConfig,
    s3_client: Option<aws_sdk_s3::Client>,
}

impl MetadataSystemBuilder {
    pub fn new(config: MetadataConfig) -> Self {
        Self {
            config,
            s3_client: None,
        }
    }

    pub fn with_s3_client(mut self, client: aws_sdk_s3::Client) -> Self {
        self.s3_client = Some(client);
        self
    }

    /// Build the metadata repository
    pub async fn build(self) -> anyhow::Result<Arc<dyn MetadataRepository>> {
        match self.config.backend_type {
            MetadataBackendType::RustFs => {
                self.build_rustfs_repo().await
            }
            MetadataBackendType::LocalFs => {
                self.build_localfs_repo().await
            }
            _ => {
                anyhow::bail!("Backend type {:?} not supported in this builder. Use RustFs or LocalFs.", self.config.backend_type)
            }
        }
    }

    async fn build_rustfs_repo(&self) -> anyhow::Result<Arc<dyn MetadataRepository>> {
        let client = self.s3_client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("S3 client required for RustFS backend"))?
            .clone();

        let cache = if self.config.enable_cache {
            Some(Arc::new(RuntimeMetadataCache::new()))
        } else {
            None
        };

        let backend_config = MetadataBackendConfig {
            base_prefix: self.config.metadata_prefix.clone(),
            namespace: self.config.namespace.clone(),
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        };

        // Create document store
        let doc_store: Arc<dyn MetadataDocumentStore> = Arc::new(
            RustFsDocumentStore::new(
                client.clone(),
                self.config.rustfs_bucket.clone(),
                backend_config.clone(),
            )
        );

        // Create event store
        let event_store: Arc<dyn EventLogStore> = Arc::new(
            RustFsEventStore::new(Arc::clone(&doc_store))
        );

        // Create path builder
        let path_builder = PathBuilder::new(
            self.config.metadata_prefix.clone(),
            self.config.namespace.clone(),
        );

        // Build combined repository
        let repo = CombinedMetadataRepository::from_components(
            doc_store,
            event_store,
            path_builder,
            cache,
        );

        Ok(Arc::new(repo))
    }

    async fn build_localfs_repo(&self) -> anyhow::Result<Arc<dyn MetadataRepository>> {
        let base_path = std::path::PathBuf::from(
            std::env::var("RUSTSHARE_LOCALFS_PATH")
                .unwrap_or_else(|_| "./local-metadata".to_string())
        );

        let cache = if self.config.enable_cache {
            Some(Arc::new(RuntimeMetadataCache::new()))
        } else {
            None
        };

        let backend_config = MetadataBackendConfig {
            base_prefix: self.config.metadata_prefix.clone(),
            namespace: self.config.namespace.clone(),
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        };

        // Create document store
        let doc_store: Arc<dyn MetadataDocumentStore> = Arc::new(
            LocalFsDocumentStore::new(base_path.clone(), backend_config.clone())
        );

        // Create event store
        let event_store: Arc<dyn EventLogStore> = Arc::new(
            RustFsEventStore::new(Arc::clone(&doc_store))
        );

        // Create path builder
        let path_builder = PathBuilder::new(
            self.config.metadata_prefix.clone(),
            self.config.namespace.clone(),
        );

        // Build combined repository
        let repo = CombinedMetadataRepository::from_components(
            doc_store,
            event_store,
            path_builder,
            cache,
        );

        Ok(Arc::new(repo))
    }
}

/// Initialize the metadata system based on configuration
pub async fn init_metadata_system(
    config: &MetadataConfig,
) -> anyhow::Result<(Arc<dyn MetadataRepository>, MetadataStoreCompat)> {
    let builder = MetadataSystemBuilder::new(config.clone());

    // Build the repository
    let repo = builder.build().await?;

    // Create compatibility layer
    let compat = MetadataStoreCompat::new(Arc::clone(&repo));

    Ok((repo, compat))
}

/// Helper to create S3 client from configuration
pub async fn create_s3_client(
    endpoint: &str,
    region: &str,
) -> anyhow::Result<aws_sdk_s3::Client> {
    use aws_config::BehaviorVersion;

    let config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(endpoint)
        .region(aws_config::Region::new(region.to_string()))
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&config)
        .force_path_style(true)
        .build();

    Ok(aws_sdk_s3::Client::from_conf(s3_config))
}

/// Admin endpoints handler (to be integrated into server)
pub struct MetadataAdminHandler {
    verifier: Arc<crate::admin::ParityVerifier>,
    repair_tool: Arc<crate::admin::RepairTool>,
    rebuild_tool: Arc<crate::admin::RebuildTool>,
}

impl MetadataAdminHandler {
    pub fn new(
        _postgres_repo: Arc<dyn MetadataRepository>,
        rustfs_repo: Arc<dyn MetadataRepository>,
    ) -> Self {
        // PostgreSQL repo is no longer used; kept in signature for API compatibility
        Self {
            verifier: Arc::new(crate::admin::ParityVerifier::new(
                Arc::clone(&rustfs_repo),
                Arc::clone(&rustfs_repo),
            )),
            repair_tool: Arc::new(crate::admin::RepairTool::new(Arc::clone(&rustfs_repo))),
            rebuild_tool: Arc::new(crate::admin::RebuildTool::new(Arc::clone(&rustfs_repo))),
        }
    }

    /// Verify parity for a specific folder
    pub async fn verify_folder(&self, folder_id: uuid::Uuid) -> Result<crate::admin::VerificationResult, RepositoryError> {
        self.verifier.verify_folder(folder_id).await
    }

    /// Verify parity for a specific file
    pub async fn verify_file(&self, file_id: uuid::Uuid) -> Result<crate::admin::VerificationResult, RepositoryError> {
        self.verifier.verify_file(file_id).await
    }

    /// Rebuild folder children index
    pub async fn rebuild_folder_index(&self, folder_id: uuid::Uuid) -> Result<crate::admin::OperationSummary, RepositoryError> {
        self.rebuild_tool.rebuild_folder_children_index(folder_id).await
    }

    /// Repair folder parent reference
    pub async fn repair_folder_parent(&self, folder_id: uuid::Uuid) -> Result<bool, RepositoryError> {
        self.repair_tool.repair_folder_parent(folder_id).await
    }
}
