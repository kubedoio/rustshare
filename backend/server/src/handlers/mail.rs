use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use rustshare_core::domain::{LinkTargetType, MailTlsMode};
use rustshare_core::services::{EmailError, EmailService, OutboundEmail};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::handlers::{AppError, AuthenticatedUser, ValidatedJson};
use crate::services::module_service::ModuleError;
use crate::state::AppState;

const MAX_MAIL_UPLOAD_SIZE_BYTES: usize = 25 * 1024 * 1024;
const MAX_MAIL_SEND_RECIPIENTS: usize = 50;
const MAX_MAIL_SEND_BODY_BYTES: usize = 256 * 1024;
const MAX_MAIL_SEND_ATTACHMENTS: usize = 20;
const MAIL_MODULE_KEY: &str = "mail";

async fn require_mail_enabled(state: &AppState, tenant_id: Uuid) -> Result<(), AppError> {
    let module = state
        .module_service
        .get_module(MAIL_MODULE_KEY, tenant_id)
        .await;
    let module = match module {
        Ok(module) => module,
        Err(ModuleError::NotFound(_)) => {
            return Err(AppError::forbidden("Mail module is disabled"));
        }
        Err(err) => return Err(AppError::internal(err.to_string())),
    };

    if !module.enabled {
        return Err(AppError::forbidden("Mail module is disabled"));
    }

    Ok(())
}

#[derive(Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct CreateMailAccountRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    #[validate(length(min = 1, max = 255))]
    pub host: String,
    #[validate(range(min = 1, max = 65535))]
    pub port: i32,
    #[validate(length(min = 1, max = 512))]
    pub username: String,
    #[validate(length(min = 1, max = 512))]
    pub password: String,
    pub tls_mode: MailTlsMode,
}

#[derive(Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct UpdateMailAccountRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 255))]
    pub host: Option<String>,
    #[validate(range(min = 1, max = 65535))]
    pub port: Option<i32>,
    #[validate(length(min = 1, max = 512))]
    pub username: Option<String>,
    #[validate(length(min = 1, max = 512))]
    pub password: Option<String>,
    pub tls_mode: Option<MailTlsMode>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailAccountResponse {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub tls_mode: String,
    pub is_enabled: bool,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailAccountListResponse {
    pub accounts: Vec<MailAccountResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailFolderListResponse {
    pub folders: Vec<MailFolderResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailFolderResponse {
    pub name: String,
    pub display_name: String,
    pub delimiter: Option<String>,
    pub role: Option<String>,
    /// Unread message count from IMAP STATUS; `null` when STATUS failed.
    pub unseen: Option<u32>,
    /// Total message count from IMAP STATUS; `null` when STATUS failed.
    pub total: Option<u32>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailMessageSummaryResponse {
    pub uid: u32,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub size_bytes: i64,
    pub is_seen: bool,
    pub is_flagged: bool,
    /// ID of the imported RustShare mail message, when this remote UID has
    /// already been imported; `null` otherwise.
    #[schema(value_type = Option<Uuid>)]
    pub imported_message_id: Option<Uuid>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailMessageSummaryListResponse {
    pub uidvalidity: Option<i64>,
    pub next_cursor: Option<i64>,
    pub messages: Vec<MailMessageSummaryResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailTestConnectionResponse {
    pub ok: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SendMailMessageRequest {
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SendMailMessageResponse {
    pub ok: bool,
}

fn validate_selected_uids(uids: &[i64]) -> Result<(), validator::ValidationError> {
    if uids.is_empty() {
        return Err(validator::ValidationError::new("empty_uids"));
    }
    if uids.len() > 1000 {
        return Err(validator::ValidationError::new("too_many_uids"));
    }
    if uids.iter().any(|&uid| uid <= 0 || uid > u32::MAX as i64) {
        return Err(validator::ValidationError::new("invalid_uid"));
    }
    Ok(())
}

fn validate_imap_uid(uid: i64) -> Result<u32, AppError> {
    if uid <= 0 || uid > u32::MAX as i64 {
        return Err(AppError::bad_request("Invalid IMAP UID"));
    }
    Ok(uid as u32)
}

fn require_destination_folder(destination: Option<&str>) -> Result<&str, AppError> {
    destination
        .filter(|folder| !folder.trim().is_empty())
        .ok_or_else(|| AppError::bad_request("A destination folder is required"))
}

#[derive(Debug, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct CreateMailImportJobRequest {
    #[validate(length(min = 1, max = 512))]
    pub folder_name: String,
    /// UIDVALIDITY value observed when the folder was listed. UIDs are only
    /// stable within this value; if it changes, the selected UIDs may refer to
    /// different messages. Servers that did not return UIDVALIDITY should pass null.
    #[validate(range(min = 1))]
    pub source_uidvalidity: Option<i64>,
    #[validate(custom(
        function = "validate_selected_uids",
        message = "selected_uids must be non-empty, contain at most 1000 entries, and all values must be in 1..=u32::MAX"
    ))]
    pub selected_uids: Vec<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailImportJobResponse {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    #[schema(value_type = Uuid)]
    pub account_id: Uuid,
    pub folder_name: String,
    pub status: String,
    pub total_messages: i32,
    pub processed_messages: i32,
    pub failed_messages: i32,
    pub last_error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailImportJobListResponse {
    pub jobs: Vec<MailImportJobResponse>,
}

#[derive(Debug, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct CreateMailArchiveJobRequest {
    #[validate(length(min = 1, max = 255))]
    pub folder_name: String,
    pub archive_since: Option<NaiveDate>,
    pub archive_before: Option<NaiveDate>,
    #[validate(range(min = 1, max = 36500))]
    pub retention_days: Option<i32>,
    #[validate(range(min = 1, max = 100))]
    pub max_retries: Option<i32>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailArchiveJobResponse {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    #[schema(value_type = Uuid)]
    pub account_id: Uuid,
    pub folder_name: String,
    pub source_mode: String,
    pub status: String,
    pub archive_since: Option<NaiveDate>,
    pub archive_before: Option<NaiveDate>,
    pub last_uid_validity: Option<i64>,
    pub last_imported_uid: Option<i64>,
    pub retention_days: Option<i32>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub total_messages: i32,
    pub processed_messages: i32,
    pub failed_messages: i32,
    pub last_error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailArchiveJobListResponse {
    pub jobs: Vec<MailArchiveJobResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailUploadResponse {
    pub id: Uuid,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub imported_at: DateTime<Utc>,
    pub size_bytes: i64,
    pub has_attachments: bool,
}

/// Upload and import a `.eml` file as a RustShare mail artifact.
///
/// POST /api/v1/mail/upload
///
/// Accepts multipart/form-data with a single `file` field containing the
/// `.eml` source. The raw source, plain-text body, HTML body, and attachment
/// payloads are persisted to object storage; metadata is written to the
/// `mail_messages` table.
#[utoipa::path(
    post,
    path = "/api/v1/mail/upload",
    tag = "Mail",
    responses(
        (status = 200, description = "Mail imported", body = MailUploadResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn upload_mail(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<MailUploadResponse>), AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;

    let mut file_temp: Option<tempfile::NamedTempFile> = None;

    while let Some(mut field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Failed to read multipart field: {}", e);
        AppError::internal(format!("Failed to read multipart field: {e}"))
    })? {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "file" {
            file_temp = Some(
                super::stream_multipart_field_to_temp_file(&mut field, MAX_MAIL_UPLOAD_SIZE_BYTES)
                    .await?
                    .0,
            );
        }
    }

    let file_temp = file_temp.ok_or_else(|| AppError::bad_request("Missing file data"))?;
    let raw = tokio::fs::read(file_temp.path())
        .await
        .map_err(|e| AppError::internal(format!("Failed to read uploaded file: {e}")))?;

    let msg = state
        .mail_service
        .import_eml(auth.tenant_id, auth.user_id, auth.user_id, raw)
        .await?;

    Ok((
        StatusCode::OK,
        Json(MailUploadResponse {
            id: msg.id,
            subject: msg.subject,
            from_address: msg.from_address,
            from_name: msg.from_name,
            sent_at: msg.sent_at,
            imported_at: msg.imported_at,
            size_bytes: msg.size_bytes.unwrap_or(0),
            has_attachments: msg.has_attachments,
        }),
    ))
}

/// List imported mail messages.
#[utoipa::path(
    get,
    path = "/api/v1/mail/messages",
    tag = "Mail",
    params(ListImportedMailMessagesQuery),
    responses(
        (status = 200, description = "Mail messages", body = ListMailMessagesResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_messages(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ListImportedMailMessagesQuery>,
) -> Result<Json<ListMailMessagesResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;

    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let cursor = match (query.cursor_at, query.cursor_id) {
        (Some(at), Some(id)) => Some((at, id)),
        (None, None) => None,
        _ => {
            return Err(AppError::bad_request(
                "Both cursor_at and cursor_id are required",
            ))
        }
    };

    let page = state
        .mail_service
        .list_messages(
            auth.tenant_id,
            auth.user_id,
            query
                .search
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
            cursor,
            limit + 1,
        )
        .await?;
    let has_more = page.len() > limit as usize;
    let messages: Vec<_> = page
        .into_iter()
        .take(limit as usize)
        .map(MailMessageResponse::from)
        .collect();

    let next_cursor = has_more.then(|| messages.last()).flatten();

    Ok(Json(ListMailMessagesResponse {
        next_cursor_at: next_cursor.map(|message| message.imported_at),
        next_cursor_id: next_cursor.map(|message| message.id),
        messages,
    }))
}

/// List draft mail messages for an account.
#[utoipa::path(
    get,
    path = "/api/v1/mail/accounts/{id}/drafts",
    tag = "Mail",
    responses(
        (status = 200, description = "Draft messages", body = ListMailMessagesResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_drafts_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
) -> Result<Json<ListMailMessagesResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;

    let messages = state
        .mail_service
        .list_drafts(auth.tenant_id, auth.user_id, account_id)
        .await?
        .into_iter()
        .map(MailMessageResponse::from)
        .collect();

    Ok(Json(ListMailMessagesResponse {
        messages,
        next_cursor_at: None,
        next_cursor_id: None,
    }))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailMessageResponse {
    pub id: Uuid,
    pub account_id: Option<Uuid>,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub to_addresses: serde_json::Value,
    pub cc_addresses: serde_json::Value,
    pub bcc_addresses: serde_json::Value,
    pub sent_at: Option<DateTime<Utc>>,
    pub imported_at: DateTime<Utc>,
    pub size_bytes: i64,
    pub has_attachments: bool,
    pub source_mode: String,
    pub in_reply_to: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListMailMessagesResponse {
    pub messages: Vec<MailMessageResponse>,
    pub next_cursor_at: Option<DateTime<Utc>>,
    pub next_cursor_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListImportedMailMessagesQuery {
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub cursor_at: Option<DateTime<Utc>>,
    pub cursor_id: Option<Uuid>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailMessagePartResponse {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    pub part_index: i32,
    pub content_type: String,
    pub charset: Option<String>,
    pub size_bytes: Option<i64>,
    pub is_body: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListMailMessagePartsResponse {
    pub parts: Vec<MailMessagePartResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListMailMessageAttachmentsResponse {
    pub attachments: Vec<MailAttachmentResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailAttachmentResponse {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    #[schema(value_type = Option<Uuid>)]
    pub file_id: Option<Uuid>,
    pub filename: String,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
}

impl From<rustshare_core::domain::MailMessagePart> for MailMessagePartResponse {
    fn from(part: rustshare_core::domain::MailMessagePart) -> Self {
        Self {
            id: part.id,
            part_index: part.part_index,
            content_type: part.content_type,
            charset: part.charset,
            size_bytes: part.size_bytes,
            is_body: part.is_body,
        }
    }
}

impl From<rustshare_core::domain::MailAttachment> for MailAttachmentResponse {
    fn from(att: rustshare_core::domain::MailAttachment) -> Self {
        Self {
            id: att.id,
            file_id: att.file_id,
            filename: att.filename,
            mime_type: att.mime_type,
            size_bytes: att.size_bytes,
        }
    }
}

impl From<rustshare_core::domain::MailMessage> for MailMessageResponse {
    fn from(msg: rustshare_core::domain::MailMessage) -> Self {
        Self {
            id: msg.id,
            account_id: msg.account_id,
            subject: msg.subject,
            from_address: msg.from_address,
            from_name: msg.from_name,
            to_addresses: msg.to_addresses,
            cc_addresses: msg.cc_addresses,
            bcc_addresses: msg.bcc_addresses,
            sent_at: msg.sent_at,
            imported_at: msg.imported_at,
            size_bytes: msg.size_bytes.unwrap_or(0),
            has_attachments: msg.has_attachments,
            source_mode: msg.source_mode,
            in_reply_to: msg.in_reply_to,
        }
    }
}

/// Get a single imported mail message by ID.
#[utoipa::path(
    get,
    path = "/api/v1/mail/messages/{id}",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail message ID")),
    responses(
        (status = 200, description = "Mail message", body = MailMessageResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_mail_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(message_id): Path<Uuid>,
) -> Result<Json<MailMessageResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;

    let msg = state
        .mail_service
        .get_message(auth.tenant_id, auth.user_id, message_id)
        .await?;

    Ok(Json(MailMessageResponse::from(msg)))
}

/// Get a draft mail message by ID.
#[utoipa::path(
    get,
    path = "/api/v1/mail/accounts/{id}/drafts/{draft_id}",
    tag = "Mail",
    responses(
        (status = 200, description = "Draft message", body = MailMessageResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_draft_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, draft_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<MailMessageResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;

    let msg = state
        .mail_service
        .get_draft(auth.tenant_id, auth.user_id, account_id, draft_id)
        .await?;

    Ok(Json(MailMessageResponse::from(msg)))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateMailLinkRequest {
    pub target_type: String,
    #[schema(value_type = Uuid)]
    pub target_id: Uuid,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailLinkResponse {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    #[schema(value_type = Uuid)]
    pub message_id: Uuid,
    pub target_type: String,
    #[schema(value_type = Uuid)]
    pub target_id: Uuid,
    #[schema(value_type = Uuid)]
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailLinkListResponse {
    pub links: Vec<MailLinkResponse>,
}

fn parse_target_type(target_type: &str) -> Result<LinkTargetType, AppError> {
    target_type.parse().map_err(|_| {
        AppError::bad_request(format!(
            "Invalid target_type: {target_type}. Expected one of: note, kanban_card, kanban_board, meeting, file, folder, mail_message"
        ))
    })
}

fn link_to_response(link: rustshare_core::domain::MailLink) -> MailLinkResponse {
    MailLinkResponse {
        id: link.id,
        message_id: link.message_id,
        target_type: link.target_type,
        target_id: link.target_id,
        created_by: link.created_by,
        created_at: link.created_at,
    }
}

/// Link a mail message to another RustShare object.
#[utoipa::path(
    post,
    path = "/api/v1/mail/messages/{id}/links",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail message ID")),
    request_body = CreateMailLinkRequest,
    responses(
        (status = 200, description = "Link created", body = MailLinkResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_mail_link(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(message_id): Path<Uuid>,
    Json(req): Json<CreateMailLinkRequest>,
) -> Result<Json<MailLinkResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let target_type = parse_target_type(&req.target_type)?;
    let link = state
        .mail_service
        .link_message(
            auth.tenant_id,
            auth.user_id,
            message_id,
            target_type,
            req.target_id,
        )
        .await?;

    Ok(Json(link_to_response(link)))
}

/// Remove a link between a mail message and another RustShare object.
#[utoipa::path(
    delete,
    path = "/api/v1/mail/messages/{id}/links/{link_id}",
    tag = "Mail",
    params(
        ("id" = Uuid, Path, description = "Mail message ID"),
        ("link_id" = Uuid, Path, description = "Link ID"),
    ),
    responses(
        (status = 200, description = "Link removed", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn delete_mail_link(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((message_id, link_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;

    // Validate the link belongs to the message in the URL. We load the link
    // including soft-deleted rows so that retrying a DELETE after the first
    // request succeeded remains idempotent (returns 200 instead of 404).
    let link = state
        .mail_service
        .find_mail_link_by_id(auth.tenant_id, auth.user_id, link_id)
        .await?;
    if link.message_id != message_id {
        return Err(AppError::not_found("link"));
    }

    state
        .mail_service
        .unlink_message(auth.tenant_id, auth.user_id, link_id)
        .await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// List active links for a mail message.
#[utoipa::path(
    get,
    path = "/api/v1/mail/messages/{id}/links",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail message ID")),
    responses(
        (status = 200, description = "Mail links", body = MailLinkListResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_links(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(message_id): Path<Uuid>,
) -> Result<Json<MailLinkListResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let links = state
        .mail_service
        .list_message_links(auth.tenant_id, auth.user_id, message_id)
        .await?;

    Ok(Json(MailLinkListResponse {
        links: links.into_iter().map(link_to_response).collect(),
    }))
}

fn account_to_response(account: rustshare_core::domain::MailAccount) -> MailAccountResponse {
    MailAccountResponse {
        id: account.id,
        name: account.name,
        host: account.host,
        port: account.port,
        username: account.username,
        tls_mode: account.tls_mode,
        is_enabled: account.is_enabled,
        last_connected_at: account.last_connected_at,
        last_error: account.last_error,
        created_at: account.created_at,
    }
}

fn folder_to_response(folder: crate::services::imap_client::MailFolder) -> MailFolderResponse {
    MailFolderResponse {
        name: folder.name,
        display_name: folder.display_name,
        delimiter: folder.delimiter,
        role: folder.role,
        unseen: folder.unseen,
        total: folder.total,
    }
}

fn summary_to_response(
    summary: crate::services::imap_client::ImapMessageSummary,
    imported_message_id: Option<Uuid>,
) -> MailMessageSummaryResponse {
    MailMessageSummaryResponse {
        uid: summary.uid,
        subject: summary.subject,
        from_address: summary.from_address,
        from_name: summary.from_name,
        sent_at: summary.sent_at,
        size_bytes: summary.size_bytes,
        is_seen: summary.is_seen,
        is_flagged: summary.is_flagged,
        imported_message_id,
    }
}

fn job_to_response(job: rustshare_core::domain::MailImportJob) -> MailImportJobResponse {
    MailImportJobResponse {
        id: job.id,
        account_id: job.account_id,
        folder_name: job.folder_name,
        status: job.status,
        total_messages: job.total_messages,
        processed_messages: job.processed_messages,
        failed_messages: job.failed_messages,
        last_error: job.last_error,
        started_at: job.started_at,
        completed_at: job.completed_at,
        created_at: job.created_at,
    }
}

fn archive_job_to_response(job: rustshare_core::domain::MailImportJob) -> MailArchiveJobResponse {
    MailArchiveJobResponse {
        id: job.id,
        account_id: job.account_id,
        folder_name: job.folder_name,
        source_mode: job.source_mode,
        status: job.status,
        archive_since: job.archive_since,
        archive_before: job.archive_before,
        last_uid_validity: job.last_uid_validity,
        last_imported_uid: job.last_imported_uid,
        retention_days: job.retention_days,
        retry_count: job.retry_count,
        max_retries: job.max_retries,
        total_messages: job.total_messages,
        processed_messages: job.processed_messages,
        failed_messages: job.failed_messages,
        last_error: job.last_error,
        started_at: job.started_at,
        completed_at: job.completed_at,
        created_at: job.created_at,
        updated_at: job.updated_at,
    }
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListMailMessagesQuery {
    #[serde(default)]
    folder: String,
    #[serde(default = "default_message_limit")]
    limit: i64,
    cursor: Option<i64>,
    search: Option<String>,
}

fn default_message_limit() -> i64 {
    100
}

/// Send a plain-text outbound email through the configured SMTP relay.
#[utoipa::path(
    post,
    path = "/api/v1/mail/send",
    tag = "Mail",
    request_body = SendMailMessageRequest,
    responses(
        (status = 200, description = "Mail sent", body = SendMailMessageResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 502, description = "SMTP send failed", body = crate::handlers::ErrorResponse),
        (status = 503, description = "SMTP unavailable", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn send_mail_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<SendMailMessageRequest>,
) -> Result<Json<SendMailMessageResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    validate_send_mail_request(&req)?;

    let user = state
        .metadata_store
        .find_user_by_id(auth.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if user.tenant_id != auth.tenant_id {
        return Err(AppError::Forbidden(
            "User is not in this tenant".to_string(),
        ));
    }
    if !user.is_admin {
        return Err(AppError::Forbidden(
            "Admin access required for system SMTP relay".to_string(),
        ));
    }

    let email_service = EmailService::new(state.db_pool.clone(), state.secret_key.clone());
    email_service
        .send_user_email(OutboundEmail {
            sender_name: &user.display_name,
            sender_email: &user.email,
            recipients: &req.to,
            cc: &req.cc,
            bcc: &req.bcc,
            subject: req.subject.trim(),
            body: &req.body,
        })
        .await
        .map_err(email_error_to_app_error)?;

    emit_mail_message_sent(&state, auth.user_id, &req).await?;
    Ok(Json(SendMailMessageResponse { ok: true }))
}

/// Create a new IMAP mail account.
#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts",
    tag = "Mail",
    request_body = CreateMailAccountRequest,
    responses(
        (status = 201, description = "Account created", body = MailAccountResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_mail_account(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    ValidatedJson(req): ValidatedJson<CreateMailAccountRequest>,
) -> Result<(StatusCode, Json<MailAccountResponse>), AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let account = state
        .mail_service
        .create_account(
            auth.tenant_id,
            auth.user_id,
            req.name,
            req.host,
            req.port,
            req.username,
            req.password,
            req.tls_mode,
        )
        .await?;

    Ok((StatusCode::CREATED, Json(account_to_response(account))))
}

/// List mail accounts for the authenticated user.
#[utoipa::path(
    get,
    path = "/api/v1/mail/accounts",
    tag = "Mail",
    responses(
        (status = 200, description = "Mail accounts", body = MailAccountListResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_accounts(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<MailAccountListResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let accounts = state
        .mail_service
        .list_accounts(auth.tenant_id, auth.user_id)
        .await?;

    Ok(Json(MailAccountListResponse {
        accounts: accounts.into_iter().map(account_to_response).collect(),
    }))
}

/// Get a single mail account by ID.
#[utoipa::path(
    get,
    path = "/api/v1/mail/accounts/{id}",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID")),
    responses(
        (status = 200, description = "Mail account", body = MailAccountResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_mail_account(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
) -> Result<Json<MailAccountResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let account = state
        .mail_service
        .get_account(auth.tenant_id, auth.user_id, account_id)
        .await?;

    Ok(Json(account_to_response(account)))
}

/// Update a mail account.
#[utoipa::path(
    patch,
    path = "/api/v1/mail/accounts/{id}",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID")),
    request_body = UpdateMailAccountRequest,
    responses(
        (status = 200, description = "Account updated", body = MailAccountResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_mail_account(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateMailAccountRequest>,
) -> Result<Json<MailAccountResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let account = state
        .mail_service
        .update_account(
            auth.tenant_id,
            auth.user_id,
            account_id,
            req.name,
            req.host,
            req.port,
            req.username,
            req.password,
            req.tls_mode,
            req.is_enabled,
        )
        .await?;

    Ok(Json(account_to_response(account)))
}

/// Delete a mail account.
#[utoipa::path(
    delete,
    path = "/api/v1/mail/accounts/{id}",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID")),
    responses(
        (status = 200, description = "Account deleted", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn delete_mail_account(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    state
        .mail_service
        .delete_account(auth.tenant_id, auth.user_id, account_id)
        .await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Test a mail account's IMAP connection.
#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/test",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID")),
    responses(
        (status = 200, description = "Connection successful", body = MailTestConnectionResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn test_mail_account(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
) -> Result<Json<MailTestConnectionResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    state
        .mail_service
        .test_account_connection(auth.tenant_id, auth.user_id, account_id)
        .await?;

    Ok(Json(MailTestConnectionResponse { ok: true }))
}

/// List folders available on a mail account's IMAP server.
#[utoipa::path(
    get,
    path = "/api/v1/mail/accounts/{id}/folders",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID")),
    responses(
        (status = 200, description = "Folder list", body = MailFolderListResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_account_folders(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
) -> Result<Json<MailFolderListResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let folders = state
        .mail_service
        .list_imap_folders(auth.tenant_id, auth.user_id, account_id)
        .await?;

    Ok(Json(MailFolderListResponse {
        folders: folders.into_iter().map(folder_to_response).collect(),
    }))
}

/// List message summaries in an IMAP folder.
#[utoipa::path(
    get,
    path = "/api/v1/mail/accounts/{id}/messages",
    tag = "Mail",
    params(
        ("id" = Uuid, Path, description = "Mail account ID"),
        ListMailMessagesQuery,
    ),
    responses(
        (status = 200, description = "Message summaries", body = MailMessageSummaryListResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_account_messages(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
    Query(query): Query<ListMailMessagesQuery>,
) -> Result<Json<MailMessageSummaryListResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    if query.folder.trim().is_empty() {
        return Err(AppError::bad_request("Missing folder query parameter"));
    }
    if query.folder.len() > 512 {
        return Err(AppError::bad_request(
            "Folder name must be at most 512 characters",
        ));
    }
    if query.limit <= 0 || query.limit > 1000 {
        return Err(AppError::bad_request("limit must be between 1 and 1000"));
    }

    let (uidvalidity, messages) = state
        .mail_service
        .list_imap_messages(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &query.folder,
            query.limit as usize,
            query.cursor.and_then(|value| u32::try_from(value).ok()),
            query
                .search
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
        )
        .await?;

    // Single batched lookup for the whole page: which of these remote UIDs
    // have already been imported into RustShare.
    let uids: Vec<i64> = messages
        .iter()
        .map(|message| i64::from(message.uid))
        .collect();
    let imported: std::collections::HashMap<i64, Uuid> = state
        .metadata_store
        .find_imported_mail_message_ids(
            auth.user_id,
            account_id,
            &query.folder,
            &uids,
            uidvalidity.map(i64::from),
        )
        .await
        .map_err(|e| AppError::internal(e.to_string()))?
        .into_iter()
        .collect();

    let messages: Vec<_> = messages
        .into_iter()
        .map(|summary| {
            let imported_message_id = imported.get(&i64::from(summary.uid)).copied();
            summary_to_response(summary, imported_message_id)
        })
        .collect();
    let next_cursor = messages
        .iter()
        .map(|message| message.uid)
        .min()
        .map(i64::from);

    Ok(Json(MailMessageSummaryListResponse {
        uidvalidity: uidvalidity.map(i64::from),
        next_cursor,
        messages,
    }))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MailMessageActionRequest {
    pub folder: String,
    pub source_uidvalidity: Option<i64>,
    pub destination_folder: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MailMessageMoveRequest {
    pub folder: String,
    pub destination_folder: String,
    pub source_uidvalidity: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/messages/{uid}/mark-read",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID"), ("uid" = i64, Path, description = "IMAP UID")),
    request_body = MailMessageActionRequest,
    responses((status = 204, description = "Message marked read")),
)]
pub async fn mark_mail_message_read(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, uid)): Path<(Uuid, i64)>,
    Json(req): Json<MailMessageActionRequest>,
) -> Result<StatusCode, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let uid = validate_imap_uid(uid)?;
    state
        .mail_service
        .mark_imap_message_seen(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &req.folder,
            req.source_uidvalidity,
            uid,
            true,
        )
        .await?;
    emit_mail_remote_action(
        &state,
        account_id,
        auth.user_id,
        &req.folder,
        uid,
        "mark_read",
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/messages/{uid}/mark-unread",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID"), ("uid" = i64, Path, description = "IMAP UID")),
    request_body = MailMessageActionRequest,
    responses((status = 204, description = "Message marked unread")),
)]
pub async fn mark_mail_message_unread(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, uid)): Path<(Uuid, i64)>,
    Json(req): Json<MailMessageActionRequest>,
) -> Result<StatusCode, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let uid = validate_imap_uid(uid)?;
    state
        .mail_service
        .mark_imap_message_seen(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &req.folder,
            req.source_uidvalidity,
            uid,
            false,
        )
        .await?;
    emit_mail_remote_action(
        &state,
        account_id,
        auth.user_id,
        &req.folder,
        uid,
        "mark_unread",
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/messages/{uid}/star",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID"), ("uid" = i64, Path, description = "IMAP UID")),
    request_body = MailMessageActionRequest,
    responses((status = 204, description = "Message starred")),
)]
pub async fn star_mail_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, uid)): Path<(Uuid, i64)>,
    Json(req): Json<MailMessageActionRequest>,
) -> Result<StatusCode, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let uid = validate_imap_uid(uid)?;
    state
        .mail_service
        .mark_imap_message_flagged(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &req.folder,
            req.source_uidvalidity,
            uid,
            true,
        )
        .await?;
    emit_mail_remote_action(&state, account_id, auth.user_id, &req.folder, uid, "star").await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/messages/{uid}/unstar",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID"), ("uid" = i64, Path, description = "IMAP UID")),
    request_body = MailMessageActionRequest,
    responses((status = 204, description = "Message unstarred")),
)]
pub async fn unstar_mail_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, uid)): Path<(Uuid, i64)>,
    Json(req): Json<MailMessageActionRequest>,
) -> Result<StatusCode, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let uid = validate_imap_uid(uid)?;
    state
        .mail_service
        .mark_imap_message_flagged(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &req.folder,
            req.source_uidvalidity,
            uid,
            false,
        )
        .await?;
    emit_mail_remote_action(&state, account_id, auth.user_id, &req.folder, uid, "unstar").await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct RemoteMailMessageQuery {
    /// IMAP folder containing the message.
    pub folder: String,
    /// UIDVALIDITY observed when the folder was listed; guards against stale UIDs.
    pub source_uidvalidity: Option<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailRemoteAddressResponse {
    pub name: Option<String>,
    pub address: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailRemoteAttachmentResponse {
    /// Index into the parsed attachment list; used by the attachment download endpoint.
    pub index: usize,
    pub filename: Option<String>,
    pub mime_type: String,
    pub size_bytes: usize,
    pub content_id: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailRemoteMessageBodyResponse {
    pub uid: u32,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub to: Vec<MailRemoteAddressResponse>,
    pub cc: Vec<MailRemoteAddressResponse>,
    pub date: Option<DateTime<Utc>>,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub text: Option<String>,
    /// Raw (unsanitized) HTML body; the frontend sanitizes it before rendering.
    pub html: Option<String>,
    pub attachments: Vec<MailRemoteAttachmentResponse>,
    pub is_seen: bool,
    pub is_flagged: bool,
}

fn parse_remote_rfc822(
    uid: u32,
    rfc822: &[u8],
) -> Result<rustshare_core::services::eml_parser::ParsedMail, AppError> {
    rustshare_core::services::eml_parser::EmlParser::parse(rfc822)
        .map_err(|e| AppError::internal(format!("Failed to parse remote message {uid}: {e}")))
}

fn validate_remote_message_query(query: &RemoteMailMessageQuery) -> Result<(), AppError> {
    if query.folder.trim().is_empty() {
        return Err(AppError::bad_request("Missing folder query parameter"));
    }
    if query.folder.len() > 512 {
        return Err(AppError::bad_request(
            "Folder name must be at most 512 characters",
        ));
    }
    Ok(())
}

fn remote_address_to_response(
    address: rustshare_core::services::eml_parser::ParsedAddress,
) -> MailRemoteAddressResponse {
    MailRemoteAddressResponse {
        name: address.name,
        address: address.address,
    }
}

/// Fetch and parse a message that still lives on the remote IMAP server.
#[utoipa::path(
    get,
    path = "/api/v1/mail/accounts/{id}/messages/{uid}/body",
    tag = "Mail",
    params(
        ("id" = Uuid, Path, description = "Mail account ID"),
        ("uid" = i64, Path, description = "IMAP UID"),
        RemoteMailMessageQuery,
    ),
    responses(
        (status = 200, description = "Remote message body", body = MailRemoteMessageBodyResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Message not found", body = crate::handlers::ErrorResponse),
        (status = 409, description = "Folder UIDVALIDITY changed", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_remote_mail_message_body(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, uid)): Path<(Uuid, i64)>,
    Query(query): Query<RemoteMailMessageQuery>,
) -> Result<Json<MailRemoteMessageBodyResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    validate_remote_message_query(&query)?;
    let uid = validate_imap_uid(uid)?;

    let remote = state
        .mail_service
        .fetch_imap_message_source(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &query.folder,
            query.source_uidvalidity,
            uid,
        )
        .await?;
    let parsed = parse_remote_rfc822(uid, &remote.rfc822)?;

    let from = parsed.from;
    Ok(Json(MailRemoteMessageBodyResponse {
        uid,
        subject: parsed.subject,
        from_address: from.as_ref().map(|from| from.address.clone()),
        from_name: from.as_ref().and_then(|from| from.name.clone()),
        to: parsed
            .to
            .into_iter()
            .map(remote_address_to_response)
            .collect(),
        cc: parsed
            .cc
            .into_iter()
            .map(remote_address_to_response)
            .collect(),
        date: parsed.sent_at,
        message_id: parsed.message_id,
        in_reply_to: parsed.in_reply_to,
        text: parsed.body_text,
        html: parsed.body_html,
        attachments: parsed
            .attachments
            .iter()
            .enumerate()
            .map(|(index, attachment)| MailRemoteAttachmentResponse {
                index,
                filename: attachment.filename.clone(),
                mime_type: attachment.mime_type.clone(),
                size_bytes: attachment.size_bytes,
                content_id: attachment.content_id.clone(),
            })
            .collect(),
        is_seen: remote.is_seen,
        is_flagged: remote.is_flagged,
    }))
}

/// Download one attachment of a remote IMAP message.
#[utoipa::path(
    get,
    path = "/api/v1/mail/accounts/{id}/messages/{uid}/attachments/{index}",
    tag = "Mail",
    params(
        ("id" = Uuid, Path, description = "Mail account ID"),
        ("uid" = i64, Path, description = "IMAP UID"),
        ("index" = usize, Path, description = "Attachment index from the body endpoint"),
        RemoteMailMessageQuery,
    ),
    responses(
        (status = 200, description = "Attachment bytes"),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Message or attachment not found", body = crate::handlers::ErrorResponse),
        (status = 409, description = "Folder UIDVALIDITY changed", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn download_remote_mail_attachment(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, uid, index)): Path<(Uuid, i64, usize)>,
    Query(query): Query<RemoteMailMessageQuery>,
) -> Result<Response, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    validate_remote_message_query(&query)?;
    let uid = validate_imap_uid(uid)?;

    // NOTE: this re-fetches and re-parses the full RFC 822 source to extract
    // the attachment part by index. Acceptable for interactive downloads
    // (messages are capped at 25 MB); revisit if it becomes a hotspot.
    let remote = state
        .mail_service
        .fetch_imap_message_source(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &query.folder,
            query.source_uidvalidity,
            uid,
        )
        .await?;
    let parsed = parse_remote_rfc822(uid, &remote.rfc822)?;

    let attachment = parsed
        .attachments
        .into_iter()
        .nth(index)
        .ok_or_else(|| AppError::not_found("attachment"))?;

    let filename = attachment
        .filename
        .clone()
        .unwrap_or_else(|| format!("attachment-{index}"));
    let content_disposition = super::public_shares::build_content_disposition(&filename);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&attachment.mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition)
            .map_err(|e| AppError::internal(e.to_string()))?,
    );
    Ok((StatusCode::OK, headers, attachment.data).into_response())
}

#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/messages/{uid}/move",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID"), ("uid" = i64, Path, description = "IMAP UID")),
    request_body = MailMessageMoveRequest,
    responses((status = 204, description = "Message moved")),
)]
pub async fn move_mail_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, uid)): Path<(Uuid, i64)>,
    Json(req): Json<MailMessageMoveRequest>,
) -> Result<StatusCode, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let uid = validate_imap_uid(uid)?;
    state
        .mail_service
        .move_imap_message(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &req.folder,
            req.source_uidvalidity,
            uid,
            &req.destination_folder,
        )
        .await?;
    emit_mail_remote_action(&state, account_id, auth.user_id, &req.folder, uid, "move").await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/messages/{uid}/archive",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID"), ("uid" = i64, Path, description = "IMAP UID")),
    request_body = MailMessageActionRequest,
    responses((status = 204, description = "Message archived")),
)]
pub async fn archive_mail_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, uid)): Path<(Uuid, i64)>,
    Json(req): Json<MailMessageActionRequest>,
) -> Result<StatusCode, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let uid = validate_imap_uid(uid)?;
    let destination = require_destination_folder(req.destination_folder.as_deref())?;
    state
        .mail_service
        .move_imap_message(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &req.folder,
            req.source_uidvalidity,
            uid,
            destination,
        )
        .await?;
    emit_mail_remote_action(
        &state,
        account_id,
        auth.user_id,
        &req.folder,
        uid,
        "archive",
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/messages/{uid}/trash",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID"), ("uid" = i64, Path, description = "IMAP UID")),
    request_body = MailMessageActionRequest,
    responses((status = 204, description = "Message trashed")),
)]
pub async fn trash_mail_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, uid)): Path<(Uuid, i64)>,
    Json(req): Json<MailMessageActionRequest>,
) -> Result<StatusCode, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let uid = validate_imap_uid(uid)?;
    let destination = require_destination_folder(req.destination_folder.as_deref())?;
    state
        .mail_service
        .move_imap_message(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &req.folder,
            req.source_uidvalidity,
            uid,
            destination,
        )
        .await?;
    emit_mail_remote_action(&state, account_id, auth.user_id, &req.folder, uid, "trash").await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/mail/accounts/{id}/messages/{uid}",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID"), ("uid" = i64, Path, description = "IMAP UID")),
    request_body = MailMessageActionRequest,
    responses((status = 204, description = "Message deleted")),
)]
pub async fn delete_mail_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, uid)): Path<(Uuid, i64)>,
    Json(req): Json<MailMessageActionRequest>,
) -> Result<StatusCode, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let uid = validate_imap_uid(uid)?;
    state
        .mail_service
        .delete_imap_message(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &req.folder,
            req.source_uidvalidity,
            uid,
        )
        .await?;
    emit_mail_remote_action(&state, account_id, auth.user_id, &req.folder, uid, "delete").await;
    Ok(StatusCode::NO_CONTENT)
}

/// Create a job to import selected messages from an IMAP folder.
#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/import",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID")),
    request_body = CreateMailImportJobRequest,
    responses(
        (status = 202, description = "Import job created", body = MailImportJobResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_mail_import_job(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<CreateMailImportJobRequest>,
) -> Result<(StatusCode, Json<MailImportJobResponse>), AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let job = state
        .mail_service
        .create_imap_import_job(
            auth.tenant_id,
            auth.user_id,
            account_id,
            req.folder_name,
            req.source_uidvalidity,
            req.selected_uids,
        )
        .await?;

    Ok((StatusCode::ACCEPTED, Json(job_to_response(job))))
}

/// Get a single mail import job by ID.
#[utoipa::path(
    get,
    path = "/api/v1/mail/import-jobs/{id}",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Import job ID")),
    responses(
        (status = 200, description = "Import job", body = MailImportJobResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_mail_import_job(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(job_id): Path<Uuid>,
) -> Result<Json<MailImportJobResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let job = state
        .mail_service
        .get_import_job(auth.tenant_id, auth.user_id, job_id)
        .await?;

    Ok(Json(job_to_response(job)))
}

/// List import jobs owned by the current user.
#[utoipa::path(
    get,
    path = "/api/v1/mail/import-jobs",
    tag = "Mail",
    responses(
        (status = 200, description = "Import jobs", body = MailImportJobListResponse),
    ),
)]
pub async fn list_mail_import_jobs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<MailImportJobListResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let jobs = state
        .mail_service
        .list_import_jobs(auth.tenant_id, auth.user_id, None)
        .await?;
    Ok(Json(MailImportJobListResponse {
        jobs: jobs.into_iter().map(job_to_response).collect(),
    }))
}

/// Create a recurring IMAP archive job for a folder and optional date range.
#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/archive-jobs",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID")),
    request_body = CreateMailArchiveJobRequest,
    responses(
        (status = 202, description = "Archive job created", body = MailArchiveJobResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_mail_archive_job(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<CreateMailArchiveJobRequest>,
) -> Result<(StatusCode, Json<MailArchiveJobResponse>), AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let job = state
        .mail_service
        .create_archive_job(
            auth.tenant_id,
            auth.user_id,
            account_id,
            req.folder_name,
            req.archive_since,
            req.archive_before,
            req.retention_days,
            req.max_retries,
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(archive_job_to_response(job))))
}

/// List active archive jobs for a mail account.
#[utoipa::path(
    get,
    path = "/api/v1/mail/accounts/{id}/archive-jobs",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail account ID")),
    responses(
        (status = 200, description = "Archive jobs", body = MailArchiveJobListResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_archive_jobs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
) -> Result<Json<MailArchiveJobListResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let jobs = state
        .mail_service
        .list_archive_jobs(auth.tenant_id, auth.user_id, account_id)
        .await?;
    Ok(Json(MailArchiveJobListResponse {
        jobs: jobs.into_iter().map(archive_job_to_response).collect(),
    }))
}

/// Get a single archive job by ID.
#[utoipa::path(
    get,
    path = "/api/v1/mail/archive-jobs/{job_id}",
    tag = "Mail",
    params(("job_id" = Uuid, Path, description = "Archive job ID")),
    responses(
        (status = 200, description = "Archive job", body = MailArchiveJobResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_mail_archive_job(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(job_id): Path<Uuid>,
) -> Result<Json<MailArchiveJobResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let job = state
        .mail_service
        .get_archive_job(auth.tenant_id, auth.user_id, job_id)
        .await?;
    Ok(Json(archive_job_to_response(job)))
}

/// Cancel a pending or running archive job.
#[utoipa::path(
    patch,
    path = "/api/v1/mail/archive-jobs/{job_id}/cancel",
    tag = "Mail",
    params(("job_id" = Uuid, Path, description = "Archive job ID")),
    responses(
        (status = 200, description = "Archive job cancelled", body = MailArchiveJobResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
        (status = 409, description = "Cannot cancel job in current state", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn cancel_mail_archive_job(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(job_id): Path<Uuid>,
) -> Result<Json<MailArchiveJobResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let job = state
        .mail_service
        .cancel_archive_job(auth.tenant_id, auth.user_id, job_id)
        .await?;
    Ok(Json(archive_job_to_response(job)))
}

/// Soft-delete an archive job.
#[utoipa::path(
    delete,
    path = "/api/v1/mail/archive-jobs/{job_id}",
    tag = "Mail",
    params(("job_id" = Uuid, Path, description = "Archive job ID")),
    responses(
        (status = 204, description = "Archive job deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn delete_mail_archive_job(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(job_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    state
        .mail_service
        .delete_archive_job(auth.tenant_id, auth.user_id, job_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Sanitize stored message HTML. When `allow_remote_images` is false (the
/// default), remote `<img src>` values are stripped to protect privacy; the
/// returned bool reports whether any image was blocked.
fn sanitize_email_html(html: &str, allow_remote_images: bool) -> (String, bool) {
    let schemes: std::collections::HashSet<&str> = ["http", "https", "mailto", "cid", "data"]
        .into_iter()
        .collect();
    let blocked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let blocked_flag = blocked.clone();
    let clean = ammonia::Builder::default()
        .url_schemes(schemes)
        .attribute_filter(move |element, attribute, value| {
            let source = value.trim_start();
            if !allow_remote_images
                && element == "img"
                && attribute == "src"
                && (source.starts_with("//")
                    || source.split_once(':').is_some_and(|(scheme, _)| {
                        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
                    }))
            {
                blocked_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                return None;
            }
            Some(value.into())
        })
        .clean(html)
        .to_string();
    (clean, blocked.load(std::sync::atomic::Ordering::Relaxed))
}

fn rewrite_cid_urls(html: &str, attachments: &[rustshare_core::domain::MailAttachment]) -> String {
    attachments
        .iter()
        .fold(html.to_string(), |html, attachment| {
            match (&attachment.content_id, attachment.file_id) {
                (Some(content_id), Some(file_id)) => html.replace(
                    &format!("cid:{}", content_id.trim_matches(['<', '>'])),
                    &format!("/api/v1/files/{file_id}/preview"),
                ),
                _ => html,
            }
        })
}

/// List body parts for an imported mail message.
#[utoipa::path(
    get,
    path = "/api/v1/mail/messages/{id}/parts",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail message ID")),
    responses(
        (status = 200, description = "Message parts", body = ListMailMessagePartsResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_message_parts(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(message_id): Path<Uuid>,
) -> Result<Json<ListMailMessagePartsResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let parts = state
        .mail_service
        .list_parts(auth.tenant_id, auth.user_id, message_id)
        .await?;
    Ok(Json(ListMailMessagePartsResponse {
        parts: parts
            .into_iter()
            .map(MailMessagePartResponse::from)
            .collect(),
    }))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MailMessagePartQuery {
    /// Load remote images in sanitized HTML parts (blocked by default).
    #[serde(default)]
    pub load_remote_images: bool,
}

/// Get the content of a single message part. HTML parts are sanitized before delivery.
#[utoipa::path(
    get,
    path = "/api/v1/mail/messages/{id}/parts/{part_id}",
    tag = "Mail",
    params(
        ("id" = Uuid, Path, description = "Mail message ID"),
        ("part_id" = Uuid, Path, description = "Part ID"),
        MailMessagePartQuery,
    ),
    responses(
        (status = 200, description = "Part content"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_mail_message_part(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((message_id, part_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<MailMessagePartQuery>,
) -> Result<Response, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let (part, bytes) = state
        .mail_service
        .get_message_part(auth.tenant_id, auth.user_id, message_id, part_id)
        .await?;

    let content_type = if part.content_type.eq_ignore_ascii_case("text/html") {
        let html = std::str::from_utf8(&bytes)
            .map_err(|_| AppError::internal("HTML part is not valid UTF-8"))?;
        let attachments = state
            .mail_service
            .list_attachments(auth.tenant_id, auth.user_id, message_id)
            .await?;
        let (sanitized, blocked_remote_images) = sanitize_email_html(
            &rewrite_cid_urls(html, &attachments),
            query.load_remote_images,
        );
        if let Err(e) = emit_mail_message_viewed(&state, message_id, auth.user_id, "body").await {
            tracing::warn!(error = ?e, message_id = %message_id, "failed to record mail view event");
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox"),
        );
        if blocked_remote_images {
            headers.insert(
                HeaderName::from_static("x-mail-blocked-remote-images"),
                HeaderValue::from_static("1"),
            );
        }
        return Ok((StatusCode::OK, headers, sanitized).into_response());
    } else if let Some(charset) = &part.charset {
        format!("{}; charset={}", part.content_type, charset)
    } else {
        part.content_type.clone()
    };

    if let Err(e) = emit_mail_message_viewed(&state, message_id, auth.user_id, "body").await {
        tracing::warn!(error = ?e, message_id = %message_id, "failed to record mail view event");
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    Ok((StatusCode::OK, headers, bytes).into_response())
}

/// Download the raw `.eml` source for an imported mail message.
#[utoipa::path(
    get,
    path = "/api/v1/mail/messages/{id}/source",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail message ID")),
    responses(
        (status = 200, description = "Raw message source"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn download_mail_message_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(message_id): Path<Uuid>,
) -> Result<Response, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let (filename, bytes) = state
        .mail_service
        .download_message_source(auth.tenant_id, auth.user_id, message_id)
        .await?;
    if let Err(e) = emit_mail_message_viewed(&state, message_id, auth.user_id, "source").await {
        tracing::warn!(error = ?e, message_id = %message_id, "failed to record mail view event");
    }

    let content_disposition = super::public_shares::build_content_disposition(&filename);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("message/rfc822"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition)
            .map_err(|e| AppError::internal(e.to_string()))?,
    );
    Ok((StatusCode::OK, headers, bytes).into_response())
}

/// List attachments for an imported mail message.
#[utoipa::path(
    get,
    path = "/api/v1/mail/messages/{id}/attachments",
    tag = "Mail",
    params(("id" = Uuid, Path, description = "Mail message ID")),
    responses(
        (status = 200, description = "Attachments", body = ListMailMessageAttachmentsResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_message_attachments(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(message_id): Path<Uuid>,
) -> Result<Json<ListMailMessageAttachmentsResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let attachments = state
        .mail_service
        .list_attachments(auth.tenant_id, auth.user_id, message_id)
        .await?;
    Ok(Json(ListMailMessageAttachmentsResponse {
        attachments: attachments
            .into_iter()
            .map(MailAttachmentResponse::from)
            .collect(),
    }))
}

/// Best-effort audit event for mutations performed directly on the remote
/// IMAP mailbox (mark read/unread, star/unstar, move, archive, trash, delete).
/// Failures are logged, never propagated, so the mailbox action still succeeds.
async fn emit_mail_remote_action(
    state: &AppState,
    account_id: Uuid,
    user_id: rustshare_core::domain::UserId,
    folder: &str,
    uid: u32,
    action: &str,
) {
    use rustshare_core::events::{AggregateType, Event, EventType, MailRemoteActionPayload};
    let result = async {
        let payload = MailRemoteActionPayload {
            account_id,
            folder: folder.to_string(),
            uid,
            action: action.to_string(),
        };
        let event = Event::new(
            EventType::MailRemoteAction,
            account_id,
            AggregateType::MailAccount,
            serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
            user_id,
        );
        state
            .event_store
            .append(&event, &state.broadcaster)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;
        Ok::<(), AppError>(())
    }
    .await;
    if let Err(e) = result {
        tracing::warn!(error = ?e, %account_id, folder, uid, action, "failed to record remote mail action event");
    }
}

async fn emit_mail_message_viewed(
    state: &AppState,
    message_id: Uuid,
    viewed_by: rustshare_core::domain::UserId,
    view_type: &str,
) -> Result<(), AppError> {
    use rustshare_core::events::{AggregateType, Event, EventType, MailMessageViewedPayload};
    let payload = MailMessageViewedPayload {
        message_id,
        viewed_by,
        view_type: view_type.to_string(),
    };
    let event = Event::new(
        EventType::MailMessageViewed,
        message_id,
        AggregateType::MailMessage,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        viewed_by,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(())
}

fn validate_send_mail_request(req: &SendMailMessageRequest) -> Result<(), AppError> {
    let recipient_count = req.to.len() + req.cc.len() + req.bcc.len();
    if recipient_count == 0 {
        return Err(AppError::bad_request("At least one recipient is required"));
    }
    if recipient_count > MAX_MAIL_SEND_RECIPIENTS {
        return Err(AppError::bad_request(format!(
            "At most {MAX_MAIL_SEND_RECIPIENTS} recipients are allowed"
        )));
    }
    if req.subject.trim().is_empty() {
        return Err(AppError::bad_request("Subject is required"));
    }
    if req.subject.len() > 998 {
        return Err(AppError::bad_request("Subject is too long"));
    }
    if req.body.trim().is_empty() {
        return Err(AppError::bad_request("Body is required"));
    }
    if req.body.len() > MAX_MAIL_SEND_BODY_BYTES {
        return Err(AppError::payload_too_large("Message body is too large"));
    }
    if req
        .to
        .iter()
        .chain(req.cc.iter())
        .chain(req.bcc.iter())
        .any(|address| address.trim().is_empty() || address.len() > 512)
    {
        return Err(AppError::bad_request("Recipient addresses are invalid"));
    }
    Ok(())
}

fn email_error_to_app_error(err: EmailError) -> AppError {
    match err {
        EmailError::SmtpNotConfigured => AppError::service_unavailable("SMTP is not configured"),
        EmailError::SmtpSendFailed(message)
            if message.starts_with("Invalid email address")
                || message.starts_with("At least one recipient") =>
        {
            AppError::bad_request(message)
        }
        EmailError::SmtpSendFailed(_) => AppError::bad_gateway("SMTP send failed"),
        EmailError::DecryptFailed | EmailError::InvalidTlsMode(_) => {
            AppError::internal("SMTP configuration is invalid")
        }
    }
}

async fn emit_mail_message_sent(
    state: &AppState,
    sent_by: rustshare_core::domain::UserId,
    req: &SendMailMessageRequest,
) -> Result<(), AppError> {
    use rustshare_core::events::{AggregateType, Event, EventType, MailMessageSentPayload};
    let payload = MailMessageSentPayload {
        sent_by,
        to_count: req.to.len(),
        cc_count: req.cc.len(),
        bcc_count: req.bcc.len(),
    };
    let event = Event::new(
        EventType::MailMessageSent,
        sent_by,
        AggregateType::User,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        sent_by,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(())
}

#[derive(Debug, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct CreateOrUpdateSmtpSettingsRequest {
    #[validate(length(min = 1, max = 255))]
    pub host: String,
    #[validate(range(min = 1, max = 65535))]
    pub port: i32,
    #[validate(length(min = 1, max = 512))]
    pub username: String,
    pub password: Option<String>,
    #[validate(custom(function = "validate_smtp_tls_mode"))]
    pub tls_mode: MailTlsMode,
    #[validate(email)]
    pub from_address: String,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    pub sent_folder: Option<String>,
    pub is_enabled: bool,
}

fn validate_smtp_tls_mode(tls_mode: &MailTlsMode) -> Result<(), validator::ValidationError> {
    if *tls_mode == MailTlsMode::None {
        return Err(validator::ValidationError::new("plaintext_smtp_disallowed"));
    }
    Ok(())
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailSmtpSettingsResponse {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    #[schema(value_type = Uuid)]
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub owner_id: Uuid,
    #[schema(value_type = Uuid)]
    pub mail_account_id: Uuid,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub tls_mode: String,
    pub from_address: String,
    pub from_name: Option<String>,
    pub reply_to: Option<String>,
    pub sent_folder: Option<String>,
    pub is_enabled: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SendOutboundMailRequest {
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub body_html: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Uuid>,
    pub in_reply_to_msg_id: Option<Uuid>,
    /// Raw Message-ID of the message being replied to, for replies to messages
    /// that were never imported into RustShare. Ignored when
    /// `in_reply_to_msg_id` resolves to a stored message, and for forwards.
    #[serde(default)]
    pub in_reply_to: Option<String>,
    /// Raw References chain (Message-IDs) for remote-only replies.
    #[serde(default)]
    pub references: Option<Vec<String>>,
    pub idempotency_key: Option<Uuid>,
}

const MAX_THREADING_REFERENCES: usize = 20;
const MAX_THREADING_MESSAGE_ID_LEN: usize = 255;

fn validate_threading_field(value: &str) -> Result<(), AppError> {
    if value.len() > MAX_THREADING_MESSAGE_ID_LEN {
        return Err(AppError::bad_request("Message-ID is too long"));
    }
    if value.contains(['\r', '\n']) {
        return Err(AppError::bad_request("Message-ID must not contain CR/LF"));
    }
    Ok(())
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SendMailResponse {
    pub message_id: Option<Uuid>,
    pub stored: bool,
    pub append_failed: bool,
}

impl From<crate::services::mail_service::SentMail> for SendMailResponse {
    fn from(sent: crate::services::mail_service::SentMail) -> Self {
        Self {
            message_id: sent.message.as_ref().map(|message| message.id),
            stored: sent.message.is_some(),
            append_failed: sent.append_failed,
        }
    }
}

fn validate_send_outbound_mail_request(req: &SendOutboundMailRequest) -> Result<(), AppError> {
    let recipient_count = req.to.len() + req.cc.len() + req.bcc.len();
    if recipient_count == 0 {
        return Err(AppError::bad_request("At least one recipient is required"));
    }
    if recipient_count > MAX_MAIL_SEND_RECIPIENTS {
        return Err(AppError::bad_request(format!(
            "At most {MAX_MAIL_SEND_RECIPIENTS} recipients are allowed"
        )));
    }
    if req.subject.trim().is_empty() {
        return Err(AppError::bad_request("Subject is required"));
    }
    if req.subject.len() > 998 {
        return Err(AppError::bad_request("Subject is too long"));
    }
    if req.body.trim().is_empty() {
        return Err(AppError::bad_request("Body is required"));
    }
    if req.body.len() > MAX_MAIL_SEND_BODY_BYTES {
        return Err(AppError::payload_too_large("Message body is too large"));
    }
    if req.attachments.len() > MAX_MAIL_SEND_ATTACHMENTS {
        return Err(AppError::bad_request(format!(
            "At most {MAX_MAIL_SEND_ATTACHMENTS} attachments are allowed"
        )));
    }
    if req
        .to
        .iter()
        .chain(req.cc.iter())
        .chain(req.bcc.iter())
        .any(|address| address.trim().is_empty() || address.len() > 512)
    {
        return Err(AppError::bad_request("Recipient addresses are invalid"));
    }
    if let Some(in_reply_to) = &req.in_reply_to {
        validate_threading_field(in_reply_to)?;
    }
    if let Some(references) = &req.references {
        if references.len() > MAX_THREADING_REFERENCES {
            return Err(AppError::bad_request(format!(
                "At most {MAX_THREADING_REFERENCES} references are allowed"
            )));
        }
        for reference in references {
            validate_threading_field(reference)?;
        }
    }
    Ok(())
}

fn remove_reply_all_sender(req: &mut SendOutboundMailRequest, from_address: &str) {
    let from = from_address.trim().to_ascii_lowercase();
    if from.is_empty() {
        return;
    }

    let mut seen = std::collections::HashSet::new();
    let mut clean = |addresses: Vec<String>| {
        addresses
            .into_iter()
            .filter(|address| {
                let normalized = address.trim().to_ascii_lowercase();
                normalized != from && seen.insert(normalized)
            })
            .collect()
    };

    req.to = clean(std::mem::take(&mut req.to));
    req.cc = clean(std::mem::take(&mut req.cc));
    req.bcc = clean(std::mem::take(&mut req.bcc));
}

/// Get SMTP settings for a mail account.
#[utoipa::path(
    get,
    path = "/api/v1/mail/accounts/{id}/smtp",
    tag = "Mail",
    responses(
        (status = 200, description = "SMTP settings summary", body = MailSmtpSettingsResponse),
        (status = 404, description = "Not found"),
    ),
)]
pub async fn get_smtp_settings_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
) -> Result<Json<MailSmtpSettingsResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;

    let settings = state
        .mail_service
        .get_smtp_settings(auth.tenant_id, auth.user_id, account_id)
        .await?
        .ok_or_else(|| AppError::not_found("SMTP settings not configured"))?;

    Ok(Json(MailSmtpSettingsResponse {
        id: settings.id,
        tenant_id: settings.tenant_id,
        owner_id: settings.owner_id,
        mail_account_id: settings.mail_account_id,
        host: settings.host,
        port: settings.port,
        username: settings.username,
        tls_mode: settings.tls_mode,
        from_address: settings.from_address,
        from_name: settings.from_name,
        reply_to: settings.reply_to,
        sent_folder: settings.sent_folder,
        is_enabled: settings.is_enabled,
    }))
}

/// Create or update SMTP settings for a mail account.
#[utoipa::path(
    put,
    path = "/api/v1/mail/accounts/{id}/smtp",
    tag = "Mail",
    request_body = CreateOrUpdateSmtpSettingsRequest,
    responses(
        (status = 200, description = "SMTP settings updated", body = MailSmtpSettingsResponse),
    ),
)]
pub async fn create_or_update_smtp_settings_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<CreateOrUpdateSmtpSettingsRequest>,
) -> Result<Json<MailSmtpSettingsResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;

    let settings = state
        .mail_service
        .create_or_update_smtp_settings(
            auth.tenant_id,
            auth.user_id,
            account_id,
            req.host,
            req.port,
            req.username,
            req.password,
            req.tls_mode,
            req.from_address,
            req.from_name,
            req.reply_to,
            req.sent_folder,
            req.is_enabled,
        )
        .await?;

    Ok(Json(MailSmtpSettingsResponse {
        id: settings.id,
        tenant_id: settings.tenant_id,
        owner_id: settings.owner_id,
        mail_account_id: settings.mail_account_id,
        host: settings.host,
        port: settings.port,
        username: settings.username,
        tls_mode: settings.tls_mode,
        from_address: settings.from_address,
        from_name: settings.from_name,
        reply_to: settings.reply_to,
        sent_folder: settings.sent_folder,
        is_enabled: settings.is_enabled,
    }))
}

/// Delete SMTP settings for a mail account.
#[utoipa::path(
    delete,
    path = "/api/v1/mail/accounts/{id}/smtp",
    tag = "Mail",
    responses(
        (status = 204, description = "SMTP settings deleted"),
    ),
)]
pub async fn delete_smtp_settings_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;

    state
        .mail_service
        .delete_smtp_settings(auth.tenant_id, auth.user_id, account_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Test SMTP connection for a mail account.
#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/smtp/test",
    tag = "Mail",
    responses(
        (status = 200, description = "Connection test successful"),
    ),
)]
pub async fn test_smtp_connection_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
) -> Result<Json<SendMailMessageResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;

    state
        .mail_service
        .test_smtp_connection(auth.tenant_id, auth.user_id, account_id)
        .await?;

    Ok(Json(SendMailMessageResponse { ok: true }))
}

/// Send outbound mail.
#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/send",
    tag = "Mail",
    request_body = SendOutboundMailRequest,
    responses(
        (status = 200, description = "Mail sent", body = SendMailResponse),
    ),
)]
pub async fn send_outbound_mail_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
    Json(req): Json<SendOutboundMailRequest>,
) -> Result<Json<SendMailResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    validate_send_outbound_mail_request(&req)?;

    let msg = state
        .mail_service
        .send_outbound_mail(
            auth.tenant_id,
            auth.user_id,
            account_id,
            req.to,
            req.cc,
            req.bcc,
            req.subject,
            req.body,
            req.body_html,
            req.attachments,
            req.in_reply_to_msg_id,
            req.in_reply_to,
            req.references,
            false,
            req.idempotency_key,
        )
        .await?;

    Ok(Json(msg.into()))
}

/// Reply to mail.
#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/reply",
    tag = "Mail",
    request_body = SendOutboundMailRequest,
    responses(
        (status = 200, description = "Reply sent", body = SendMailResponse),
    ),
)]
pub async fn reply_mail_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
    Json(req): Json<SendOutboundMailRequest>,
) -> Result<Json<SendMailResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    validate_send_outbound_mail_request(&req)?;

    let msg = state
        .mail_service
        .send_outbound_mail(
            auth.tenant_id,
            auth.user_id,
            account_id,
            req.to,
            req.cc,
            req.bcc,
            req.subject,
            req.body,
            req.body_html,
            req.attachments,
            req.in_reply_to_msg_id,
            req.in_reply_to,
            req.references,
            false,
            req.idempotency_key,
        )
        .await?;

    Ok(Json(msg.into()))
}

/// Reply all to mail.
#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/reply-all",
    tag = "Mail",
    request_body = SendOutboundMailRequest,
    responses(
        (status = 200, description = "Reply all sent", body = SendMailResponse),
    ),
)]
pub async fn reply_all_mail_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
    Json(mut req): Json<SendOutboundMailRequest>,
) -> Result<Json<SendMailResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    if let Some(settings) = state
        .mail_service
        .get_smtp_settings(auth.tenant_id, auth.user_id, account_id)
        .await?
    {
        remove_reply_all_sender(&mut req, &settings.from_address);
    }
    validate_send_outbound_mail_request(&req)?;

    let msg = state
        .mail_service
        .send_outbound_mail(
            auth.tenant_id,
            auth.user_id,
            account_id,
            req.to,
            req.cc,
            req.bcc,
            req.subject,
            req.body,
            req.body_html,
            req.attachments,
            req.in_reply_to_msg_id,
            req.in_reply_to,
            req.references,
            false,
            req.idempotency_key,
        )
        .await?;

    Ok(Json(msg.into()))
}

/// Forward mail.
#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/forward",
    tag = "Mail",
    request_body = SendOutboundMailRequest,
    responses(
        (status = 200, description = "Forward sent", body = SendMailResponse),
    ),
)]
pub async fn forward_mail_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
    Json(req): Json<SendOutboundMailRequest>,
) -> Result<Json<SendMailResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    validate_send_outbound_mail_request(&req)?;

    let msg = state
        .mail_service
        .send_outbound_mail(
            auth.tenant_id,
            auth.user_id,
            account_id,
            req.to,
            req.cc,
            req.bcc,
            req.subject,
            req.body,
            req.body_html,
            req.attachments,
            None,
            None,
            None,
            true,
            req.idempotency_key,
        )
        .await?;

    Ok(Json(msg.into()))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SaveDraftRequest {
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub body_html: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Uuid>,
    pub in_reply_to_msg_id: Option<Uuid>,
    /// Raw Message-ID / References for replies to remote-only messages; kept
    /// with the draft so threading survives save/reload. See
    /// `SendOutboundMailRequest`.
    #[serde(default)]
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub references: Option<Vec<String>>,
}

/// Create draft mail.
#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/drafts",
    tag = "Mail",
    request_body = SaveDraftRequest,
    responses(
        (status = 200, description = "Draft created", body = MailMessageResponse),
    ),
)]
pub async fn create_draft_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
    Json(req): Json<SaveDraftRequest>,
) -> Result<Json<MailMessageResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let msg = state
        .mail_service
        .save_draft(
            auth.tenant_id,
            auth.user_id,
            account_id,
            None,
            req.to,
            req.cc,
            req.bcc,
            req.subject,
            req.body,
            req.body_html,
            req.attachments,
            req.in_reply_to_msg_id,
            req.in_reply_to,
            req.references,
        )
        .await?;
    Ok(Json(MailMessageResponse::from(msg)))
}

/// Update draft mail.
#[utoipa::path(
    put,
    path = "/api/v1/mail/accounts/{id}/drafts/{draft_id}",
    tag = "Mail",
    request_body = SaveDraftRequest,
    responses(
        (status = 200, description = "Draft updated", body = MailMessageResponse),
    ),
)]
pub async fn update_draft_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, draft_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<SaveDraftRequest>,
) -> Result<Json<MailMessageResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let msg = state
        .mail_service
        .save_draft(
            auth.tenant_id,
            auth.user_id,
            account_id,
            Some(draft_id),
            req.to,
            req.cc,
            req.bcc,
            req.subject,
            req.body,
            req.body_html,
            req.attachments,
            req.in_reply_to_msg_id,
            req.in_reply_to,
            req.references,
        )
        .await?;
    Ok(Json(MailMessageResponse::from(msg)))
}

/// Discard draft mail.
#[utoipa::path(
    delete,
    path = "/api/v1/mail/accounts/{id}/drafts/{draft_id}",
    tag = "Mail",
    responses(
        (status = 200, description = "Draft discarded"),
    ),
)]
pub async fn discard_draft_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, draft_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    state
        .mail_service
        .discard_draft(auth.tenant_id, auth.user_id, account_id, draft_id)
        .await?;
    Ok(StatusCode::OK)
}

/// Send draft mail.
#[utoipa::path(
    post,
    path = "/api/v1/mail/accounts/{id}/drafts/{draft_id}/send",
    tag = "Mail",
    responses(
        (status = 200, description = "Draft sent", body = SendMailResponse),
    ),
)]
pub async fn send_draft_handler(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((account_id, draft_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<SendMailResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;
    let msg = state
        .mail_service
        .send_draft(auth.tenant_id, auth.user_id, account_id, draft_id)
        .await?;
    Ok(Json(msg.into()))
}

#[cfg(test)]
mod tests {
    use super::{
        remove_reply_all_sender, require_destination_folder, rewrite_cid_urls, sanitize_email_html,
        validate_send_outbound_mail_request, CreateMailImportJobRequest,
        CreateOrUpdateSmtpSettingsRequest, SendOutboundMailRequest,
    };
    use chrono::Utc;
    use uuid::Uuid;
    use validator::Validate;

    #[test]
    fn archive_and_trash_require_an_explicit_destination() {
        assert!(require_destination_folder(None).is_err());
        assert!(require_destination_folder(Some("  ")).is_err());
        assert_eq!(
            require_destination_folder(Some("[Gmail]/All Mail")).unwrap(),
            "[Gmail]/All Mail"
        );
    }

    #[test]
    fn smtp_settings_request_rejects_plaintext_tls_mode() {
        let req: CreateOrUpdateSmtpSettingsRequest = serde_json::from_value(serde_json::json!({
            "host": "smtp.example.com",
            "port": 587,
            "username": "alice@example.com",
            "password": "secret",
            "tls_mode": "none",
            "from_address": "alice@example.com",
            "is_enabled": true
        }))
        .expect("request should deserialize");

        let err = req
            .validate()
            .expect_err("plaintext SMTP should fail validation");
        assert!(err.field_errors().contains_key("tls_mode"));
    }

    #[test]
    fn outbound_request_accepts_raw_remote_threading_headers() {
        let req: SendOutboundMailRequest = serde_json::from_value(serde_json::json!({
            "to": ["alice@example.com"],
            "subject": "Re: hello",
            "body": "reply",
            "in_reply_to": "abc123@remote.example",
            "references": ["<root@remote.example>", "abc123@remote.example"]
        }))
        .expect("request should deserialize");

        assert!(validate_send_outbound_mail_request(&req).is_ok());
        assert_eq!(req.in_reply_to.as_deref(), Some("abc123@remote.example"));
        assert_eq!(req.references.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn outbound_request_rejects_threading_header_injection() {
        let req: SendOutboundMailRequest = serde_json::from_value(serde_json::json!({
            "to": ["alice@example.com"],
            "subject": "Re: hello",
            "body": "reply",
            "in_reply_to": "abc@example.com\r\nBcc: attacker@example.com"
        }))
        .expect("request should deserialize");

        assert!(validate_send_outbound_mail_request(&req).is_err());
    }

    #[test]
    fn outbound_request_rejects_too_many_references() {
        let req: SendOutboundMailRequest = serde_json::from_value(serde_json::json!({
            "to": ["alice@example.com"],
            "subject": "Re: hello",
            "body": "reply",
            "references": (0..21).map(|i| format!("id-{i}@example.com")).collect::<Vec<_>>()
        }))
        .expect("request should deserialize");

        assert!(validate_send_outbound_mail_request(&req).is_err());
    }

    #[test]
    fn reply_all_removes_authoritative_sender_and_duplicates() {
        let mut req = SendOutboundMailRequest {
            to: vec![
                "alice@example.com".to_string(),
                "BOB@example.com".to_string(),
                "bob@example.com".to_string(),
            ],
            cc: vec![
                "Alice@Example.com".to_string(),
                "carol@example.com".to_string(),
            ],
            bcc: vec![
                "carol@example.com".to_string(),
                "dave@example.com".to_string(),
            ],
            subject: "Re: hello".to_string(),
            body: "hello".to_string(),
            body_html: None,
            attachments: Vec::new(),
            in_reply_to_msg_id: None,
            in_reply_to: None,
            references: None,
            idempotency_key: None,
        };

        remove_reply_all_sender(&mut req, "alice@example.com");

        assert_eq!(req.to, vec!["BOB@example.com"]);
        assert_eq!(req.cc, vec!["carol@example.com"]);
        assert_eq!(req.bcc, vec!["dave@example.com"]);
    }

    #[test]
    fn import_job_request_allows_null_uidvalidity() {
        let req: CreateMailImportJobRequest = serde_json::from_value(serde_json::json!({
            "folder_name": "INBOX",
            "source_uidvalidity": null,
            "selected_uids": [1]
        }))
        .expect("request should deserialize");

        assert!(req.validate().is_ok());
    }

    #[test]
    fn import_job_request_allows_omitted_uidvalidity() {
        let req: CreateMailImportJobRequest = serde_json::from_value(serde_json::json!({
            "folder_name": "INBOX",
            "selected_uids": [1]
        }))
        .expect("request should deserialize when source_uidvalidity is omitted");

        assert!(req.source_uidvalidity.is_none());
        assert!(req.validate().is_ok());
    }

    #[test]
    fn import_job_request_accepts_positive_uidvalidity() {
        let req: CreateMailImportJobRequest = serde_json::from_value(serde_json::json!({
            "folder_name": "INBOX",
            "source_uidvalidity": 1,
            "selected_uids": [1]
        }))
        .expect("request should deserialize");

        assert!(req.validate().is_ok());
    }

    #[test]
    fn import_job_request_rejects_non_positive_uidvalidity() {
        let req: CreateMailImportJobRequest = serde_json::from_value(serde_json::json!({
            "folder_name": "INBOX",
            "source_uidvalidity": 0,
            "selected_uids": [1]
        }))
        .expect("request should deserialize");

        let err = req.validate().expect_err("zero should fail validation");
        assert!(
            err.field_errors().contains_key("source_uidvalidity"),
            "error should be on source_uidvalidity"
        );
    }

    #[test]
    fn import_job_request_rejects_negative_uidvalidity() {
        let req: CreateMailImportJobRequest = serde_json::from_value(serde_json::json!({
            "folder_name": "INBOX",
            "source_uidvalidity": -1,
            "selected_uids": [1]
        }))
        .expect("request should deserialize");

        let err = req
            .validate()
            .expect_err("negative value should fail validation");
        assert!(
            err.field_errors().contains_key("source_uidvalidity"),
            "error should be on source_uidvalidity"
        );
    }

    #[test]
    fn sanitize_email_html_strips_scripts() {
        let raw =
            r#"<p>Hello</p><script>alert('xss')</script><a href="javascript:bad()">click</a>"#;
        let (clean, _) = sanitize_email_html(raw, false);
        assert!(!clean.contains("<script>"));
        assert!(!clean.contains("javascript:"));
        assert!(clean.contains("<p>Hello</p>"));
    }

    #[test]
    fn sanitize_email_html_blocks_remote_images() {
        let raw = r#"<p>Hello</p><img alt="tracker" src="https://tracker.example/pixel.gif"><img alt="upper" src="HTTPS://tracker.example/upper.gif"><img src="//tracker.example/relative.gif"><img src="cid:inline">"#;
        let (clean, blocked) = sanitize_email_html(raw, false);
        assert!(blocked, "blocked flag should report removed remote images");
        assert!(!clean.contains("https://tracker.example"));
        assert!(!clean.contains("HTTPS://tracker.example"));
        assert!(!clean.contains("//tracker.example"));
        assert!(clean.contains(r#"<img alt="tracker">"#));
        assert!(clean.contains(r#"<img alt="upper">"#));
        assert!(clean.contains(r#"<img src="cid:inline">"#));
    }

    #[test]
    fn sanitize_email_html_reports_no_block_without_remote_images() {
        let raw = r#"<p>Hello</p><img src="cid:inline">"#;
        let (clean, blocked) = sanitize_email_html(raw, false);
        assert!(!blocked, "no remote images means no blocked flag");
        assert!(clean.contains(r#"<img src="cid:inline">"#));
    }

    #[test]
    fn sanitize_email_html_explicit_load_keeps_remote_images() {
        let raw = r#"<p>Hello</p><img alt="tracker" src="https://tracker.example/pixel.gif"><img src="//tracker.example/relative.gif">"#;
        let (clean, blocked) = sanitize_email_html(raw, true);
        assert!(!blocked, "allow mode never reports blocked images");
        assert!(clean.contains(r#"<img alt="tracker" src="https://tracker.example/pixel.gif">"#));
        assert!(clean.contains(r#"<img src="//tracker.example/relative.gif">"#));
    }

    #[test]
    fn sanitize_email_html_strips_scripts_in_allow_mode() {
        let raw = r#"<img src="https://tracker.example/pixel.gif" onerror="alert(1)"><script>alert('xss')</script>"#;
        let (clean, _) = sanitize_email_html(raw, true);
        assert!(!clean.contains("<script>"));
        assert!(!clean.contains("onerror"));
        assert!(clean.contains("https://tracker.example"));
    }

    #[test]
    fn sanitize_email_html_strips_srcset_in_both_modes() {
        let raw = r#"<img src="cid:inline" srcset="https://tracker.example/pixel.gif 1x, https://tracker.example/pixel@2x.gif 2x">"#;
        let (blocked_mode, _) = sanitize_email_html(raw, false);
        let (allow_mode, _) = sanitize_email_html(raw, true);
        assert!(!blocked_mode.contains("srcset"));
        assert!(!allow_mode.contains("srcset"));
        assert!(!blocked_mode.contains("tracker.example"));
        assert!(!allow_mode.contains("tracker.example"));
    }

    #[test]
    fn sanitize_email_html_blocks_tracking_pixel() {
        let raw =
            r#"<p>Hi</p><img src="https://tracker.example/open.gif" width="1" height="1" alt="">"#;
        let (clean, blocked) = sanitize_email_html(raw, false);
        assert!(blocked, "tracking pixel should be reported as blocked");
        assert!(!clean.contains("tracker.example"));
    }

    #[test]
    fn cid_urls_rewrite_to_authenticated_file_preview() {
        let file_id = Uuid::new_v4();
        let attachment = rustshare_core::domain::MailAttachment {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            message_id: Uuid::new_v4(),
            file_id: Some(file_id),
            filename: "logo.png".to_string(),
            mime_type: Some("image/png".to_string()),
            size_bytes: Some(5),
            part_index: None,
            content_disposition: Some("inline".to_string()),
            content_id: Some("logo".to_string()),
            blob_key: None,
            created_at: Utc::now(),
        };

        assert_eq!(
            rewrite_cid_urls(r#"<img src="cid:logo">"#, &[attachment]),
            format!(r#"<img src="/api/v1/files/{file_id}/preview">"#)
        );
    }
}
