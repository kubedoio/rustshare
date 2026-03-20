mod file;
mod file_version;
mod folder;
mod notification;
mod oidc_login_state;
mod replication;
mod response_types;
mod share;
mod user;
mod user_session;

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
