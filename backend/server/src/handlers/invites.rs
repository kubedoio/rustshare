use axum::{
    extract::{Path, State},
    Json,
};
use rand::Rng;
use rustshare_auth::PasswordHasher;
use rustshare_core::services::EmailService;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    handlers::{AppError, AuthenticatedUser},
    AppState,
};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateInviteRequest {
    pub recipient_email: String,
    /// Kept for API compatibility; the server ignores it and always builds the
    /// emailed invite link from `RUSTSHARE_PUBLIC_URL` (see `create_invite`).
    pub origin: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CreateInviteResponse {
    pub token: String,
    pub invite_link: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    post,
    path = "/api/v1/invites",
    tag = "Invites",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_invite(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Json(req): Json<CreateInviteRequest>,
) -> Result<Json<CreateInviteResponse>, AppError> {
    let workflow = sqlx::query_as!(
        WorkflowRow,
        "SELECT id, subject, body, terms_enabled, terms_text
         FROM workflows
         WHERE key = 'invite_email' AND status = 'active'",
    )
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::bad_request("Invite workflow is not active"))?;

    let sender = sqlx::query_as!(
        SenderRow,
        "SELECT display_name FROM users WHERE id = $1",
        user_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    let mut token_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut token_bytes);
    let token = hex::encode(token_bytes);

    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    let tenant_id: Uuid = sqlx::query_scalar!("SELECT tenant_id FROM users WHERE id = $1", user_id)
        .fetch_one(&state.db_pool)
        .await?;

    let email = req.recipient_email.trim().to_ascii_lowercase();
    let mut tx = state.db_pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended(lower($1), 331))")
        .bind(&email)
        .execute(&mut *tx)
        .await?;
    let identity_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE lower(email)=lower($1) OR lower(username)=lower($1))",
    )
    .bind(&email)
    .fetch_one(&mut *tx)
    .await?;
    let invitation_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM invite_tokens WHERE lower(recipient_email)=lower($1) AND used_at IS NULL AND revoked_at IS NULL AND expires_at>now())",
    )
    .bind(&email)
    .fetch_one(&mut *tx)
    .await?;
    if identity_exists || invitation_exists {
        return Err(AppError::conflict("Invitation identifier is unavailable"));
    }
    sqlx::query!(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
        tenant_id,
        &token,
        user_id,
        &email,
        workflow.id,
        expires_at
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // The invite link must be built from the server's configured public URL,
    // never from client-supplied input: the link is emailed to a third party
    // by the server, so trusting the client origin would let an attacker
    // point the recipient at a phishing page carrying a valid token.
    let origin = state.public_base_url.trim_end_matches('/').to_string();
    let invite_link = format!("{}/invite/{}", origin, token);

    let email_service = EmailService::new(state.db_pool.clone(), state.secret_key.clone());
    let result = email_service
        .send_invite_email(
            &sender.display_name,
            &email,
            &email,
            &invite_link,
            workflow
                .subject
                .as_deref()
                .unwrap_or("You've been invited to RustShare"),
            workflow.body.as_deref().unwrap_or(""),
        )
        .await;

    if let Err(e) = result {
        sqlx::query!("DELETE FROM invite_tokens WHERE token = $1", &token)
            .execute(&state.db_pool)
            .await
            .ok();
        return Err(AppError::bad_gateway(format!(
            "Failed to send invite email: {}",
            e
        )));
    }

    Ok(Json(CreateInviteResponse {
        token,
        invite_link,
        expires_at,
    }))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct InviteDetailResponse {
    pub sender_name: String,
    pub recipient_email: String,
    pub subject: String,
    pub body: String,
    pub terms_enabled: bool,
    pub terms_text: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get,
    path = "/api/v1/invites/{token}",
    tag = "Invites",
    params(("token" = String, Path, description = "Token")),
    responses(
        (status = 200, description = "Success", body = InviteDetailResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_invite(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<InviteDetailResponse>, AppError> {
    let row = sqlx::query_as!(
        InviteTokenRow,
        "SELECT it.sender_id, it.recipient_email, it.expires_at, it.used_at, it.revoked_at,
                w.subject, w.body, w.terms_enabled, w.terms_text
         FROM invite_tokens it
         JOIN workflows w ON it.workflow_id = w.id
         WHERE it.token = $1",
        &token
    )
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::gone("Invite link has expired or already been used"))?;

    if row.used_at.is_some() || row.revoked_at.is_some() || row.expires_at < chrono::Utc::now() {
        return Err(AppError::gone(
            "Invite link has expired or already been used",
        ));
    }

    let sender = sqlx::query_as!(
        SenderRow,
        "SELECT display_name FROM users WHERE id = $1",
        row.sender_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(InviteDetailResponse {
        sender_name: sender.display_name,
        recipient_email: row.recipient_email,
        subject: row
            .subject
            .unwrap_or_else(|| "You've been invited to RustShare".to_string()),
        body: row.body.unwrap_or_default(),
        terms_enabled: row.terms_enabled,
        terms_text: row.terms_text,
        expires_at: row.expires_at,
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AcceptInviteRequest {
    pub display_name: String,
    pub email: String,
    pub password: String,
    pub terms_accepted: Option<bool>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AcceptInviteResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    post,
    path = "/api/v1/invites/{token}/accept",
    tag = "Invites",
    params(("token" = String, Path, description = "Token")),
    request_body = AcceptInviteRequest,
    responses(
        (status = 200, description = "Success", body = AcceptInviteResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn accept_invite(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(req): Json<AcceptInviteRequest>,
) -> Result<Json<AcceptInviteResponse>, AppError> {
    let mut tx = state.db_pool.begin().await?;
    let row = sqlx::query_as::<_, InviteAcceptRow>(
        "SELECT it.id as token_id, it.tenant_id, it.sender_id, it.recipient_email, it.expires_at, it.used_at, it.revoked_at,
                w.terms_enabled, w.terms_text
         FROM invite_tokens it
         JOIN workflows w ON it.workflow_id = w.id
         WHERE it.token = $1
         FOR UPDATE OF it",
    )
        .bind(&token)
        .fetch_optional(&mut *tx)
        .await?;
    let row = row.ok_or_else(|| AppError::gone("Invite link has expired or already been used"))?;

    if row.used_at.is_some() || row.revoked_at.is_some() || row.expires_at < chrono::Utc::now() {
        return Err(AppError::gone(
            "Invite link has expired or already been used",
        ));
    }

    let email = req.email.trim().to_ascii_lowercase();
    if email != row.recipient_email.trim().to_ascii_lowercase() {
        return Err(AppError::bad_request(
            "Email does not match the invited address",
        ));
    }

    if req.password.len() < 8 {
        return Err(AppError::bad_request(
            "Password must be at least 8 characters",
        ));
    }

    if req.password.len() > 128 {
        return Err(AppError::bad_request(
            "Password must be at most 128 characters",
        ));
    }

    if row.terms_enabled && !req.terms_accepted.unwrap_or(false) {
        return Err(AppError::bad_request(
            "You must accept the Terms & Conditions",
        ));
    }

    let group_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT a.group_id FROM invite_token_group_assignments a WHERE a.invite_token_id=$1 ORDER BY a.group_id",
    )
    .bind(row.token_id)
    .fetch_all(&mut *tx)
    .await?;
    let available_group_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT a.group_id FROM invite_token_group_assignments a JOIN user_groups g ON g.id=a.group_id AND g.tenant_id=$2 WHERE a.invite_token_id=$1 ORDER BY a.group_id FOR SHARE OF g",
    )
    .bind(row.token_id)
    .bind(row.tenant_id)
    .fetch_all(&mut *tx)
    .await?;
    if group_ids != available_group_ids {
        sqlx::query("UPDATE invite_tokens SET revoked_at=now() WHERE id=$1")
            .bind(row.token_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM invite_token_group_assignments WHERE invite_token_id=$1")
            .bind(row.token_id)
            .execute(&mut *tx)
            .await?;
        crate::handlers::admin::insert_scoped_admin_action(
            &mut tx,
            row.tenant_id,
            row.sender_id,
            "organization_admin.invite_revoked_group_unavailable",
            Some("invite"),
            Some(row.token_id),
            serde_json::json!({"group_ids": group_ids}),
            None,
        )
        .await?;
        tx.commit().await?;
        return Err(AppError::conflict(
            "This invitation is no longer available; request a new invitation",
        ));
    }

    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE lower(email) = lower($1)")
            .bind(&email)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_some() {
        return Err(AppError::conflict(
            "An account with this email already exists",
        ));
    }

    let password_hash =
        PasswordHasher::hash(&req.password).map_err(|e| AppError::internal(e.to_string()))?;

    let user_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    // Insert the user into the inviter's tenant (from the token row), not a
    // hardcoded nil tenant, so invitees land in the same tenant as the sender.
    let insert_result = sqlx::query(
        "INSERT INTO users (id, tenant_id, username, email, password_hash, display_name, is_admin, storage_quota, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, false, 10737418240, $7, $7)",
    )
    .bind(user_id)
    .bind(row.tenant_id)
    .bind(&email)
    .bind(&email)
    .bind(&password_hash)
    .bind(&req.display_name)
    .bind(now)
    .execute(&mut *tx)
    .await;

    // The check-then-insert above is not atomic: two concurrent accepts of the
    // same token/email can both pass the existence check. The users.email
    // unique constraint is the real guard — map the resulting violation to 409
    // instead of letting it surface as a 500.
    if let Err(sqlx::Error::Database(db_err)) = &insert_result {
        if db_err.is_unique_violation() {
            return Err(AppError::conflict(
                "An account with this email already exists",
            ));
        }
    }
    insert_result?;

    sqlx::query(
        "INSERT INTO group_members (group_id,user_id,added_by) SELECT a.group_id,$2,$3 FROM invite_token_group_assignments a WHERE a.invite_token_id=$1 ON CONFLICT (group_id,user_id) DO NOTHING",
    )
    .bind(row.token_id)
    .bind(user_id)
    .bind(row.sender_id)
    .execute(&mut *tx)
    .await?;
    crate::handlers::admin::insert_scoped_admin_action(
        &mut tx,
        row.tenant_id,
        user_id,
        "organization_admin.invite_accepted",
        Some("user"),
        Some(user_id),
        serde_json::json!({"accepted_by_user_id": user_id, "group_ids": group_ids}),
        None,
    )
    .await?;
    sqlx::query("DELETE FROM invite_token_group_assignments WHERE invite_token_id=$1")
        .bind(row.token_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("UPDATE invite_tokens SET used_at = NOW() WHERE id = $1")
        .bind(row.token_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    // Seed default Application preferences after the identity transaction commits.
    let pref_repo =
        rustshare_infrastructure::repositories::ApplicationUserPreferenceRepository::new(
            state.db_pool.clone(),
        );
    pref_repo.seed_defaults(user_id).await.ok();

    Ok(Json(AcceptInviteResponse {
        id: user_id.to_string(),
        email,
        display_name: req.display_name,
        created_at: now,
    }))
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct WorkflowRow {
    id: Uuid,
    subject: Option<String>,
    body: Option<String>,
    terms_enabled: bool,
    terms_text: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SenderRow {
    display_name: String,
}

#[derive(sqlx::FromRow)]
struct InviteTokenRow {
    sender_id: Uuid,
    recipient_email: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    used_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    subject: Option<String>,
    body: Option<String>,
    terms_enabled: bool,
    terms_text: Option<String>,
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct InviteAcceptRow {
    token_id: Uuid,
    tenant_id: Uuid,
    sender_id: Uuid,
    recipient_email: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    used_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    terms_enabled: bool,
    terms_text: Option<String>,
}
