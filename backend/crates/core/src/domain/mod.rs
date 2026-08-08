mod application;
mod application_config;
mod application_user_preference;
mod device_token;
mod file;
mod file_version;
mod folder;
mod mail_link;
mod mail_message;
mod notification;
mod oidc_login_state;
mod replication;
mod response_types;
mod share;
mod template;
mod tenant_config;
mod thumbnail;
mod user;
mod user_session;
mod vault;

pub mod mail_account;
pub use mail_account::*;

pub use device_token::{DevicePairRequest, DeviceToken};

pub use application::*;
pub use application_config::{
    AiIndexingPolicy, ApplicationConfig, ApplicationPermissions, AuditPolicy,
};
pub use application_user_preference::ApplicationUserPreference;
pub use file::File;
pub use file_version::{FileVersion, ReplicationState};
pub use folder::Folder;
pub use mail_link::{LinkTargetType, MailLink, MailLinkId};
pub use mail_message::{
    MailAttachment, MailMessage, MailMessagePart, MailSortOrder, MailSourceMode, MailVisibility,
};
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
pub use user_session::UserSession;
pub use vault::{
    CreateVaultRequest, DeleteVaultFileRequest, RenameVaultFileRequest,
    SaveVaultFileContentRequest, UpdateVaultWritePolicyRequest, UploadVaultFileRequest, Vault,
    VaultAdapter, VaultDevice, VaultFile, VaultFileContentResponse, VaultFileContentSavedResponse,
    VaultManifest, VaultManifestEntry, VaultManifestResult, VaultWritePolicy,
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
/// Unique identifier for a mail account.
pub type MailAccountId = Uuid;
/// Unique identifier for a mail import job.
pub type MailImportJobId = Uuid;
