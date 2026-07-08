use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use rustshare_core::domain::{LinkTargetType, MailTlsMode};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

use crate::handlers::{AppError, AuthenticatedUser};
use crate::services::module_service::ModuleError;
use crate::state::AppState;

const MAX_MAIL_UPLOAD_SIZE_BYTES: usize = 25 * 1024 * 1024;
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateMailAccountRequest {
    pub name: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    pub password: String,
    pub tls_mode: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateMailAccountRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i32>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls_mode: Option<String>,
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
pub struct MailFolderListResponse {
    pub folders: Vec<MailFolderResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailFolderResponse {
    pub name: String,
    pub delimiter: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailMessageSummaryResponse {
    pub uid: u32,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
    pub size_bytes: i64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateMailImportJobRequest {
    pub folder_name: String,
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
    responses(
        (status = 200, description = "Mail messages", body = ListMailMessagesResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_messages(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<ListMailMessagesResponse>, AppError> {
    require_mail_enabled(&state, auth.tenant_id).await?;

    let messages = state
        .mail_service
        .list_messages(auth.tenant_id, auth.user_id)
        .await?
        .into_iter()
        .map(MailMessageResponse::from)
        .collect();

    Ok(Json(ListMailMessagesResponse { messages }))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MailMessageResponse {
    pub id: Uuid,
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
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListMailMessagesResponse {
    pub messages: Vec<MailMessageResponse>,
}

impl From<rustshare_core::domain::MailMessage> for MailMessageResponse {
    fn from(msg: rustshare_core::domain::MailMessage) -> Self {
        Self {
            id: msg.id,
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
        delimiter: folder.delimiter,
    }
}

fn summary_to_response(
    summary: crate::services::imap_client::ImapMessageSummary,
) -> MailMessageSummaryResponse {
    MailMessageSummaryResponse {
        uid: summary.uid,
        subject: summary.subject,
        from_address: summary.from_address,
        from_name: summary.from_name,
        sent_at: summary.sent_at,
        size_bytes: summary.size_bytes,
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

fn parse_tls_mode(tls_mode: &str) -> Result<MailTlsMode, AppError> {
    MailTlsMode::from_str(tls_mode).map_err(|_| {
        AppError::bad_request(format!(
            "Invalid tls_mode: {tls_mode}. Expected one of: tls, starttls, none"
        ))
    })
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListMailMessagesQuery {
    folder: String,
    #[serde(default = "default_message_limit")]
    limit: i64,
}

fn default_message_limit() -> i64 {
    100
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
    Json(req): Json<CreateMailAccountRequest>,
) -> Result<(StatusCode, Json<MailAccountResponse>), AppError> {
    let tls_mode = parse_tls_mode(&req.tls_mode)?;
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
            tls_mode,
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
        (status = 200, description = "Mail accounts", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_mail_accounts(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let accounts = state
        .mail_service
        .list_accounts(auth.tenant_id, auth.user_id)
        .await?;

    Ok(Json(serde_json::json!({
        "accounts": accounts.into_iter().map(account_to_response).collect::<Vec<_>>(),
    })))
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
    Json(req): Json<UpdateMailAccountRequest>,
) -> Result<Json<MailAccountResponse>, AppError> {
    let tls_mode = req.tls_mode.as_deref().map(parse_tls_mode).transpose()?;

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
            tls_mode,
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
        (status = 200, description = "Connection successful", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn test_mail_account(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(account_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .mail_service
        .test_account_connection(auth.tenant_id, auth.user_id, account_id)
        .await?;

    Ok(Json(serde_json::json!({ "ok": true })))
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
        (status = 200, description = "Message summaries", body = serde_json::Value),
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
) -> Result<Json<serde_json::Value>, AppError> {
    if query.limit <= 0 || query.limit > 1000 {
        return Err(AppError::bad_request("limit must be between 1 and 1000"));
    }

    let messages = state
        .mail_service
        .list_imap_messages(
            auth.tenant_id,
            auth.user_id,
            account_id,
            &query.folder,
            query.limit as usize,
        )
        .await?;

    Ok(Json(serde_json::json!({
        "messages": messages.into_iter().map(summary_to_response).collect::<Vec<_>>(),
    })))
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
    Json(req): Json<CreateMailImportJobRequest>,
) -> Result<(StatusCode, Json<MailImportJobResponse>), AppError> {
    let job = state
        .mail_service
        .create_imap_import_job(
            auth.tenant_id,
            auth.user_id,
            account_id,
            req.folder_name,
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
    let job = state
        .mail_service
        .get_import_job(auth.tenant_id, auth.user_id, job_id)
        .await?;

    Ok(Json(job_to_response(job)))
}
