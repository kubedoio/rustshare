use super::{pagination, PaginationQuery};
use crate::{
    handlers::{
        admin::{
            admin_bad_request, admin_conflict, admin_not_found, insert_scoped_admin_action,
            request_correlation_id,
        },
        AppError, OrganizationAdmin,
    },
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

const RESEND_STALE_AFTER_MINUTES: i64 = 5;

#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
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

#[derive(sqlx::FromRow)]
struct InvitationRow {
    token: String,
    recipient_email: String,
    recipient_display_name: Option<String>,
    sender_name: String,
    expires_at: DateTime<Utc>,
    used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    cancelled_at: Option<DateTime<Utc>>,
    bulk_delivery_status: String,
    bulk_delivery_attempted_at: Option<DateTime<Utc>>,
    workflow_subject: Option<String>,
    workflow_body: Option<String>,
    workflow_status: String,
    db_now: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ResendAttempt {
    status: String,
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
    let invitations = sqlx::query_as::<_, OrganizationInvitation>(
        "SELECT it.id, it.recipient_email, it.recipient_display_name,
                sender.display_name AS sender_name,
                CASE
                    WHEN it.used_at IS NOT NULL THEN 'accepted'
                    WHEN it.cancelled_at IS NOT NULL THEN 'cancelled'
                    WHEN it.revoked_at IS NOT NULL AND it.expires_at > clock_timestamp() THEN 'cancelled'
                    WHEN it.expires_at <= clock_timestamp() THEN 'expired'
                    WHEN it.bulk_delivery_status = 'failed' THEN 'failed'
                    WHEN it.bulk_delivery_status = 'sending' THEN 'pending'
                    ELSE 'sent'
                END AS status,
                it.created_at, it.expires_at,
                it.bulk_delivery_attempted_at AS delivery_attempted_at,
                it.used_at AS accepted_at, it.cancelled_at
         FROM invite_tokens it
         JOIN users sender ON sender.id = it.sender_id
         WHERE it.tenant_id = $1
         ORDER BY it.created_at DESC, it.id DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(tenant_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db_pool)
    .await?;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM invite_tokens WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&state.db_pool)
        .await?;

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

    let mut tx = state.db_pool.begin().await?;
    let mut invitation = lock_invitation(&mut tx, tenant_id, invitation_id).await?;

    if let Some(attempt) = sqlx::query_as::<_, ResendAttempt>(
        "SELECT status FROM organization_invite_resend_attempts
         WHERE tenant_id=$1 AND invite_token_id=$2 AND idempotency_key_hash=$3",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .bind(idempotency_hash.as_slice())
    .fetch_optional(&mut *tx)
    .await?
    {
        let delivery_status = if attempt.status == "sending" {
            "pending".to_string()
        } else {
            attempt.status
        };
        let invitation_status = invitation_status(&invitation);
        tx.commit().await?;
        return Ok(Json(ResendInvitationResponse {
            invitation_id,
            delivery_status,
            invitation_status,
            replayed: true,
        }));
    }

    let stale_resends = sqlx::query(
        "UPDATE organization_invite_resend_attempts
         SET status='failed', completed_at=clock_timestamp()
         WHERE tenant_id=$1 AND invite_token_id=$2 AND status='sending'
           AND requested_at <= clock_timestamp() - ($3 * interval '1 minute')",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .bind(RESEND_STALE_AFTER_MINUTES as f64)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if stale_resends > 0 {
        sqlx::query(
            "UPDATE invite_tokens SET bulk_delivery_status='failed'
             WHERE tenant_id=$1 AND id=$2 AND used_at IS NULL AND revoked_at IS NULL",
        )
        .bind(tenant_id)
        .bind(invitation_id)
        .execute(&mut *tx)
        .await?;
        insert_scoped_admin_action(
            &mut tx,
            tenant_id,
            user_id,
            "organization_admin.invitation_resend_recovered",
            Some("invite"),
            Some(invitation_id),
            json!({"count": stale_resends}),
            correlation_id,
        )
        .await?;
        invitation = load_invitation(&mut tx, tenant_id, invitation_id).await?;
    }

    if invitation.bulk_delivery_status == "sending" {
        let stale_bulk_delivery = invitation
            .bulk_delivery_attempted_at
            .map(|attempted_at| {
                attempted_at
                    <= invitation.db_now - chrono::Duration::minutes(RESEND_STALE_AFTER_MINUTES)
            })
            .unwrap_or(true);
        if !stale_bulk_delivery {
            return Err(admin_conflict("Invitation delivery is already in progress"));
        }
        sqlx::query(
            "UPDATE invite_tokens SET bulk_delivery_status='failed'
             WHERE tenant_id=$1 AND id=$2 AND used_at IS NULL AND revoked_at IS NULL",
        )
        .bind(tenant_id)
        .bind(invitation_id)
        .execute(&mut *tx)
        .await?;
        insert_scoped_admin_action(
            &mut tx,
            tenant_id,
            user_id,
            "organization_admin.invitation_resend_recovered",
            Some("invite"),
            Some(invitation_id),
            json!({"count": 1}),
            correlation_id,
        )
        .await?;
        invitation = load_invitation(&mut tx, tenant_id, invitation_id).await?;
    }

    if invitation_status(&invitation) == "expired"
        || invitation_status(&invitation) == "accepted"
        || invitation_status(&invitation) == "cancelled"
    {
        return Err(admin_conflict("Invitation is no longer active"));
    }
    if invitation.bulk_delivery_status == "sending" {
        return Err(admin_conflict("Invitation delivery is already in progress"));
    }
    if invitation.workflow_status != "active" {
        return Err(admin_conflict("Invitation workflow is not active"));
    }

    let inserted = sqlx::query(
        "INSERT INTO organization_invite_resend_attempts
            (tenant_id, invite_token_id, actor_id, idempotency_key_hash, status)
         VALUES ($1,$2,$3,$4,'sending')",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .bind(user_id)
    .bind(idempotency_hash.as_slice())
    .execute(&mut *tx)
    .await;
    if let Err(sqlx::Error::Database(error)) = &inserted {
        if error.is_unique_violation() {
            return Err(admin_conflict("Invitation delivery is already in progress"));
        }
    }
    inserted?;
    sqlx::query(
        "UPDATE invite_tokens SET bulk_delivery_status='sending',
                bulk_delivery_attempted_at=clock_timestamp()
         WHERE tenant_id=$1 AND id=$2",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .execute(&mut *tx)
    .await?;
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        user_id,
        "organization_admin.invitation_resend_requested",
        Some("invite"),
        Some(invitation_id),
        json!({}),
        correlation_id,
    )
    .await?;
    tx.commit().await?;

    let invite_link = format!(
        "{}/invite/{}",
        state.public_base_url.trim_end_matches('/'),
        invitation.token
    );
    let email_service = rustshare_core::services::EmailService::new(
        state.db_pool.clone(),
        state.secret_key.clone(),
    );
    let sent = email_service
        .send_invite_email(
            &invitation.sender_name,
            invitation
                .recipient_display_name
                .as_deref()
                .unwrap_or(&invitation.recipient_email),
            &invitation.recipient_email,
            &invite_link,
            invitation
                .workflow_subject
                .as_deref()
                .unwrap_or("You've been invited to RustShare"),
            invitation.workflow_body.as_deref().unwrap_or(""),
        )
        .await
        .is_ok();

    let mut tx = state.db_pool.begin().await?;
    let current = lock_invitation(&mut tx, tenant_id, invitation_id).await?;
    let current_status = invitation_status(&current);
    let delivery_status = if current_status == "accepted"
        || current_status == "cancelled"
        || current_status == "expired"
    {
        current_status.clone()
    } else if sent {
        "sent".to_string()
    } else {
        "failed".to_string()
    };
    if current_status != "accepted" && current_status != "cancelled" && current_status != "expired"
    {
        sqlx::query(
            "UPDATE invite_tokens SET bulk_delivery_status=$3
             WHERE tenant_id=$1 AND id=$2 AND used_at IS NULL AND revoked_at IS NULL",
        )
        .bind(tenant_id)
        .bind(invitation_id)
        .bind(&delivery_status)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE organization_invite_resend_attempts
         SET status=$4, completed_at=clock_timestamp()
         WHERE tenant_id=$1 AND invite_token_id=$2 AND idempotency_key_hash=$3 AND status='sending'",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .bind(idempotency_hash.as_slice())
    .bind(&delivery_status)
    .execute(&mut *tx)
    .await?;
    let action_type = match delivery_status.as_str() {
        "sent" => "organization_admin.invitation_resend_sent",
        "failed" => "organization_admin.invitation_resend_failed",
        _ => "organization_admin.invitation_resend_superseded",
    };
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        user_id,
        action_type,
        Some("invite"),
        Some(invitation_id),
        json!({}),
        correlation_id,
    )
    .await?;
    tx.commit().await?;

    Ok(Json(ResendInvitationResponse {
        invitation_id,
        delivery_status: delivery_status.clone(),
        invitation_status: if matches!(delivery_status.as_str(), "sent" | "failed") {
            delivery_status
        } else {
            current_status
        },
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
    let mut tx = state.db_pool.begin().await?;
    let invitation = lock_invitation(&mut tx, tenant_id, invitation_id).await?;
    if invitation.used_at.is_some() {
        return Err(admin_conflict("Invitation has already been accepted"));
    }
    if invitation.cancelled_at.is_some() {
        tx.commit().await?;
        return Ok(Json(CancelInvitationResponse {
            invitation_id,
            status: "cancelled".to_string(),
            changed: false,
            group_assignments_removed: 0,
        }));
    }
    if invitation.expires_at <= invitation.db_now || invitation.revoked_at.is_some() {
        return Err(admin_conflict("Invitation is no longer active"));
    }

    let group_ids: Vec<Uuid> = sqlx::query_scalar(
        "DELETE FROM invite_token_group_assignments WHERE invite_token_id=$1 RETURNING group_id",
    )
    .bind(invitation_id)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE invite_tokens
         SET revoked_at=clock_timestamp(), cancelled_at=clock_timestamp()
         WHERE tenant_id=$1 AND id=$2 AND used_at IS NULL AND revoked_at IS NULL",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE organization_invite_resend_attempts
         SET status='cancelled', completed_at=clock_timestamp()
         WHERE tenant_id=$1 AND invite_token_id=$2 AND status='sending'",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .execute(&mut *tx)
    .await?;
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        user_id,
        "organization_admin.invitation_cancelled",
        Some("invite"),
        Some(invitation_id),
        json!({"group_assignments_removed": group_ids.len()}),
        request_correlation_id(&headers),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(CancelInvitationResponse {
        invitation_id,
        status: "cancelled".to_string(),
        changed: true,
        group_assignments_removed: group_ids.len() as u64,
    }))
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

async fn lock_invitation(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    invitation_id: Uuid,
) -> Result<InvitationRow, AppError> {
    let email: Option<String> = sqlx::query_scalar(
        "SELECT recipient_email FROM invite_tokens WHERE tenant_id=$1 AND id=$2",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .fetch_optional(&mut **tx)
    .await?;
    let email = email.ok_or_else(|| admin_not_found("Invitation not found"))?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended(lower($1), 331))")
        .bind(email)
        .execute(&mut **tx)
        .await?;
    load_invitation(tx, tenant_id, invitation_id).await
}

async fn load_invitation(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    invitation_id: Uuid,
) -> Result<InvitationRow, AppError> {
    sqlx::query_as::<_, InvitationRow>(
        "SELECT it.token, it.recipient_email,
                it.recipient_display_name, sender.display_name AS sender_name,
                it.expires_at, it.used_at, it.revoked_at,
                it.cancelled_at, it.bulk_delivery_status,
                it.bulk_delivery_attempted_at, workflow.subject AS workflow_subject,
                workflow.body AS workflow_body, workflow.status AS workflow_status,
                clock_timestamp() AS db_now
         FROM invite_tokens it
         JOIN users sender ON sender.id=it.sender_id
         JOIN workflows workflow ON workflow.id=it.workflow_id
         WHERE it.tenant_id=$1 AND it.id=$2
         FOR UPDATE OF it",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| admin_not_found("Invitation not found"))
}

fn invitation_status(invitation: &InvitationRow) -> String {
    if invitation.used_at.is_some() {
        "accepted".to_string()
    } else if invitation.cancelled_at.is_some()
        || (invitation.revoked_at.is_some() && invitation.expires_at > invitation.db_now)
    {
        "cancelled".to_string()
    } else if invitation.expires_at <= invitation.db_now {
        "expired".to_string()
    } else if invitation.bulk_delivery_status == "failed" {
        "failed".to_string()
    } else if invitation.bulk_delivery_status == "sending" {
        "pending".to_string()
    } else {
        "sent".to_string()
    }
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
