//! Thumbnail generation service

use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;

use crate::domain::{
    FileThumbnail, ThumbnailSize, ThumbnailCategory, get_file_thumbnail_category, is_file_thumbnail_supported,
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
            SELECT id, file_id, size, storage_path, content_type, generated_at
            FROM file_thumbnails
            WHERE file_id = $1 AND size = $2
            "#,
        )
        .bind(file_id)
        .bind(size)
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| ThumbnailError::Database(e.to_string()))?;

        Ok(row)
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
        let file_row = sqlx::query_as::<_, (String, i64, String)>(
            "SELECT name, size, content_hash FROM files WHERE id = $1"
        )
        .bind(file_id)
        .fetch_optional(&self.db_pool)
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
        let file_name = if file_name.is_empty() { &db_file_name } else { file_name };

        if !is_file_thumbnail_supported(mime_type, file_name) {
            tracing::warn!(mime_type = mime_type, file_name = file_name, "Unsupported file type for thumbnail");
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
        
        tracing::debug!(content_size = content.len(), "File content fetched successfully");

        // Generate thumbnail based on file type
        let category = get_file_thumbnail_category(mime_type, file_name);
        tracing::debug!(category = ?category, "Thumbnail category determined");
        
        let (thumbnail_data, content_type) = match category {
            ThumbnailCategory::Image => {
                self.generate_image_thumbnail(&content, size).await?
            }
            ThumbnailCategory::Pdf => {
                self.generate_pdf_thumbnail(&content, size).await?
            }
            ThumbnailCategory::Video => {
                self.generate_video_thumbnail(&content, size).await?
            }
            ThumbnailCategory::Diagram => {
                self.generate_diagram_thumbnail(&content, file_name, size).await?
            }
            ThumbnailCategory::Unsupported => {
                return Err(ThumbnailError::UnsupportedType);
            }
        };

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
        let thumbnail = sqlx::query_as::<_, FileThumbnail>(
            r#"
            INSERT INTO file_thumbnails (file_id, size, storage_path, content_type)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (file_id, size) DO UPDATE SET
                storage_path = EXCLUDED.storage_path,
                content_type = EXCLUDED.content_type,
                generated_at = NOW()
            RETURNING id, file_id, size, storage_path, content_type, generated_at
            "#,
        )
        .bind(file_id)
        .bind(size)
        .bind(&thumbnail_path)
        .bind(&content_type)
        .fetch_one(&self.db_pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Database error saving thumbnail metadata");
            ThumbnailError::Database(e.to_string())
        })?;

        tracing::info!(file_id = %file_id, size = size.as_str(), "Thumbnail generation completed successfully");
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
        let content_len = content.len();

        let content = content.to_vec();
        let result = tokio::task::spawn_blocking(move || {
            use image::imageops::FilterType;

            tracing::debug!(content_len = content_len, width = width, height = height, "Starting image thumbnail generation");

            // Load image
            let img = image::load_from_memory(&content)
                .map_err(|e| {
                    tracing::error!(error = %e, "Failed to load image from memory");
                    ThumbnailError::Generation(format!("Failed to load image: {}", e))
                })?;

            tracing::debug!(original_width = img.width(), original_height = img.height(), "Image loaded successfully");

            // Resize maintaining aspect ratio (fit within bounds)
            let resized = img.resize(width, height, FilterType::Lanczos3);

            tracing::debug!(resized_width = resized.width(), resized_height = resized.height(), "Image resized");

            // Encode as WebP
            let mut output = Vec::new();
            let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut output);
            let rgba = resized.to_rgba8();
            
            tracing::debug!(rgba_len = rgba.len(), "Encoding WebP");
            
            encoder
                .encode(
                    &rgba,
                    resized.width(),
                    resized.height(),
                    image::ColorType::Rgba8,
                )
                .map_err(|e| {
                    tracing::error!(error = %e, "WebP encoding failed");
                    ThumbnailError::Generation(format!("WebP encode failed: {}", e))
                })?;

            tracing::debug!(output_len = output.len(), "WebP encoding successful");

            Ok::<(Vec<u8>, String), ThumbnailError>((output, "image/webp".to_string()))
        })
        .await;

        match result {
            Ok(Ok(data)) => Ok(data),
            Ok(Err(e)) => Err(e),
            Err(join_err) => {
                tracing::error!(error = %join_err, "Thumbnail generation task panicked or was cancelled");
                Err(ThumbnailError::Generation(format!("Task failed: {}", join_err)))
            }
        }
    }

    /// Generate PDF thumbnail by rendering first page
    async fn generate_pdf_thumbnail(
        &self,
        content: &[u8],
        size: ThumbnailSize,
    ) -> Result<(Vec<u8>, String), ThumbnailError> {
        let (width, height) = size.dimensions();
        let _content = content.to_vec();

        let result = tokio::task::spawn_blocking(move || {
            // Try to render PDF using pdf2image or similar
            // For now, generate a PDF icon placeholder with page count indicator
            // This is a simplified version - in production you'd use a PDF rendering library
            
            // Create a PDF document icon
            let mut img = image::RgbaImage::new(width, height);
            
            // Fill with PDF red color
            let pdf_red = image::Rgba([220, 53, 69, 255]);
            for pixel in img.pixels_mut() {
                *pixel = pdf_red;
            }
            
            // Add white PDF text/icon indicator
            // This is a simplified placeholder - real implementation would render actual PDF page
            
            // Encode as WebP
            let mut output = Vec::new();
            let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut output);
            encoder
                .encode(&img, width, height, image::ColorType::Rgba8)
                .map_err(|e| ThumbnailError::Generation(format!("WebP encode failed: {}", e)))?;

            Ok::<(Vec<u8>, String), ThumbnailError>((output, "image/webp".to_string()))
        })
        .await
        .map_err(|e| ThumbnailError::Generation(format!("PDF thumbnail task failed: {}", e)))?;

        result
    }

    /// Generate video thumbnail by extracting a frame
    async fn generate_video_thumbnail(
        &self,
        _content: &[u8],
        size: ThumbnailSize,
    ) -> Result<(Vec<u8>, String), ThumbnailError> {
        let (width, height) = size.dimensions();

        let result = tokio::task::spawn_blocking(move || {
            // Create a video play button placeholder
            // Real implementation would use ffmpeg or similar to extract a frame
            let mut img = image::RgbaImage::new(width, height);
            
            // Fill with dark background
            let dark_bg = image::Rgba([30, 30, 30, 255]);
            for pixel in img.pixels_mut() {
                *pixel = dark_bg;
            }
            
            // Add play button triangle (simplified)
            let play_color = image::Rgba([255, 255, 255, 200]);
            let center_x = width / 2;
            let center_y = height / 2;
            let triangle_size = width.min(height) / 4;
            
            // Simple triangle drawing
            for y in center_y.saturating_sub(triangle_size/2)..=center_y.saturating_add(triangle_size/2) {
                for x in center_x..=center_x.saturating_add(triangle_size) {
                    if x < width && y < height {
                        let dy = y as i32 - center_y as i32;
                        let dx = x as i32 - center_x as i32;
                        if dx.abs() <= triangle_size as i32 && dy.abs() <= dx.abs() {
                            img.put_pixel(x, y, play_color);
                        }
                    }
                }
            }
            
            // Encode as WebP
            let mut output = Vec::new();
            let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut output);
            encoder
                .encode(&img, width, height, image::ColorType::Rgba8)
                .map_err(|e| ThumbnailError::Generation(format!("WebP encode failed: {}", e)))?;

            Ok::<(Vec<u8>, String), ThumbnailError>((output, "image/webp".to_string()))
        })
        .await
        .map_err(|e| ThumbnailError::Generation(format!("Video thumbnail task failed: {}", e)))?;

        result
    }

    /// Generate diagram thumbnail (Excalidraw, Draw.io)
    async fn generate_diagram_thumbnail(
        &self,
        _content: &[u8],
        file_name: &str,
        size: ThumbnailSize,
    ) -> Result<(Vec<u8>, String), ThumbnailError> {
        let (width, height) = size.dimensions();
        let file_name = file_name.to_lowercase();

        let result = tokio::task::spawn_blocking(move || {
            let mut img = image::RgbaImage::new(width, height);
            
            // Determine diagram type and color scheme
            let (bg_color, icon_color, _label) = if file_name.ends_with(".excalidraw") || file_name.ends_with(".excalidraw.json") {
                // Excalidraw: light beige background
                (image::Rgba([255, 250, 240, 255]), image::Rgba([105, 67, 53, 255]), "✏️")
            } else if file_name.ends_with(".drawio") || file_name.ends_with(".dio") {
                // Draw.io: blue background
                (image::Rgba([240, 248, 255, 255]), image::Rgba([46, 125, 247, 255]), "📐")
            } else {
                // Generic diagram
                (image::Rgba([245, 245, 245, 255]), image::Rgba([100, 100, 100, 255]), "📊")
            };

            // Fill background
            for pixel in img.pixels_mut() {
                *pixel = bg_color;
            }

            // Add border
            let border_color = icon_color;
            for x in 0..width {
                img.put_pixel(x, 0, border_color);
                img.put_pixel(x, height - 1, border_color);
            }
            for y in 0..height {
                img.put_pixel(0, y, border_color);
                img.put_pixel(width - 1, y, border_color);
            }

            // Note: In a real implementation, we would parse the JSON content
            // and render a simplified preview of the diagram elements
            // For now, we create a styled placeholder with an icon

            // Encode as WebP
            let mut output = Vec::new();
            let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut output);
            encoder
                .encode(&img, width, height, image::ColorType::Rgba8)
                .map_err(|e| ThumbnailError::Generation(format!("WebP encode failed: {}", e)))?;

            Ok::<(Vec<u8>, String), ThumbnailError>((output, "image/webp".to_string()))
        })
        .await
        .map_err(|e| ThumbnailError::Generation(format!("Diagram thumbnail task failed: {}", e)))?;

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
        for size in [ThumbnailSize::Sm, ThumbnailSize::Md, ThumbnailSize::Lg] {
            let path = format!("thumbnails/{}/{}.webp", file_id, size.as_str());
            if let Err(e) = self.storage.delete(&path).await {
                tracing::debug!(path = %path, error = %e, "failed to delete thumbnail from storage");
            }
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
