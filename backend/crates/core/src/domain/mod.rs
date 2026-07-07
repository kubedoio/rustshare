mod device_token;
mod file;
mod file_version;
mod folder;
mod mail_message;
mod module;
mod notification;
mod oidc_login_state;
mod replication;
mod response_types;
mod share;
mod template;
mod tenant_config;
mod thumbnail;
mod user;
mod user_module_preference;
mod user_session;
mod vault;

pub use device_token::{DevicePairRequest, DeviceToken};

pub use file::File;
pub use file_version::{FileVersion, ReplicationState};
pub use folder::Folder;
pub use mail_message::{
    MailAttachment, MailMessage, MailMessagePart, MailSourceMode, MailVisibility,
};
pub use module::{AiIndexingPolicy, AuditPolicy, Module, ModulePermissions};
pub use notification::{Notification, NotificationId, NotificationType, ResourceType};
pub use oidc_login_state::OidcLoginState;
pub use replication::{
    ReplicationJob, ReplicationJobId, ReplicationJobStatus, ReplicationTarget, ReplicationTargetId,
};
pub use response_types::{FolderContents, FolderTree};
pub use share::{Share, SharePermissions, ShareRecipient, ShareType};
pub use template::{CreateFromTemplateRequest, CreatedObject, Template, TemplateDefaultFile};
pub use tenant_config::RecipientVisibility;
pub use thumbnail::{
    get_file_thumbnail_category, get_thumbnail_category, is_file_thumbnail_supported,
    is_thumbnail_supported, FileThumbnail, ThumbnailCategory, ThumbnailSize,
    SUPPORTED_DIAGRAM_EXTENSIONS, SUPPORTED_IMAGE_TYPES, SUPPORTED_PDF_TYPES,
    SUPPORTED_VIDEO_TYPES,
};
pub use user::{DashboardConfig, Theme, User};
pub use user_module_preference::UserModulePreference;
pub use user_session::UserSession;
pub use vault::{
    CreateVaultRequest, DeleteVaultFileRequest, RenameVaultFileRequest, UploadVaultFileRequest,
    Vault, VaultAdapter, VaultDevice, VaultFile, VaultManifest, VaultManifestEntry,
    VaultManifestResult,
};

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
/// Unique identifier for a vault.
pub type VaultId = Uuid;
/// Unique identifier for a mail message.
pub type MailMessageId = Uuid;
/// Unique identifier for a mail message part.
pub type MailMessagePartId = Uuid;
/// Unique identifier for a mail attachment.
pub type MailAttachmentId = Uuid;
