mod device_token;
mod file;
mod file_version;
mod folder;
mod notification;
mod oidc_login_state;
mod replication;
mod response_types;
mod share;
mod thumbnail;
mod user;
mod user_session;

pub use device_token::{DevicePairRequest, DeviceToken};

pub use file::File;
pub use file_version::{FileVersion, ReplicationState};
pub use folder::Folder;
pub use notification::{Notification, NotificationId, NotificationType, ResourceType};
pub use oidc_login_state::OidcLoginState;
pub use replication::{
    ReplicationJob, ReplicationJobId, ReplicationJobStatus, ReplicationTarget, ReplicationTargetId,
};
pub use response_types::{FolderContents, FolderTree};
pub use share::{Share, SharePermissions, ShareRecipient};
pub use thumbnail::{
    FileThumbnail, ThumbnailCategory, ThumbnailSize, 
    SUPPORTED_DIAGRAM_EXTENSIONS, SUPPORTED_IMAGE_TYPES, SUPPORTED_PDF_TYPES, SUPPORTED_VIDEO_TYPES,
    get_file_thumbnail_category, get_thumbnail_category, is_file_thumbnail_supported, is_thumbnail_supported
};
pub use user::{Theme, User};
pub use user_session::UserSession;

use uuid::Uuid;

// Type aliases for better type safety
/// Unique identifier for a user.
pub type UserId = Uuid;
/// Unique identifier for a file.
pub type FileId = Uuid;
/// Unique identifier for a folder.
pub type FolderId = Uuid;
/// Unique identifier for a share link.
pub type ShareId = Uuid;
/// Unique identifier for a file version.
pub type VersionId = Uuid;
