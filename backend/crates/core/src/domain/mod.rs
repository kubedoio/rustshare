mod file;
mod file_version;
mod folder;
mod response_types;
mod share;
mod user;

pub use file::File;
pub use file_version::FileVersion;
pub use folder::Folder;
pub use response_types::{FolderContents, FolderTree};
pub use share::{Share, SharePermissions, ShareRecipient};
pub use user::User;

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
