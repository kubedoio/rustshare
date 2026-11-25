use super::{pagination, PaginationQuery};
use crate::{
    handlers::{
        admin::{admin_bad_request, admin_conflict, admin_not_found, request_correlation_id},
        AppError, OrganizationAdmin,
    },
    services::{organization_invitations, EmailService},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrganizationInvitation {
    pub id: Uuid,
    pub recipient_email: String,
    pub recipient_display_name: Option<String>,
    pub sender_name: String,
    /// `pending`, `sent`, `failed`, `expired`, `accepted`, or `cancelled`.
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub delivery_attempted_at: Option<DateTime<Utc>>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrganizationInvitations {
    pub invitations: Vec<OrganizationInvitation>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ResendInvitationResponse {
    pub invitation_id: Uuid,
    /// `sent`, `failed`, `pending`, `expired`, `accepted`, or `cancelled`.
    pub delivery_status: String,
    pub invitation_status: String,
    pub replayed: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CancelInvitationResponse {
    pub invitation_id: Uuid,
    pub status: String,
    pub changed: bool,
    pub group_assignments_removed: u64,
}

#[utoipa::path(
    get,
    path = "/api/v1/organization-admin/invitations",
    tag = "Organization Admin",
    params(PaginationQuery),
    responses(
        (status = 200, description = "Tenant invitations with secret-free lifecycle state", body = OrganizationInvitations),
        (status = 400, body = crate::handlers::ErrorResponse),
        (status = 403, body = crate::handlers::ErrorResponse)
    )
)]
pub async fn list_invitations(
    State(state): State<AppState>,
    OrganizationAdmin { tenant_id, .. }: OrganizationAdmin,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<OrganizationInvitations>, AppError> {
    let (page, per_page, offset) = pagination(query.page, query.per_page)?;
    let (total, rows) =
        organization_invitations::list(&state.db_pool, tenant_id, per_page, offset).await?;
    let invitations = rows
        .into_iter()
        .map(|row| OrganizationInvitation {
            id: row.id,
            recipient_email: row.recipient_email,
            recipient_display_name: row.recipient_display_name,
            sender_name: row.sender_name,
            status: row.status,
            created_at: row.created_at,
            expires_at: row.expires_at,
            delivery_attempted_at: row.delivery_attempted_at,
            accepted_at: row.accepted_at,
            cancelled_at: row.cancelled_at,
        })
        .collect();

    Ok(Json(OrganizationInvitations {
        invitations,
        total,
        page,
        per_page,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/organization-admin/invitations/{id}/resend",
    tag = "Organization Admin",
    params(
        ("id" = Uuid, Path, description = "Invitation identifier"),
        ("Idempotency-Key" = String, Header, description = "Unique key for safe request retries")
    ),
    responses(
        (status = 200, body = ResendInvitationResponse),
        (status = 400, body = crate::handlers::ErrorResponse),
        (status = 403, body = crate::handlers::ErrorResponse),
        (status = 404, body = crate::handlers::ErrorResponse),
        (status = 409, body = crate::handlers::ErrorResponse)
    )
)]
pub async fn resend_invitation(
    State(state): State<AppState>,
    OrganizationAdmin { user_id, tenant_id }: OrganizationAdmin,
    Path(invitation_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ResendInvitationResponse>, AppError> {
    let idempotency_hash = idempotency_key_hash(&headers)?;
    let _permit = super::BULK_INVITE_DELIVERY_SEMAPHORE
        .acquire()
        .await
        .map_err(|_| AppError::internal("Invitation delivery is unavailable"))?;
    let correlation_id = request_correlation_id(&headers);
    let initiated = organization_invitations::begin_resend(
        &state.db_pool,
        tenant_id,
        user_id,
        invitation_id,
        &idempotency_hash,
        correlation_id,
    )
    .await
    .map_err(map_invitation_error)?;

    let delivery = match initiated {
        organization_invitations::BeginResend::Replayed {
            delivery_status,
            invitation_status,
        } => {
            return Ok(Json(ResendInvitationResponse {
                invitation_id,
                delivery_status,
                invitation_status,
                replayed: true,
            }));
        }
        organization_invitations::BeginResend::Delivery(delivery) => delivery,
    };

    let invite_link = format!(
        "{}/invite/{}",
        state.public_base_url.trim_end_matches('/'),
        delivery.token
    );
    let email_service = EmailService::new(state.db_pool.clone(), state.secret_key.clone());
    let sent = email_service
        .send_invite_email(
            &delivery.sender_name,
            delivery
                .recipient_display_name
                .as_deref()
                .unwrap_or(&delivery.recipient_email),
            &delivery.recipient_email,
            &invite_link,
            delivery
                .workflow_subject
                .as_deref()
                .unwrap_or("You've been invited to RustShare"),
            delivery.workflow_body.as_deref().unwrap_or(""),
        )
        .await
        .is_ok();

    let result = organization_invitations::finish_resend(
        &state.db_pool,
        tenant_id,
        user_id,
        invitation_id,
        &idempotency_hash,
        correlation_id,
        sent,
    )
    .await
    .map_err(map_invitation_error)?;

    Ok(Json(ResendInvitationResponse {
        invitation_id,
        delivery_status: result.delivery_status,
        invitation_status: result.invitation_status,
        replayed: false,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/organization-admin/invitations/{id}/cancel",
    tag = "Organization Admin",
    params(("id" = Uuid, Path, description = "Invitation identifier")),
    responses(
        (status = 200, body = CancelInvitationResponse),
        (status = 403, body = crate::handlers::ErrorResponse),
        (status = 404, body = crate::handlers::ErrorResponse),
        (status = 409, body = crate::handlers::ErrorResponse)
    )
)]
pub async fn cancel_invitation(
    State(state): State<AppState>,
    OrganizationAdmin { user_id, tenant_id }: OrganizationAdmin,
    Path(invitation_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<CancelInvitationResponse>, AppError> {
    let result = organization_invitations::cancel(
        &state.db_pool,
        tenant_id,
        user_id,
        invitation_id,
        request_correlation_id(&headers),
    )
    .await
    .map_err(map_invitation_error)?;

    match result {
        organization_invitations::CancelResult::AlreadyCancelled => {
            Ok(Json(CancelInvitationResponse {
                invitation_id,
                status: "cancelled".to_string(),
                changed: false,
                group_assignments_removed: 0,
            }))
        }
        organization_invitations::CancelResult::Changed {
            group_assignments_removed,
        } => Ok(Json(CancelInvitationResponse {
            invitation_id,
            status: "cancelled".to_string(),
            changed: true,
            group_assignments_removed,
        })),
    }
}

fn map_invitation_error(error: organization_invitations::InvitationError) -> AppError {
    use organization_invitations::InvitationError;
    match error {
        InvitationError::Database(error) => AppError::from(error),
        InvitationError::NotFound => admin_not_found("Invitation not found"),
        InvitationError::AlreadyAccepted => admin_conflict("Invitation has already been accepted"),
        InvitationError::NotActive => admin_conflict("Invitation is no longer active"),
        InvitationError::DeliveryInProgress => {
            admin_conflict("Invitation delivery is already in progress")
        }
        InvitationError::WorkflowInactive => admin_conflict("Invitation workflow is not active"),
    }
}

fn idempotency_key_hash(headers: &HeaderMap) -> Result<[u8; 32], AppError> {
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
                })
        })
        .ok_or_else(|| admin_bad_request("A valid Idempotency-Key header is required"))?;
    Ok(Sha256::digest(key.as_bytes()).into())
}

#[cfg(test)]
mod tests {
    use super::idempotency_key_hash;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn idempotency_key_hash_rejects_missing_or_malformed_values() {
        assert!(idempotency_key_hash(&HeaderMap::new()).is_err());
        let mut headers = HeaderMap::new();
        headers.insert("idempotency-key", HeaderValue::from_static("bad key"));
        assert!(idempotency_key_hash(&headers).is_err());
        headers.insert("idempotency-key", HeaderValue::from_static("request-123"));
        assert_eq!(
            idempotency_key_hash(&headers).unwrap(),
            idempotency_key_hash(&headers).unwrap()
        );
    }
}
