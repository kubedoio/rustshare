//! Tenant-scoped organization invitation lifecycle persistence.

use crate::services::organization_administrators::insert_scoped_admin_action;
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

const RESEND_STALE_AFTER_MINUTES: i64 = 5;

#[derive(sqlx::FromRow)]
pub(crate) struct InvitationSummary {
    pub(crate) id: Uuid,
    pub(crate) recipient_email: String,
    pub(crate) recipient_display_name: Option<String>,
    pub(crate) sender_name: String,
    pub(crate) status: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) delivery_attempted_at: Option<DateTime<Utc>>,
    pub(crate) accepted_at: Option<DateTime<Utc>>,
    pub(crate) cancelled_at: Option<DateTime<Utc>>,
}

pub(crate) struct ResendDelivery {
    pub(crate) token: String,
    pub(crate) recipient_email: String,
    pub(crate) recipient_display_name: Option<String>,
    pub(crate) sender_name: String,
    pub(crate) workflow_subject: Option<String>,
    pub(crate) workflow_body: Option<String>,
}

pub(crate) enum BeginResend {
    Replayed {
        delivery_status: String,
        invitation_status: String,
    },
    Delivery(ResendDelivery),
}

#[derive(Debug)]
pub(crate) enum InvitationError {
    Database(sqlx::Error),
    NotFound,
    AlreadyAccepted,
    NotActive,
    DeliveryInProgress,
    WorkflowInactive,
}

pub(crate) struct ResendResult {
    pub(crate) delivery_status: String,
    pub(crate) invitation_status: String,
}

pub(crate) enum CancelResult {
    Changed { group_assignments_removed: u64 },
    AlreadyCancelled,
}

pub(crate) async fn list(
    pool: &PgPool,
    tenant_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(i64, Vec<InvitationSummary>), sqlx::Error> {
    let invitations = sqlx::query_as::<_, InvitationSummary>(
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
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM invite_tokens WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;
    Ok((total, invitations))
}

pub(crate) async fn begin_resend(
    pool: &PgPool,
    tenant_id: Uuid,
    actor_id: Uuid,
    invitation_id: Uuid,
    idempotency_hash: &[u8],
    correlation_id: Option<&str>,
) -> Result<BeginResend, InvitationError> {
    let mut tx = pool.begin().await?;
    let mut invitation = lock_invitation(&mut tx, tenant_id, invitation_id).await?;

    if let Some(attempt) = sqlx::query_scalar::<_, String>(
        "SELECT status FROM organization_invite_resend_attempts
         WHERE tenant_id=$1 AND invite_token_id=$2 AND idempotency_key_hash=$3",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .bind(idempotency_hash)
    .fetch_optional(&mut *tx)
    .await?
    {
        let delivery_status = if attempt == "sending" {
            "pending".to_string()
        } else {
            attempt
        };
        let invitation_status = invitation_status(&invitation);
        tx.commit().await?;
        return Ok(BeginResend::Replayed {
            delivery_status,
            invitation_status,
        });
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
            actor_id,
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
            return Err(InvitationError::DeliveryInProgress);
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
            actor_id,
            "organization_admin.invitation_resend_recovered",
            Some("invite"),
            Some(invitation_id),
            json!({"count": 1}),
            correlation_id,
        )
        .await?;
        invitation = load_invitation(&mut tx, tenant_id, invitation_id).await?;
    }

    if matches!(
        invitation_status(&invitation).as_str(),
        "expired" | "accepted" | "cancelled"
    ) {
        return Err(InvitationError::NotActive);
    }
    if invitation.bulk_delivery_status == "sending" {
        return Err(InvitationError::DeliveryInProgress);
    }
    if invitation.workflow_status != "active" {
        return Err(InvitationError::WorkflowInactive);
    }

    let inserted = sqlx::query(
        "INSERT INTO organization_invite_resend_attempts
            (tenant_id, invite_token_id, actor_id, idempotency_key_hash, status)
         VALUES ($1,$2,$3,$4,'sending')",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .bind(actor_id)
    .bind(idempotency_hash)
    .execute(&mut *tx)
    .await;
    if let Err(sqlx::Error::Database(error)) = &inserted {
        if error.is_unique_violation() {
            return Err(InvitationError::DeliveryInProgress);
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
        actor_id,
        "organization_admin.invitation_resend_requested",
        Some("invite"),
        Some(invitation_id),
        json!({}),
        correlation_id,
    )
    .await?;
    tx.commit().await?;

    Ok(BeginResend::Delivery(ResendDelivery {
        token: invitation.token,
        recipient_email: invitation.recipient_email,
        recipient_display_name: invitation.recipient_display_name,
        sender_name: invitation.sender_name,
        workflow_subject: invitation.workflow_subject,
        workflow_body: invitation.workflow_body,
    }))
}

impl From<sqlx::Error> for InvitationError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

pub(crate) async fn finish_resend(
    pool: &PgPool,
    tenant_id: Uuid,
    actor_id: Uuid,
    invitation_id: Uuid,
    idempotency_hash: &[u8],
    correlation_id: Option<&str>,
    sent: bool,
) -> Result<ResendResult, InvitationError> {
    let mut tx = pool.begin().await?;
    let current = lock_invitation(&mut tx, tenant_id, invitation_id).await?;
    let current_status = invitation_status(&current);
    let delivery_status = if matches!(
        current_status.as_str(),
        "accepted" | "cancelled" | "expired"
    ) {
        current_status.clone()
    } else if sent {
        "sent".to_string()
    } else {
        "failed".to_string()
    };
    if !matches!(
        current_status.as_str(),
        "accepted" | "cancelled" | "expired"
    ) {
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
    .bind(idempotency_hash)
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
        actor_id,
        action_type,
        Some("invite"),
        Some(invitation_id),
        json!({}),
        correlation_id,
    )
    .await?;
    tx.commit().await?;

    Ok(ResendResult {
        invitation_status: if matches!(delivery_status.as_str(), "sent" | "failed") {
            delivery_status.clone()
        } else {
            current_status
        },
        delivery_status,
    })
}

pub(crate) async fn cancel(
    pool: &PgPool,
    tenant_id: Uuid,
    actor_id: Uuid,
    invitation_id: Uuid,
    correlation_id: Option<&str>,
) -> Result<CancelResult, InvitationError> {
    let mut tx = pool.begin().await?;
    let invitation = lock_invitation(&mut tx, tenant_id, invitation_id).await?;
    if invitation.used_at.is_some() {
        return Err(InvitationError::AlreadyAccepted);
    }
    if invitation.cancelled_at.is_some() {
        tx.commit().await?;
        return Ok(CancelResult::AlreadyCancelled);
    }
    if invitation.expires_at <= invitation.db_now || invitation.revoked_at.is_some() {
        return Err(InvitationError::NotActive);
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
        actor_id,
        "organization_admin.invitation_cancelled",
        Some("invite"),
        Some(invitation_id),
        json!({"group_assignments_removed": group_ids.len()}),
        correlation_id,
    )
    .await?;
    tx.commit().await?;

    Ok(CancelResult::Changed {
        group_assignments_removed: group_ids.len() as u64,
    })
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

async fn lock_invitation(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    invitation_id: Uuid,
) -> Result<InvitationRow, InvitationError> {
    let email: Option<String> = sqlx::query_scalar(
        "SELECT recipient_email FROM invite_tokens WHERE tenant_id=$1 AND id=$2",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .fetch_optional(&mut **tx)
    .await?;
    let email = email.ok_or(InvitationError::NotFound)?;
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
) -> Result<InvitationRow, InvitationError> {
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
    .ok_or(InvitationError::NotFound)
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
