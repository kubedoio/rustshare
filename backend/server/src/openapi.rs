//! Auto-generated OpenAPI documentation for the RustShare REST API.
//!
//! This module uses `utoipa` to generate the OpenAPI spec at compile time
//! directly from handler functions and request/response types. The spec is:
//!
//! - Served at runtime as `/api/docs/openapi.json`
//! - Rendered interactively at `/api/docs` via Swagger UI
//! - Exported to `docs/contracts/rustshare-api-openapi.json` by the
//!   `openapi_export_test` integration test so external consumers can read
//!   a static, committed copy without running the server.

use std::path::Path;

use utoipa::OpenApi;

/// The generated RustShare REST API OpenAPI document.
///
/// This struct is populated by utoipa's derive macro at compile time. Keep the
/// `paths` and `components(schemas)` lists in sync with newly annotated
/// handlers and domain types.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "RustShare REST API",
        version = "1.0.0",
        description = "Auto-generated OpenAPI specification for the RustShare file-sharing and sync platform.",
    ),
    servers(
        (url = "/", description = "Current server")
    ),
    paths(
        // Admin webhooks
        crate::handlers::admin::webhooks::list_webhooks,
        crate::handlers::admin::webhooks::create_webhook,
        crate::handlers::admin::webhooks::update_webhook,
        crate::handlers::admin::webhooks::delete_webhook,
        crate::handlers::admin::webhooks::test_webhook,

        // Chat / RustChat integration
        crate::handlers::chat_integration::unfurl_link,
        crate::handlers::chat_integration::unfurl_link_public,
        crate::handlers::chat_integration::receive_chat_event,
        crate::handlers::chat_integration::dispatch_webhooks,
        crate::handlers::chat_integration::register_chat_webhook,
        crate::handlers::chat_integration::list_chat_webhooks,

        // Files
        crate::handlers::files::upload_file,
        crate::handlers::files::get_file,
        crate::handlers::files::download_file,
        crate::handlers::files::delete_file,

        // Notes
        crate::handlers::notes::create_note,
        crate::handlers::notes::list_notes,
        crate::handlers::notes::get_note,
        crate::handlers::notes::save_note,
        crate::handlers::notes::delete_note,

        // AI
        crate::handlers::ai::semantic_search,
        crate::handlers::ai::summarize_file,
        crate::handlers::ai::ask_question,

        // Auth
        crate::handlers::auth::login,
        crate::handlers::auth::logout,
        crate::oidc::auth_config,

        // Folders
        crate::handlers::folders::create_folder,
        crate::handlers::folders::get_folder,
        crate::handlers::folders::get_folder_tree,
        crate::handlers::folders::get_folder_contents,
        crate::handlers::folders::get_root_contents,

        // Public shares
        crate::handlers::public_shares::get_share_info,
        crate::handlers::public_shares::create_session,
    ),
    components(schemas(
        // Common
        crate::handlers::ErrorResponse,

        // Admin webhooks
        crate::handlers::admin::webhooks::WebhookResponse,
        crate::handlers::admin::webhooks::CreateWebhookRequest,
        crate::handlers::admin::webhooks::UpdateWebhookRequest,

        // Chat integration
        crate::handlers::chat_integration::UnfurlLinkRequest,
        crate::handlers::chat_integration::UnfurlLinkResponse,
        crate::handlers::chat_integration::ReceiveChatEventRequest,
        crate::handlers::chat_integration::DispatchWebhookRequest,
        crate::handlers::chat_integration::DispatchWebhookResponse,
        crate::handlers::chat_integration::WebhookDispatchResult,
        crate::handlers::chat_integration::RegisterWebhookRequest,
        crate::handlers::chat_integration::WebhookListResponse,

        // Files
        crate::handlers::files::FileUploadResponse,
        crate::handlers::files::DownloadUrlResponse,
        crate::handlers::files::FileWithShares,

        // Notes
        crate::handlers::notes::CreateNoteRequest,
        crate::handlers::notes::CreateNoteResponse,
        crate::handlers::notes::GetNoteResponse,
        crate::handlers::notes::SaveNoteRequest,
        crate::handlers::notes::SaveNoteResponse,
        crate::handlers::notes::RenameNoteRequest,
        crate::handlers::notes::MoveNoteRequest,
        crate::handlers::notes::ListNotesQuery,
        crate::handlers::notes::RecentNotesResponse,
        crate::handlers::notes::VisibilityResponse,
        crate::handlers::notes::DuplicateNoteResponse,
        crate::handlers::notes::PublicNoteResponse,
        crate::services::note_service::NoteAttachment,
        crate::services::note_service::NoteMetadata,
        crate::services::note_service::NoteVisibility,
        crate::services::note_service::NoteSummary,

        // AI
        crate::handlers::ai::SemanticSearchRequest,
        crate::handlers::ai::SemanticSearchResultItem,
        crate::handlers::ai::SemanticSearchResponse,
        crate::handlers::ai::SummarizeRequest,
        crate::handlers::ai::SummarizeResponse,
        crate::handlers::ai::SourceCitation,
        crate::handlers::ai::AskQuestionRequest,
        crate::handlers::ai::AskQuestionResponse,

        // Auth
        crate::handlers::auth::LoginRequest,
        crate::handlers::auth::LoginResponse,
        crate::handlers::auth::UserResponse,
        crate::oidc::AuthConfigResponse,

        // Folders
        crate::handlers::folders::CreateFolderRequest,
        crate::handlers::folders::FolderWithShares,
        crate::handlers::folders::FolderContentsWithShares,
        crate::handlers::folders::FolderTreeNode,
        crate::handlers::folders::FolderTreeWithShares,

        // Public shares
        crate::handlers::public_shares::CreateSessionRequest,
        crate::handlers::public_shares::SessionResponse,
        crate::handlers::public_shares::ShareInfoResponse,
        crate::handlers::public_shares::SharedFolderContentsResponse,
        crate::handlers::public_shares::SharedFolderContentsQuery,

        // Core domain types
        rustshare_core::domain::File,
        rustshare_core::domain::Folder,
        rustshare_core::domain::FolderContents,
        rustshare_core::domain::FolderTree,
        rustshare_core::domain::Share,
        rustshare_core::domain::SharePermissions,
        rustshare_core::domain::ShareType,
        rustshare_core::domain::ShareRecipient,
        rustshare_core::domain::User,
        rustshare_core::domain::Theme,
        rustshare_core::domain::DashboardConfig,
        rustshare_core::domain::Vault,
        rustshare_core::domain::VaultAdapter,
        rustshare_core::domain::VaultFile,
        rustshare_core::domain::VaultDevice,
        rustshare_core::domain::CreateVaultRequest,
        rustshare_core::domain::DeleteVaultFileRequest,
        rustshare_core::domain::RenameVaultFileRequest,
        rustshare_core::domain::VaultManifest,
        rustshare_core::domain::VaultManifestEntry,
        rustshare_core::domain::FileVersion,
        rustshare_core::domain::ReplicationState,
        rustshare_core::domain::Notification,
        rustshare_core::domain::NotificationType,
        rustshare_core::domain::ResourceType,
        rustshare_core::domain::Module,
        rustshare_core::domain::ModulePermissions,
        rustshare_core::domain::AiIndexingPolicy,
        rustshare_core::domain::AuditPolicy,
        rustshare_core::domain::Template,
        rustshare_core::domain::TemplateDefaultFile,
        rustshare_core::domain::CreateFromTemplateRequest,
        rustshare_core::domain::CreatedObject,
        rustshare_core::domain::ReplicationTarget,
        rustshare_core::domain::ReplicationJobStatus,
        rustshare_core::domain::ReplicationJob,
        rustshare_core::domain::UserSession,
        rustshare_core::domain::UserModulePreference,

        // Event payloads (used in webhook documentation)
        rustshare_core::events::AggregateType,
        rustshare_core::events::EventType,
        rustshare_core::events::Event,
        rustshare_core::events::FileUploadedPayload,
        rustshare_core::events::FileModifiedPayload,
        rustshare_core::events::FileDeletedPayload,
        rustshare_core::events::FileRestoredPayload,
        rustshare_core::events::FolderCreatedPayload,
        rustshare_core::events::FolderRenamedPayload,
        rustshare_core::events::FolderMovedPayload,
        rustshare_core::events::FolderDeletedPayload,
        rustshare_core::events::ShareCreatedPayload,
        rustshare_core::events::ShareRevokedPayload,
        rustshare_core::events::ShareUpdatedPayload,
        rustshare_core::events::ShareReceivedByUserPayload,
        rustshare_core::events::SharePermissionChangedPayload,
        rustshare_core::events::ShareRevokedFromUserPayload,
        rustshare_core::events::NotificationCreatedPayload,
        rustshare_core::events::ReplicationStateChangedPayload,
        rustshare_core::events::BrainstormBoardModifiedPayload,
        rustshare_core::events::MeetingNoteModifiedPayload,
        rustshare_core::events::DecisionModifiedPayload,
        rustshare_core::events::StandupModifiedPayload,
        rustshare_core::events::KanbanModifiedPayload,
        rustshare_core::events::NoteModifiedPayload,
    )),
    tags(
        (name = "Admin", description = "Administration endpoints"),
        (name = "Webhooks", description = "Outbound webhook management"),
        (name = "Chat Integration", description = "RustChat and external chat integration"),
        (name = "Files", description = "File operations"),
        (name = "Notes", description = "Note operations"),
        (name = "AI", description = "AI-powered search and summarization"),
        (name = "Auth", description = "Authentication"),
        (name = "Folders", description = "Folder operations"),
        (name = "Public Shares", description = "Public share access"),
    )
)]
pub struct ApiDoc;

/// Write the generated OpenAPI JSON to `path`.
///
/// Called by the export test to keep `docs/contracts/rustshare-api-openapi.json`
/// in sync with the codebase. Panics only on I/O errors; it always overwrites the
/// destination file.
pub fn export_to_file(path: &Path) -> std::io::Result<()> {
    let spec = ApiDoc::openapi().to_pretty_json()?;
    std::fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))?;
    std::fs::write(path, spec)?;
    Ok(())
}

/// Return the generated OpenAPI JSON as a string.
pub fn to_pretty_json() -> Result<String, serde_json::Error> {
    ApiDoc::openapi().to_pretty_json()
}

/// Axum handler that serves the generated OpenAPI JSON document.
pub async fn openapi_json_handler() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
