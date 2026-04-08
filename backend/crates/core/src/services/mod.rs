mod ai;
mod ai_service;
mod email_service;
// TODO: Fix chat_integration compilation errors
// mod chat_integration;
mod errors;
mod file_service;
mod folder_ancestry;
mod folder_service;
mod notification_errors;
mod notification_service;
mod permission_resolver;
mod scim_service;
mod scim_v2_service;
// TODO: Fix search_service - needs proper implementation
// mod search_service;
mod share_errors;
mod share_service;
// TODO: Fix sync_service - has circular dependency with rustshare_storage
// mod sync_service;
mod thumbnail_service;
mod upload_service;
pub mod upload_session;
#[allow(deprecated)]
mod user_share_service;

pub use ai::{
    ContentIndex, ContentIndexer, EmbeddingGenerator, IndexedDocument, SimpleEmbeddingGenerator,
};
pub use ai_service::{
    AiError, AiService, FileSummary, QuestionAnswer, SemanticSearchResult, SourceCitation,
};
pub use email_service::{EmailError, EmailService};
pub use errors::{FileError, FolderError};
pub use file_service::{
    EventStoreOps as FileEventStoreOps, FileService, FileUploadActor,
    MetadataStoreOps as FileMetadataStoreOps, ObjectStoreOps,
};
pub use folder_ancestry::{AncestryFolderRepository, FolderAncestryBuilder};
pub use folder_service::{
    EventStoreOps as FolderEventStoreOps, FolderService, MetadataStoreOps as FolderMetadataStoreOps,
};
pub use notification_errors::NotificationError;
pub use notification_service::{
    CreateNotification, NotificationRepositoryOps, NotificationService,
};
// TODO: Fix chat_integration compilation errors
// pub use chat_integration::{
//     ChatEvent, ChatEventPayload, ChatEventType, ChatIntegrationError, ChatIntegrationService,
//     HttpWebhookDispatcher, IncomingChatEvent, MetadataStoreOps as ChatMetadataStoreOps,
//     UnfurlMetadata, UnfurlRequest, UnfurlResponse, WebhookDispatcher,
// };
pub use permission_resolver::{PermissionResolver, PermissionResolverOps, Resource};
pub use scim_service::{
    GroupRecord, ScimAction, ScimEmail, ScimError, ScimGroup, ScimGroupResult, ScimMember,
    ScimName, ScimRepository, ScimService, ScimUser, ScimUserResult,
};
pub use scim_v2_service::{
    parse_filter, FilterOperator, ScimFilter, ScimPatchOperation, ScimPatchRequest, ScimSchemas,
    ScimV2Address, ScimV2AuthScheme, ScimV2BulkConfig, ScimV2Email, ScimV2Error,
    ScimV2ErrorResponse, ScimV2FilterConfig, ScimV2Group, ScimV2GroupRecord, ScimV2GroupRef,
    ScimV2ListResponse, ScimV2Member, ScimV2Meta, ScimV2Name, ScimV2PhoneNumber, ScimV2Repository,
    ScimV2ResourceType, ScimV2Schema, ScimV2SchemaAttribute, ScimV2SchemaExtension, ScimV2Service,
    ScimV2ServiceProviderConfig, ScimV2SupportConfig, ScimV2User, ScimV2UserRecord,
};
// TODO: Fix search_service - needs proper implementation
// pub use search_service::{SearchIndexRepository, SearchResult, SearchResultItem, SearchService};
pub use share_errors::ShareError;
pub use share_service::{
    EventStoreOps as ShareEventStoreOps, JwtOps, MetadataStoreOps as ShareMetadataStoreOps,
    ShareNotificationRepo, ShareService,
};
// pub use sync_service::{
//     ConflictInfo, ConflictResolution, DeviceSyncInfo, SyncError, SyncService, SyncServiceOps,
// };
pub use thumbnail_service::{ThumbnailError, ThumbnailService};
pub use upload_service::{
    UploadError, UploadMetadataStore, UploadObjectStore, UploadService, UploadSessionRepository,
};
pub use upload_session::{
    ChunkInfo, ChunkUploadResponse, CompleteUploadResponse, CreateSessionRequest,
    CreateSessionResponse, SessionStatusResponse, UploadSession, UploadSessionStatus,
};
#[allow(deprecated)]
pub use user_share_service::{
    FileOps, FolderOps, ShareOps, UserOps, UserShareService, UserShareServiceDeps,
};
