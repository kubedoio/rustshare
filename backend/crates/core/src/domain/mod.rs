mod file;
mod file_version;
mod folder;
mod share;
mod user;

pub use file::File;
pub use file_version::FileVersion;
pub use folder::Folder;
pub use share::{Share, SharePermissions};
pub use user::User;

use uuid::Uuid;

// Type aliases for better type safety
pub type UserId = Uuid;
pub type FileId = Uuid;
pub type FolderId = Uuid;
pub type ShareId = Uuid;
pub type VersionId = Uuid;
