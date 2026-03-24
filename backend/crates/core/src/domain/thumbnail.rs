//! Thumbnail domain types

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Thumbnail size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThumbnailSize {
    Sm, // 40x40px
    Md, // 128x128px
    Lg, // 256x256px
}

impl ThumbnailSize {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThumbnailSize::Sm => "sm",
            ThumbnailSize::Md => "md",
            ThumbnailSize::Lg => "lg",
        }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            ThumbnailSize::Sm => (40, 40),
            ThumbnailSize::Md => (128, 128),
            ThumbnailSize::Lg => (256, 256),
        }
    }
}

impl TryFrom<&str> for ThumbnailSize {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "sm" => Ok(ThumbnailSize::Sm),
            "md" => Ok(ThumbnailSize::Md),
            "lg" => Ok(ThumbnailSize::Lg),
            _ => Err(format!("Invalid thumbnail size: {}", value)),
        }
    }
}

/// File thumbnail record
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FileThumbnail {
    pub id: Uuid,
    pub file_id: Uuid,
    pub size: ThumbnailSize,
    pub storage_path: String,
    pub content_type: String,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

/// Supported MIME type categories for thumbnail generation
pub const SUPPORTED_IMAGE_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/bmp",
];

pub const SUPPORTED_PDF_TYPES: &[&str] = &["application/pdf"];

pub const SUPPORTED_VIDEO_TYPES: &[&str] = &[
    "video/mp4",
    "video/quicktime", // .mov
    "video/webm",
];

/// Check if a MIME type is supported for thumbnail generation
pub fn is_thumbnail_supported(mime_type: &str) -> bool {
    let mime_lower = mime_type.to_lowercase();
    SUPPORTED_IMAGE_TYPES.contains(&mime_lower.as_str())
        || SUPPORTED_PDF_TYPES.contains(&mime_lower.as_str())
        || SUPPORTED_VIDEO_TYPES.contains(&mime_lower.as_str())
}

/// Get the category of thumbnail generation for a MIME type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailCategory {
    Image,
    Pdf,
    Video,
    Unsupported,
}

pub fn get_thumbnail_category(mime_type: &str) -> ThumbnailCategory {
    let mime_lower = mime_type.to_lowercase();
    if SUPPORTED_IMAGE_TYPES.contains(&mime_lower.as_str()) {
        ThumbnailCategory::Image
    } else if SUPPORTED_PDF_TYPES.contains(&mime_lower.as_str()) {
        ThumbnailCategory::Pdf
    } else if SUPPORTED_VIDEO_TYPES.contains(&mime_lower.as_str()) {
        ThumbnailCategory::Video
    } else {
        ThumbnailCategory::Unsupported
    }
}
