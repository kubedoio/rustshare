use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use rand::RngCore;
use rustshare_auth::PasswordHasher;
use rustshare_core::services::EmailService;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    handlers::{AuthenticatedUser, ErrorResponse},
    AppState,
};

#[derive(Deserialize)]
pub struct CreateInviteRequest {
    pub recipient_email: String,
    pub origin: Option<String>,
}

#[derive(Serialize)]
pub struct CreateInviteResponse {
    pub token: String,
    pub invite_link: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create_invite(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Json(req): Json<CreateInviteRequest>,
) -> Result<Json<CreateInviteResponse>, (StatusCode, Json<ErrorResponse>)> {
    let workflow = sqlx::query_as::<_, WorkflowRow>(
        "SELECT id, subject, body, terms_enabled, terms_text
         FROM workflows
         WHERE key = 'invite_email' AND status = 'active'",
    )
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse::new("Invite workflow is not active")),
        )
    })?;

    let sender = sqlx::query_as::<_, SenderRow>("SELECT display_name FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    let mut token_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut token_bytes);
    let token = hex::encode(token_bytes);

    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    let tenant_id: Uuid = sqlx::query_scalar("SELECT tenant_id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db_pool)
        .await
        .unwrap_or_else(|_| Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap());

    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(tenant_id)
    .bind(&token)
    .bind(user_id)
    .bind(&req.recipient_email)
    .bind(workflow.id)
    .bind(expires_at)
    .execute(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    let origin = req
        .origin
        .unwrap_or_else(|| "http://localhost:8080".to_string());
    let invite_link = format!("{}/invite/{}", origin.trim_end_matches('/'), token);

    let email_service = EmailService::new(state.db_pool.clone(), state.secret_key.clone());
    let result = email_service
        .send_invite_email(
            &sender.display_name,
            &req.recipient_email, // recipient_name fallback
            &req.recipient_email,
            &invite_link,
            workflow
                .subject
                .as_deref()
                .unwrap_or("You've been invited to RustShare"),
            workflow.body.as_deref().unwrap_or(""),
        )
        .await;

    if let Err(e) = result {
        sqlx::query("DELETE FROM invite_tokens WHERE token = $1")
            .bind(&token)
            .execute(&state.db_pool)
            .await
            .ok();
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse::new(format!(
                "Failed to send invite email: {}",
                e
            ))),
        ));
    }

    Ok(Json(CreateInviteResponse {
        token,
        invite_link,
        expires_at,
    }))
}

#[derive(Serialize)]
pub struct InviteDetailResponse {
    pub sender_name: String,
    pub recipient_email: String,
    pub subject: String,
    pub body: String,
    pub terms_enabled: bool,
    pub terms_text: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub async fn get_invite(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<InviteDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    let row = sqlx::query_as::<_, InviteTokenRow>(
        "SELECT it.sender_id, it.recipient_email, it.expires_at, it.used_at, it.revoked_at,
                w.subject, w.body, w.terms_enabled, w.terms_text
         FROM invite_tokens it
         JOIN workflows w ON it.workflow_id = w.id
         WHERE it.token = $1",
    )
    .bind(&token)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::GONE,
            Json(ErrorResponse::new(
                "Invite link has expired or already been used",
            )),
        )
    })?;

    if row.used_at.is_some() || row.revoked_at.is_some() || row.expires_at < chrono::Utc::now() {
        return Err((
            StatusCode::GONE,
            Json(ErrorResponse::new(
                "Invite link has expired or already been used",
            )),
        ));
    }

    let sender = sqlx::query_as::<_, SenderRow>("SELECT display_name FROM users WHERE id = $1")
        .bind(row.sender_id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

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

#[derive(Deserialize)]
pub struct AcceptInviteRequest {
    pub display_name: String,
    pub email: String,
    pub password: String,
    pub terms_accepted: Option<bool>,
}

#[derive(Serialize)]
pub struct AcceptInviteResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn accept_invite(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(req): Json<AcceptInviteRequest>,
) -> Result<Json<AcceptInviteResponse>, (StatusCode, Json<ErrorResponse>)> {
    let row = sqlx::query_as::<_, InviteAcceptRow>(
        "SELECT it.id as token_id, it.recipient_email, it.expires_at, it.used_at, it.revoked_at,
                w.terms_enabled, w.terms_text
         FROM invite_tokens it
         JOIN workflows w ON it.workflow_id = w.id
         WHERE it.token = $1",
    )
    .bind(&token)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::GONE,
            Json(ErrorResponse::new(
                "Invite link has expired or already been used",
            )),
        )
    })?;

    if row.used_at.is_some() || row.revoked_at.is_some() || row.expires_at < chrono::Utc::now() {
        return Err((
            StatusCode::GONE,
            Json(ErrorResponse::new(
                "Invite link has expired or already been used",
            )),
        ));
    }

    if req.email != row.recipient_email {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "Email does not match the invited address",
            )),
        ));
    }

    if req.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Password must be at least 8 characters")),
        ));
    }

    if row.terms_enabled && !req.terms_accepted.unwrap_or(false) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("You must accept the Terms & Conditions")),
        ));
    }

    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&req.email)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    if existing.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::new(
                "An account with this email already exists",
            )),
        ));
    }

    let password_hash = PasswordHasher::hash(&req.password).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?;

    let user_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO users (id, tenant_id, username, email, password_hash, display_name, is_admin, storage_quota, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, false, 10737418240, $7, $7)"
    )
    .bind(user_id)
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap())
    .bind(&req.email)
    .bind(&req.email)
    .bind(&password_hash)
    .bind(&req.display_name)
    .bind(now)
    .execute(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(e.to_string()))))?;

    // Seed default module preferences
    let pref_repo = rustshare_infrastructure::repositories::UserModulePreferenceRepository::new(
        state.db_pool.clone(),
    );
    pref_repo.seed_defaults(user_id).await.ok();

    sqlx::query("UPDATE invite_tokens SET used_at = NOW() WHERE id = $1")
        .bind(row.token_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    Ok(Json(AcceptInviteResponse {
        id: user_id.to_string(),
        email: req.email,
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
    recipient_email: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    used_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    terms_enabled: bool,
    terms_text: Option<String>,
}
