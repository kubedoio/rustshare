use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::handlers::{AppError, AuthenticatedUser};
use crate::state::AppState;

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
    let mut file_temp: Option<tempfile::NamedTempFile> = None;

    while let Some(mut field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Failed to read multipart field: {}", e);
        AppError::internal(format!("Failed to read multipart field: {e}"))
    })? {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "file" {
            file_temp = Some(
                super::stream_multipart_field_to_temp_file(
                    &mut field,
                    super::max_upload_size_bytes(),
                )
                .await?
                .0,
            );
        }
    }

    let file_temp = file_temp.ok_or_else(|| AppError::bad_request("Missing file data"))?;
    let raw = tokio::fs::read(file_temp.path())
        .await
        .map_err(|e| AppError::internal(format!("Failed to read uploaded file: {e}")))?;

    if raw.is_empty() {
        return Err(AppError::bad_request("Uploaded file is empty"));
    }

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

pub async fn list_mail_messages(
    State(_state): State<AppState>,
    _auth: AuthenticatedUser,
) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::OK, Json(serde_json::json!({ "messages": [] })))
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
    let msg = state
        .mail_service
        .get_message(auth.tenant_id, auth.user_id, message_id)
        .await?;

    Ok(Json(MailMessageResponse {
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
    }))
}
