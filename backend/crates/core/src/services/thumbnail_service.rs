//! Thumbnail generation service

use std::sync::Arc;
use uuid::Uuid;

use crate::domain::{
    get_file_thumbnail_category, is_file_thumbnail_supported, FileThumbnail, ThumbnailCategory,
    ThumbnailSize,
};
use crate::services::ObjectStoreOps;

/// Persistence operations used by thumbnail generation and retrieval.
#[async_trait::async_trait]
pub trait ThumbnailRepository: Send + Sync {
    async fn get_thumbnail(
        &self,
        file_id: Uuid,
        size: ThumbnailSize,
    ) -> anyhow::Result<Option<FileThumbnail>>;
    async fn get_file_metadata(
        &self,
        file_id: Uuid,
    ) -> anyhow::Result<Option<(String, i64, String)>>;
    async fn save_thumbnail(
        &self,
        file_id: Uuid,
        size: ThumbnailSize,
        storage_path: &str,
        content_type: &str,
    ) -> anyhow::Result<FileThumbnail>;
    async fn delete_thumbnails(&self, file_id: Uuid) -> anyhow::Result<()>;
}

/// Domain-neutral rendering boundary implemented by the application adapter.
#[async_trait::async_trait]
pub trait ThumbnailRenderer: Send + Sync {
    /// Render a supported category to bytes and its response content type.
    async fn render(
        &self,
        content: &[u8],
        file_name: &str,
        size: ThumbnailSize,
        category: ThumbnailCategory,
    ) -> Result<RenderedThumbnail, String>;
}

/// Rendered bytes and their media type, ready for object storage.
#[derive(Debug)]
pub struct RenderedThumbnail {
    /// Encoded thumbnail bytes.
    pub bytes: Vec<u8>,
    /// Media type associated with `bytes`.
    pub content_type: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ThumbnailError {
    #[error("File not found")]
    NotFound,
    #[error("Unsupported file type for thumbnail generation")]
    UnsupportedType,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Generation error: {0}")]
    Generation(String),
    #[error("Database error: {0}")]
    Database(String),
}

/// Thumbnail service for generating and retrieving file thumbnails
pub struct ThumbnailService<S> {
    repository: Arc<dyn ThumbnailRepository>,
    storage: Arc<S>,
    renderer: Arc<dyn ThumbnailRenderer>,
}

impl<S> ThumbnailService<S>
where
    S: ObjectStoreOps,
{
    pub fn new(
        repository: Arc<dyn ThumbnailRepository>,
        storage: Arc<S>,
        renderer: Arc<dyn ThumbnailRenderer>,
    ) -> Self {
        Self {
            repository,
            storage,
            renderer,
        }
    }

    /// Check if a thumbnail exists and return it
    pub async fn get_thumbnail(
        &self,
        file_id: Uuid,
        size: ThumbnailSize,
    ) -> Result<Option<FileThumbnail>, ThumbnailError> {
        self.repository
            .get_thumbnail(file_id, size)
            .await
            .map_err(|e| ThumbnailError::Database(e.to_string()))
    }

    /// Check if thumbnail generation is supported for a file type
    pub fn is_supported(&self, mime_type: &str, file_name: &str) -> bool {
        is_file_thumbnail_supported(mime_type, file_name)
    }

    /// Generate a thumbnail for a file
    pub async fn generate_thumbnail(
        &self,
        file_id: Uuid,
        mime_type: &str,
        file_name: &str,
        size: ThumbnailSize,
    ) -> Result<FileThumbnail, ThumbnailError> {
        tracing::info!(file_id = %file_id, mime_type = mime_type, file_name = file_name, size = size.as_str(), "Starting thumbnail generation");

        // Check if file exists and get content hash for storage lookup
        let file_row = self
            .repository
            .get_file_metadata(file_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Database error fetching file");
                ThumbnailError::Database(e.to_string())
            })?;

        let (db_file_name, _file_size, content_hash) = match file_row {
            Some(row) => row,
            None => {
                tracing::warn!(file_id = %file_id, "File not found for thumbnail generation");
                return Err(ThumbnailError::NotFound);
            }
        };

        // Use filename from DB if not provided
        let file_name = if file_name.is_empty() {
            &db_file_name
        } else {
            file_name
        };

        if !is_file_thumbnail_supported(mime_type, file_name) {
            tracing::warn!(
                mime_type = mime_type,
                file_name = file_name,
                "Unsupported file type for thumbnail"
            );
            return Err(ThumbnailError::UnsupportedType);
        }

        // Get the file content from storage using content hash
        let file_path = format!("blobs/{}", content_hash);
        tracing::debug!(file_path = %file_path, "Fetching file content from storage");

        let content = self
            .storage
            .get(&file_path)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, file_path = %file_path, "Storage error fetching file content");
                ThumbnailError::Storage(e.to_string())
            })?;

        tracing::debug!(
            content_size = content.len(),
            "File content fetched successfully"
        );

        // Generate thumbnail based on file type
        let category = get_file_thumbnail_category(mime_type, file_name);
        tracing::debug!(category = ?category, "Thumbnail category determined");

        if category == ThumbnailCategory::Unsupported {
            return Err(ThumbnailError::UnsupportedType);
        }
        let rendered = self
            .renderer
            .render(&content, file_name, size, category)
            .await
            .map_err(ThumbnailError::Generation)?;
        let RenderedThumbnail {
            bytes: thumbnail_data,
            content_type,
        } = rendered;

        tracing::debug!(thumbnail_size = thumbnail_data.len(), content_type = %content_type, "Thumbnail generated");

        // Store thumbnail
        let thumbnail_path = format!("thumbnails/{}/{}.webp", file_id, size.as_str());
        self.storage
            .put(&thumbnail_path, thumbnail_data.into())
            .await
            .map_err(|e| {
                tracing::error!(error = %e, path = %thumbnail_path, "Storage error saving thumbnail");
                ThumbnailError::Storage(e.to_string())
            })?;

        tracing::debug!(path = %thumbnail_path, "Thumbnail saved to storage");

        // Save to database
        let thumbnail = self
            .repository
            .save_thumbnail(file_id, size, &thumbnail_path, &content_type)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Database error saving thumbnail metadata");
                ThumbnailError::Database(e.to_string())
            })?;

        tracing::info!(file_id = %file_id, size = size.as_str(), "Thumbnail generation completed successfully");
        Ok(thumbnail)
    }

    /// Delete thumbnails when file is updated
    pub async fn invalidate_thumbnails(&self, file_id: Uuid) -> Result<(), ThumbnailError> {
        // Delete from database
        self.repository
            .delete_thumbnails(file_id)
            .await
            .map_err(|e| ThumbnailError::Database(e.to_string()))?;

        // Delete from storage (best effort - don't fail if storage delete fails)
        for size in [ThumbnailSize::Sm, ThumbnailSize::Md, ThumbnailSize::Lg] {
            let path = format!("thumbnails/{}/{}.webp", file_id, size.as_str());
            if let Err(e) = self.storage.delete(&path).await {
                tracing::debug!(path = %path, error = %e, "failed to delete thumbnail from storage");
            }
        }

        Ok(())
    }

    /// Get thumbnail data from storage
    pub async fn get_thumbnail_data(&self, storage_path: &str) -> Result<Vec<u8>, ThumbnailError> {
        self.storage
            .get(storage_path)
            .await
            .map_err(|e| ThumbnailError::Storage(e.to_string()))
            .map(|bytes| bytes.to_vec())
    }
}
