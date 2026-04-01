//! Thumbnail domain types

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Thumbnail size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ThumbnailSize {
    /// 40x40px
    Sm,
    /// 128x128px
    Md,
    /// 256x256px
    Lg,
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

impl std::fmt::Display for ThumbnailSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ThumbnailSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sm" => Ok(ThumbnailSize::Sm),
            "md" => Ok(ThumbnailSize::Md),
            "lg" => Ok(ThumbnailSize::Lg),
            _ => Err(format!("Invalid thumbnail size: {}", s)),
        }
    }
}

impl TryFrom<&str> for ThumbnailSize {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
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

/// Special diagram file types (identified by extension)
pub const SUPPORTED_DIAGRAM_EXTENSIONS: &[&str] = &[
    ".excalidraw",
    ".excalidraw.json",
    ".drawio",
    ".dio",
];

/// Check if a MIME type is supported for thumbnail generation
pub fn is_thumbnail_supported(mime_type: &str) -> bool {
    let mime_lower = mime_type.to_lowercase();
    SUPPORTED_IMAGE_TYPES.contains(&mime_lower.as_str())
        || SUPPORTED_PDF_TYPES.contains(&mime_lower.as_str())
        || SUPPORTED_VIDEO_TYPES.contains(&mime_lower.as_str())
}

/// Check if a file supports thumbnail generation (by MIME type and filename)
pub fn is_file_thumbnail_supported(mime_type: &str, file_name: &str) -> bool {
    // First check MIME type
    if is_thumbnail_supported(mime_type) {
        return true;
    }
    
    // Check special diagram file extensions
    let name_lower = file_name.to_lowercase();
    for ext in SUPPORTED_DIAGRAM_EXTENSIONS {
        if name_lower.ends_with(ext) {
            return true;
        }
    }
    
    false
}

/// Get the category of thumbnail generation for a MIME type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailCategory {
    Image,
    Pdf,
    Video,
    Diagram, // Excalidraw, Draw.io, etc.
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

/// Get thumbnail category considering both MIME type and filename
pub fn get_file_thumbnail_category(mime_type: &str, file_name: &str) -> ThumbnailCategory {
    // First try MIME type based detection
    let category = get_thumbnail_category(mime_type);
    if category != ThumbnailCategory::Unsupported {
        return category;
    }
    
    // Check for diagram files by extension
    let name_lower = file_name.to_lowercase();
    for ext in SUPPORTED_DIAGRAM_EXTENSIONS {
        if name_lower.ends_with(ext) {
            return ThumbnailCategory::Diagram;
        }
    }
    
    ThumbnailCategory::Unsupported
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumbnail_size_from_str_valid() {
        assert_eq!("sm".parse::<ThumbnailSize>().unwrap(), ThumbnailSize::Sm);
        assert_eq!("md".parse::<ThumbnailSize>().unwrap(), ThumbnailSize::Md);
        assert_eq!("lg".parse::<ThumbnailSize>().unwrap(), ThumbnailSize::Lg);
    }

    #[test]
    fn test_thumbnail_size_from_str_case_insensitive() {
        assert_eq!("SM".parse::<ThumbnailSize>().unwrap(), ThumbnailSize::Sm);
        assert_eq!("Md".parse::<ThumbnailSize>().unwrap(), ThumbnailSize::Md);
        assert_eq!("lG".parse::<ThumbnailSize>().unwrap(), ThumbnailSize::Lg);
    }

    #[test]
    fn test_thumbnail_size_from_str_invalid() {
        let result: Result<ThumbnailSize, _> = "invalid".parse();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid thumbnail size: invalid");
    }

    #[test]
    fn test_thumbnail_size_try_from() {
        assert_eq!(ThumbnailSize::try_from("sm").unwrap(), ThumbnailSize::Sm);
        assert_eq!(ThumbnailSize::try_from("md").unwrap(), ThumbnailSize::Md);
        assert_eq!(ThumbnailSize::try_from("lg").unwrap(), ThumbnailSize::Lg);
        assert!(ThumbnailSize::try_from("invalid").is_err());
    }

    #[test]
    fn test_thumbnail_size_as_str() {
        assert_eq!(ThumbnailSize::Sm.as_str(), "sm");
        assert_eq!(ThumbnailSize::Md.as_str(), "md");
        assert_eq!(ThumbnailSize::Lg.as_str(), "lg");
    }

    #[test]
    fn test_thumbnail_size_display() {
        assert_eq!(format!("{}", ThumbnailSize::Sm), "sm");
        assert_eq!(format!("{}", ThumbnailSize::Md), "md");
        assert_eq!(format!("{}", ThumbnailSize::Lg), "lg");
    }

    #[test]
    fn test_thumbnail_size_dimensions() {
        assert_eq!(ThumbnailSize::Sm.dimensions(), (40, 40));
        assert_eq!(ThumbnailSize::Md.dimensions(), (128, 128));
        assert_eq!(ThumbnailSize::Lg.dimensions(), (256, 256));
    }

    #[test]
    fn test_is_thumbnail_supported_image() {
        assert!(is_thumbnail_supported("image/jpeg"));
        assert!(is_thumbnail_supported("image/png"));
        assert!(is_thumbnail_supported("image/gif"));
        assert!(is_thumbnail_supported("image/webp"));
        assert!(is_thumbnail_supported("image/bmp"));
    }

    #[test]
    fn test_is_thumbnail_supported_pdf() {
        assert!(is_thumbnail_supported("application/pdf"));
    }

    #[test]
    fn test_is_thumbnail_supported_video() {
        assert!(is_thumbnail_supported("video/mp4"));
        assert!(is_thumbnail_supported("video/quicktime"));
        assert!(is_thumbnail_supported("video/webm"));
    }

    #[test]
    fn test_is_thumbnail_supported_unsupported() {
        assert!(!is_thumbnail_supported("text/plain"));
        assert!(!is_thumbnail_supported("application/json"));
        assert!(!is_thumbnail_supported("audio/mpeg"));
    }

    #[test]
    fn test_is_thumbnail_supported_case_insensitive() {
        assert!(is_thumbnail_supported("IMAGE/JPEG"));
        assert!(is_thumbnail_supported("Image/Png"));
        assert!(is_thumbnail_supported("APPLICATION/PDF"));
    }

    #[test]
    fn test_get_thumbnail_category() {
        assert_eq!(get_thumbnail_category("image/jpeg"), ThumbnailCategory::Image);
        assert_eq!(get_thumbnail_category("application/pdf"), ThumbnailCategory::Pdf);
        assert_eq!(get_thumbnail_category("video/mp4"), ThumbnailCategory::Video);
        assert_eq!(get_thumbnail_category("text/plain"), ThumbnailCategory::Unsupported);
    }
}
