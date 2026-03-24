//! Thumbnail generation service

use std::sync::Arc;
use uuid::Uuid;

use crate::domain::{
    FileThumbnail, ThumbnailSize, is_thumbnail_supported,
};
use crate::services::ObjectStoreOps;

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
    db_pool: sqlx::PgPool,
    storage: Arc<S>,
}

impl<S> ThumbnailService<S>
where
    S: ObjectStoreOps,
{
    pub fn new(db_pool: sqlx::PgPool, storage: Arc<S>) -> Self {
        Self { db_pool, storage }
    }

    /// Check if a thumbnail exists and return it
    pub async fn get_thumbnail(
        &self,
        file_id: Uuid,
        size: ThumbnailSize,
    ) -> Result<Option<FileThumbnail>, ThumbnailError> {
        let row = sqlx::query_as::<_, FileThumbnail>(
            r#"
            SELECT id, file_id, size as "size: _", storage_path, content_type, generated_at
            FROM file_thumbnails
            WHERE file_id = $1 AND size = $2
            "#,
        )
        .bind(file_id)
        .bind(size.as_str())
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| ThumbnailError::Database(e.to_string()))?;

        Ok(row)
    }

    /// Check if thumbnail generation is supported for a file type
    pub fn is_supported(&self, mime_type: &str) -> bool {
        is_thumbnail_supported(mime_type)
    }

    /// Generate a thumbnail for a file
    pub async fn generate_thumbnail(
        &self,
        file_id: Uuid,
        _mime_type: &str,
        size: ThumbnailSize,
    ) -> Result<FileThumbnail, ThumbnailError> {
        // Check if file exists
        let file_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM files WHERE id = $1)"
        )
        .bind(file_id)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| ThumbnailError::Database(e.to_string()))?;

        if !file_exists {
            return Err(ThumbnailError::NotFound);
        }

        // Get the file content from storage
        let file_path = format!("files/{}", file_id);
        let content = self
            .storage
            .get(&file_path)
            .await
            .map_err(|e| ThumbnailError::Storage(e.to_string()))?;

        // Generate thumbnail based on file type (image only for now)
        let (thumbnail_data, content_type) = self
            .generate_image_thumbnail(&content, size)
            .await?;

        // Store thumbnail
        let thumbnail_path = format!("thumbnails/{}/{}.webp", file_id, size.as_str());
        self.storage
            .put(&thumbnail_path, thumbnail_data.into())
            .await
            .map_err(|e| ThumbnailError::Storage(e.to_string()))?;

        // Save to database
        let thumbnail = sqlx::query_as::<_, FileThumbnail>(
            r#"
            INSERT INTO file_thumbnails (file_id, size, storage_path, content_type)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (file_id, size) DO UPDATE SET
                storage_path = EXCLUDED.storage_path,
                content_type = EXCLUDED.content_type,
                generated_at = NOW()
            RETURNING id, file_id, size as "size: _", storage_path, content_type, generated_at
            "#,
        )
        .bind(file_id)
        .bind(size.as_str())
        .bind(&thumbnail_path)
        .bind(&content_type)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| ThumbnailError::Database(e.to_string()))?;

        Ok(thumbnail)
    }

    /// Generate image thumbnail using the `image` crate
    async fn generate_image_thumbnail(
        &self,
        content: &[u8],
        size: ThumbnailSize,
    ) -> Result<(Vec<u8>, String), ThumbnailError> {
        // Use tokio::task::spawn_blocking for CPU-intensive image processing
        let (width, height) = size.dimensions();

        let content = content.to_vec();
        let result = tokio::task::spawn_blocking(move || {
            use image::imageops::FilterType;

            // Load image
            let img = image::load_from_memory(&content)
                .map_err(|e| ThumbnailError::Generation(format!("Failed to load image: {}", e)))?;

            // Resize maintaining aspect ratio (fit within bounds)
            let resized = img.resize(width, height, FilterType::Lanczos3);

            // Encode as WebP
            let mut output = Vec::new();
            let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut output);
            let rgba = resized.to_rgba8();
            encoder
                .encode(
                    &rgba,
                    resized.width(),
                    resized.height(),
                    image::ColorType::Rgba8,
                )
                .map_err(|e| ThumbnailError::Generation(format!("WebP encode failed: {}", e)))?;

            Ok::<(Vec<u8>, String), ThumbnailError>((output, "image/webp".to_string()))
        })
        .await
        .map_err(|e| ThumbnailError::Generation(format!("Task failed: {}", e)))?;

        result
    }

    /// Delete thumbnails when file is updated
    pub async fn invalidate_thumbnails(&self, file_id: Uuid) -> Result<(), ThumbnailError> {
        // Delete from database
        sqlx::query("DELETE FROM file_thumbnails WHERE file_id = $1")
            .bind(file_id)
            .execute(&self.db_pool)
            .await
            .map_err(|e| ThumbnailError::Database(e.to_string()))?;

        // Delete from storage (best effort - don't fail if storage delete fails)
        for size in ["sm", "md", "lg"] {
            let path = format!("thumbnails/{}/{}.webp", file_id, size);
            let _ = self.storage.delete(&path).await;
        }

        Ok(())
    }

    /// Get thumbnail data from storage
    pub async fn get_thumbnail_data(
        &self,
        storage_path: &str,
    ) -> Result<Vec<u8>, ThumbnailError> {
        self.storage
            .get(storage_path)
            .await
            .map_err(|e| ThumbnailError::Storage(e.to_string()))
            .map(|bytes| bytes.to_vec())
    }
}
