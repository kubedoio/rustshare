//! Trusted HTTP boundary for the Elembra Principal ↔ Buzz identity binding.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{Duration, Utc};
use nostr::Event;
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_resource_auth::{
    BindingChallenge, BindingError, BindingStatus, ChatIdentityBinding, WorkspaceCommunityMapping,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AdminUser, AppError, AuthenticatedUser};
use crate::state::DatabaseState;

#[derive(Debug, Deserialize)]
pub struct ChallengeRequest {
    pub workspace_id: Uuid,
    pub buzz_pubkey: String,
    #[serde(default)]
    pub rotation_of: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    pub challenge_id: Uuid,
    pub nonce: String,
    pub buzz_pubkey: String,
    pub relay_url: String,
    pub expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub challenge_id: Uuid,
    pub event: Event,
}

#[derive(Debug, Deserialize)]
pub struct AdmissionRequest {
    pub workspace_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct AdmissionResponse {
    pub state: &'static str,
}

#[derive(Debug, Serialize)]
pub struct BindingResponse {
    pub binding_id: Uuid,
    pub buzz_pubkey: String,
    pub status: BindingStatus,
}

#[derive(Debug, Deserialize)]
pub struct MappingRequest {
    pub community_id: String,
    pub relay_url: String,
}

/// Configure an explicit tenant/workspace → Buzz community mapping.
pub async fn configure_mapping(
    AdminUser { user_id: admin_id }: AdminUser,
    auth: AuthenticatedUser,
    State(db): State<DatabaseState>,
    Path(workspace_id): Path<Uuid>,
    Json(input): Json<MappingRequest>,
) -> Result<StatusCode, AppError> {
    let admin_tenant = sqlx::query_scalar::<_, Uuid>(
        "SELECT tenant_id FROM users WHERE id = $1 AND disabled_at IS NULL",
    )
    .bind(admin_id)
    .fetch_optional(&db.db_pool)
    .await
    .map_err(internal_db)?
    .ok_or(AppError::Unauthorized)?;
    if admin_tenant != auth.tenant_id {
        return Err(AppError::Forbidden("tenant scope mismatch".into()));
    }
    ensure_workspace_scope(auth.tenant_id, workspace_id)?;
    if input.community_id.trim().is_empty() || input.relay_url.trim().is_empty() {
        return Err(AppError::bad_request(
            "community_id and relay_url are required",
        ));
    }
    validate_relay_url(&input.relay_url)?;
    let mapping = WorkspaceCommunityMapping {
        tenant_id: TenantId(auth.tenant_id),
        workspace_id: WorkspaceId(workspace_id),
        community_id: input.community_id,
        relay_url: input.relay_url,
        active: true,
    };
    db.chat_identity_store
        .insert_mapping(&mapping)
        .await
        .map_err(|error| {
            AppError::conflict(format!("mapping already exists or is invalid: {error}"))
        })?;
    Ok(StatusCode::CREATED)
}

/// Issue a short-lived, mapping-bound NIP-42 challenge.
pub async fn create_challenge(
    auth: AuthenticatedUser,
    State(db): State<DatabaseState>,
    Json(input): Json<ChallengeRequest>,
) -> Result<(StatusCode, Json<ChallengeResponse>), AppError> {
    ensure_active_principal(&db, &auth).await?;
    ensure_workspace_scope(auth.tenant_id, input.workspace_id)?;
    validate_pubkey(&input.buzz_pubkey)?;
    let mapping = db
        .chat_identity_store
        .mapping(TenantId(auth.tenant_id), WorkspaceId(input.workspace_id))
        .await
        .map_err(internal_db)?
        .filter(|mapping| mapping.active)
        .ok_or_else(|| AppError::not_found("active Chat workspace mapping not found"))?;
    if !db
        .chat_identity_store
        .chat_access(
            TenantId(auth.tenant_id),
            WorkspaceId(input.workspace_id),
            PrincipalId(auth.user_id),
        )
        .await
        .map_err(internal_db)?
    {
        return Err(AppError::Forbidden(
            "Chat is not enabled for this Principal/workspace".into(),
        ));
    }

    let challenge = BindingChallenge::issue(
        TenantId(auth.tenant_id),
        PrincipalId(auth.user_id),
        input.buzz_pubkey,
        mapping.relay_url,
        input.rotation_of,
        Utc::now(),
        Duration::minutes(5),
    );
    db.chat_identity_store
        .insert_challenge(&challenge)
        .await
        .map_err(internal_db)?;
    Ok((
        StatusCode::CREATED,
        Json(ChallengeResponse {
            challenge_id: challenge.challenge_id,
            nonce: challenge.nonce,
            buzz_pubkey: challenge.buzz_pubkey,
            relay_url: challenge.relay_url,
            expires_at: challenge.expires_at,
        }),
    ))
}

/// Verify the client's signed NIP-42 event and atomically activate the binding.
pub async fn verify_binding(
    auth: AuthenticatedUser,
    State(db): State<DatabaseState>,
    Json(input): Json<VerifyRequest>,
) -> Result<(StatusCode, Json<BindingResponse>), AppError> {
    ensure_active_principal(&db, &auth).await?;
    let mut challenge = db
        .chat_identity_store
        .challenge(input.challenge_id)
        .await
        .map_err(internal_db)?
        .ok_or_else(|| AppError::not_found("binding challenge not found"))?;
    let now = Utc::now();
    challenge
        .verify_and_consume(
            TenantId(auth.tenant_id),
            PrincipalId(auth.user_id),
            &input.event,
            now,
        )
        .map_err(binding_error)?;

    let binding = ChatIdentityBinding {
        binding_id: Uuid::new_v4(),
        tenant_id: challenge.tenant_id,
        principal_id: challenge.principal_id,
        buzz_pubkey: challenge.buzz_pubkey.clone(),
        status: BindingStatus::Active,
        created_at: now,
        verified_at: Some(now),
        revoked_at: None,
        rotation_of: challenge.rotation_of,
        audit_metadata: serde_json::json!({"proof": "nip-42"}),
    };
    if !db
        .chat_identity_store
        .consume_and_activate(&challenge, &binding, now)
        .await
        .map_err(internal_db)?
    {
        return Err(AppError::conflict(
            "binding challenge was already consumed or expired",
        ));
    }
    Ok((
        StatusCode::CREATED,
        Json(BindingResponse {
            binding_id: binding.binding_id,
            buzz_pubkey: binding.buzz_pubkey,
            status: binding.status,
        }),
    ))
}

/// Persist the Elembra-side admission and enqueue the Buzz bridge operation.
pub async fn admit_identity(
    auth: AuthenticatedUser,
    State(db): State<DatabaseState>,
    Json(input): Json<AdmissionRequest>,
) -> Result<(StatusCode, Json<AdmissionResponse>), AppError> {
    ensure_active_principal(&db, &auth).await?;
    ensure_workspace_scope(auth.tenant_id, input.workspace_id)?;
    if !db
        .chat_identity_store
        .admit_binding(
            TenantId(auth.tenant_id),
            PrincipalId(auth.user_id),
            WorkspaceId(input.workspace_id),
        )
        .await
        .map_err(internal_db)?
    {
        return Err(AppError::Forbidden(
            "active Chat binding, workspace mapping, or Chat access not found".into(),
        ));
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(AdmissionResponse { state: "queued" }),
    ))
}

async fn ensure_active_principal(
    db: &DatabaseState,
    auth: &AuthenticatedUser,
) -> Result<(), AppError> {
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM users
            WHERE id = $1 AND tenant_id = $2 AND disabled_at IS NULL
        )",
    )
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .fetch_one(&db.db_pool)
    .await
    .map_err(internal_db)?;
    if active {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

fn validate_pubkey(value: &str) -> Result<(), AppError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AppError::bad_request(
            "buzz_pubkey must be 64 hexadecimal characters",
        ))
    }
}

fn validate_relay_url(value: &str) -> Result<(), AppError> {
    let url = url::Url::parse(value)
        .map_err(|_| AppError::bad_request("relay_url must be a valid WebSocket URL"))?;
    if !matches!(url.scheme(), "wss" | "ws") || url.host_str().is_none() {
        return Err(AppError::bad_request(
            "relay_url must use ws:// or wss:// and include a host",
        ));
    }
    Ok(())
}

fn ensure_workspace_scope(tenant_id: Uuid, workspace_id: Uuid) -> Result<(), AppError> {
    if tenant_id == workspace_id {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "workspace is outside the current tenant scope".into(),
        ))
    }
}

fn binding_error(error: BindingError) -> AppError {
    match error {
        BindingError::ChallengeReplayed => AppError::conflict(error.to_string()),
        BindingError::ChallengeExpired => AppError::gone(error.to_string()),
        BindingError::ChallengeScopeMismatch => AppError::Forbidden(error.to_string()),
        _ => AppError::bad_request(error.to_string()),
    }
}

fn internal_db(error: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("Chat identity storage failure: {error}"))
}
