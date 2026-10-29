//! Trusted HTTP boundary for the Elembra Principal ↔ Buzz identity binding.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{Duration, Utc};
use nostr::{Event, Kind};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_resource_auth::{
    BindingChallenge, BindingError, BindingStatus, ChatIdentityBinding, WorkspaceCommunityMapping,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AdminUser, AppError, AuthenticatedUser, OrganizationAdmin};
use crate::services::chat_bootstrap::{ChatBootstrapError, ProvisionOutcome};
use crate::state::{AppState, DatabaseState};

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
    pub tenant_id: Uuid,
    pub principal_id: Uuid,
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ReplacementRequest {
    pub workspace_id: Uuid,
    pub challenge_id: Uuid,
    pub old_binding_id: Uuid,
    #[schema(value_type = Object)]
    pub event: Event,
    pub operation_id: Uuid,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReplacementInitiationResponse {
    pub operation_id: Uuid,
    pub state: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReplacementStatusResponse {
    pub operation_id: Uuid,
    pub state: String,
    pub generation: i64,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub error_category: Option<String>,
}

const BUZZ_COMPATIBILITY: &str = include_str!("../../../../config/buzz-compatibility.env");
// This PR has no qualified v1alpha2 production Buzz build. Populate only in a
// reviewed compatibility update after live acceptance and security approval.
const QUALIFIED_BUZZ_V1ALPHA2_BUILDS: &[(&str, &str)] = &[];

fn buzz_runtime_qualified(config: &str, approved_builds: &[(&str, &str)]) -> bool {
    let field = |name: &str| {
        let mut values = config.lines().filter_map(|line| {
            line.strip_prefix(name)
                .and_then(|value| value.strip_prefix('='))
        });
        let value = values.next()?;
        values.next().is_none().then_some(value)
    };
    let (Some(commit), Some(image), Some(tag), Some(digest), Some(relay_image)) = (
        field("BUZZ_SUPPORTED_COMMIT"),
        field("BUZZ_SUPPORTED_IMAGE"),
        field("BUZZ_SUPPORTED_IMAGE_TAG"),
        field("BUZZ_SUPPORTED_IMAGE_DIGEST"),
        field("BUZZ_RELAY_IMAGE"),
    ) else {
        return false;
    };
    let commit_is_canonical = commit.len() == 40
        && commit
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    let digest_is_canonical = digest.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    });
    commit_is_canonical
        && digest_is_canonical
        && field("BUZZ_CONTRACT_VERSION") == Some("v1alpha2")
        && field("BUZZ_REPLACEMENT_V1ALPHA2_QUALIFIED") == Some("1")
        && tag == format!("sha-{}", &commit[..7])
        && relay_image == format!("{image}@{digest}")
        && approved_builds.contains(&(commit, digest))
}

fn buzz_replacement_runtime_qualified() -> bool {
    buzz_runtime_qualified(BUZZ_COMPATIBILITY, QUALIFIED_BUZZ_V1ALPHA2_BUILDS)
}

fn explicit_replacement_flag(value: Option<&str>) -> bool {
    value == Some("1")
}

fn replacement_initiation_enabled(db: &AppState) -> bool {
    explicit_replacement_flag(
        std::env::var("RUSTSHARE_CHAT_REPLACEMENT_INITIATION_ENABLED")
            .ok()
            .as_deref(),
    ) && explicit_replacement_flag(
        std::env::var("RUSTSHARE_CHAT_REPLACEMENT_WORKER_ENABLED")
            .ok()
            .as_deref(),
    ) && db.outbox_worker_enabled
        && std::env::var("RUSTSHARE_CHAT_AUTHORITY").as_deref() == Ok("buzz")
        && std::env::var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY")
            .ok()
            .is_some_and(|key| nostr::Keys::parse(&key).is_ok())
        && buzz_replacement_runtime_qualified()
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RevokePrincipalResponse {
    pub revoked: bool,
}

#[derive(Debug, Deserialize)]
pub struct MappingRequest {
    pub community_id: String,
    pub relay_url: String,
    /// Optional pinned relay public key (64 lowercase hex) whose signatures
    /// are trusted when asking the community's authoritative relay.
    #[serde(default)]
    pub relay_pubkey: Option<String>,
    /// Admin-selected Buzz wire contract. Existing clients and mappings stay
    /// on v1alpha1 unless an administrator explicitly opts in.
    #[serde(default = "default_membership_protocol")]
    pub membership_protocol_version: String,
}

fn default_membership_protocol() -> String {
    "v1alpha1".to_string()
}

/// Revoke a user's Chat binding and active admissions in the admin's tenant.
#[utoipa::path(
    post,
    path = "/api/v1/admin/applications/chat/principals/{principal_id}/revoke",
    tag = "Chat",
    params(("principal_id" = Uuid, Path, description = "User principal id")),
    responses(
        (status = 200, description = "Chat access revoked", body = RevokePrincipalResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn revoke_principal(
    AdminUser { user_id: admin_id }: AdminUser,
    State(db): State<DatabaseState>,
    Path(principal_id): Path<Uuid>,
) -> Result<Json<RevokePrincipalResponse>, AppError> {
    let tenant_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT tenant_id FROM users WHERE id = $1 AND disabled_at IS NULL",
    )
    .bind(admin_id)
    .fetch_optional(&db.db_pool)
    .await
    .map_err(internal_db)?
    .ok_or(AppError::Unauthorized)?;
    let revoked = db
        .chat_identity_store
        .revoke_principal(TenantId(tenant_id), PrincipalId(principal_id))
        .await
        .map_err(internal_db)?;
    Ok(Json(RevokePrincipalResponse {
        revoked: revoked > 0,
    }))
}

/// Retry only a recoverable blocked replacement; the worker must reverify membership.
#[utoipa::path(
    post,
    path = "/api/v1/admin/applications/chat/replacements/{operation_id}/recover",
    tag = "Chat (admin)",
    params(("operation_id" = Uuid, Path, description = "Replacement operation id")),
    responses(
        (status = 204, description = "Replacement queued for fresh verification"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Operation unavailable or not recoverable", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn recover_replacement(
    OrganizationAdmin {
        user_id: admin_id,
        tenant_id,
    }: OrganizationAdmin,
    State(db): State<DatabaseState>,
    Path(operation_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    if std::env::var("RUSTSHARE_CHAT_REPLACEMENT_RECOVERY_ENABLED").as_deref() != Ok("1")
        || std::env::var("RUSTSHARE_CHAT_REPLACEMENT_WORKER_ENABLED").as_deref() != Ok("1")
    {
        return Err(AppError::not_found("Chat replacement recovery is disabled"));
    }
    let recovered = db
        .chat_identity_store
        .recover_blocked_replacement(TenantId(tenant_id), admin_id, operation_id)
        .await
        .map_err(internal_db)?;
    if !recovered {
        return Err(AppError::not_found(
            "blocked Chat replacement is not recoverable",
        ));
    }
    Ok(StatusCode::NO_CONTENT)
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
    validate_relay_url(&input.relay_url).await?;
    if let Some(relay_pubkey) = &input.relay_pubkey {
        validate_relay_pubkey(relay_pubkey)?;
    }
    if !matches!(
        input.membership_protocol_version.as_str(),
        "v1alpha1" | "v1alpha2"
    ) || (input.membership_protocol_version == "v1alpha2" && input.relay_pubkey.is_none())
    {
        return Err(AppError::bad_request(
            "v1alpha2 requires an explicit pinned relay public key",
        ));
    }
    if input.membership_protocol_version == "v1alpha2"
        && Uuid::parse_str(&input.community_id).map_or(true, |community| {
            community.to_string() != input.community_id
        })
    {
        return Err(AppError::bad_request(
            "v1alpha2 community_id must be a canonical UUID",
        ));
    }
    let mapping = WorkspaceCommunityMapping {
        tenant_id: TenantId(auth.tenant_id),
        workspace_id: WorkspaceId(workspace_id),
        community_id: input.community_id,
        relay_url: input.relay_url,
        relay_pubkey: input.relay_pubkey,
        membership_protocol_version: input.membership_protocol_version,
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

#[derive(Debug, Deserialize)]
pub struct UpdateCommunityMappingRequest {
    pub relay_url: String,
    /// Optional pinned relay public key (64 lowercase hex) whose signatures
    /// are trusted when asking the community's authoritative relay. Both
    /// fields are always written: pass the current `relay_url` (and/or the
    /// current pin) for the side you are not rotating.
    #[serde(default)]
    pub relay_pubkey: Option<String>,
    #[serde(default)]
    pub membership_protocol_version: Option<String>,
}

/// Rotate the mapping's relay endpoint and/or pinned pubkey WITHOUT changing
/// `community_id`/`workspace_id` (which would orphan admissions). A stale pin
/// makes BUZZ-mode reads fail closed (Deny) until the relay rotates to the
/// new key, so an operator can pre-pin the new key before the relay switches.
pub async fn update_community_mapping(
    AdminUser { user_id: admin_id }: AdminUser,
    auth: AuthenticatedUser,
    State(db): State<DatabaseState>,
    Path(workspace_id): Path<WorkspaceId>,
    Json(input): Json<UpdateCommunityMappingRequest>,
) -> Result<(StatusCode, Json<WorkspaceCommunityMapping>), AppError> {
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
    ensure_workspace_scope(auth.tenant_id, workspace_id.0)?;
    if input.relay_url.trim().is_empty() {
        return Err(AppError::bad_request("relay_url is required"));
    }
    validate_relay_url(&input.relay_url).await?;
    if let Some(relay_pubkey) = &input.relay_pubkey {
        validate_relay_pubkey(relay_pubkey)?;
    }

    let tenant_id = TenantId(auth.tenant_id);
    let existing = db
        .chat_identity_store
        .mapping(tenant_id, workspace_id)
        .await
        .map_err(internal_db)?
        .ok_or_else(|| AppError::not_found("Chat workspace mapping not found"))?;
    let requested_protocol = input
        .membership_protocol_version
        .as_deref()
        .unwrap_or(&existing.membership_protocol_version);
    if requested_protocol == "v1alpha2" && input.relay_pubkey.is_none() {
        return Err(AppError::bad_request(
            "v1alpha2 mappings require the pinned relay public key on updates",
        ));
    }
    if requested_protocol == "v1alpha2"
        && Uuid::parse_str(&existing.community_id).map_or(true, |community| {
            community.to_string() != existing.community_id
        })
    {
        return Err(AppError::bad_request(
            "v1alpha2 community_id must be a canonical UUID",
        ));
    }
    if let Some(version) = &input.membership_protocol_version {
        if !matches!(version.as_str(), "v1alpha1" | "v1alpha2") {
            return Err(AppError::bad_request("unsupported membership protocol"));
        }
    }
    let updated = WorkspaceCommunityMapping {
        tenant_id: existing.tenant_id,
        workspace_id: existing.workspace_id,
        community_id: existing.community_id,
        relay_url: input.relay_url,
        relay_pubkey: input.relay_pubkey,
        membership_protocol_version: input
            .membership_protocol_version
            .unwrap_or(existing.membership_protocol_version.clone()),
        active: existing.active,
    };
    if !db
        .chat_identity_store
        .update_mapping_relay(
            tenant_id,
            workspace_id,
            updated.relay_url.clone(),
            updated.relay_pubkey.clone(),
            &updated.membership_protocol_version,
        )
        .await
        .map_err(internal_db)?
    {
        return Err(AppError::conflict(
            "protocol changes or relay rotation are unsafe while legacy mutations or Buzz operations remain outstanding",
        ));
    }
    Ok((StatusCode::OK, Json(updated)))
}

/// Auto-provision the deployment Buzz community for a workspace (idempotent,
/// ADR-0036). Admin-only and tenant-scoped. On success the workspace has a
/// mapping row (either just created or pre-existing and verified identical).
#[utoipa::path(
    post,
    path = "/api/v1/admin/applications/chat/workspaces/{workspace_id}/provision",
    tag = "Chat (admin)",
    responses(
        (status = 201, description = "Provisioned or already configured", body = ProvisionCommunityResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 409, description = "Community in use or mismatch", body = crate::handlers::ErrorResponse),
        (status = 502, description = "Upstream Buzz authority error", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn provision_community_mapping(
    AdminUser { user_id: admin_id }: AdminUser,
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<(StatusCode, Json<ProvisionCommunityResponse>), AppError> {
    let admin_tenant = sqlx::query_scalar::<_, Uuid>(
        "SELECT tenant_id FROM users WHERE id = $1 AND disabled_at IS NULL",
    )
    .bind(admin_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(internal_db)?
    .ok_or(AppError::Unauthorized)?;
    if admin_tenant != auth.tenant_id {
        return Err(AppError::Forbidden("tenant scope mismatch".into()));
    }
    ensure_workspace_scope(auth.tenant_id, workspace_id)?;
    let outcome = state
        .chat_bootstrap
        .as_ref()
        .ok_or_else(|| AppError::bad_request("chat provisioning requires the buzz chat authority"))?
        .provision(TenantId(auth.tenant_id), WorkspaceId(workspace_id))
        .await
        .map_err(bootstrap_error_to_app_error)?;
    let (status, community_id, relay_url, relay_pubkey) = match outcome {
        ProvisionOutcome::Inserted {
            community_id,
            relay_url,
            relay_pubkey,
        } => ("created", community_id, relay_url, relay_pubkey),
        ProvisionOutcome::AlreadyConfigured {
            community_id,
            relay_url,
            relay_pubkey,
        } => (
            "already_configured",
            community_id,
            relay_url,
            relay_pubkey.unwrap_or_default(),
        ),
    };
    Ok((
        StatusCode::CREATED,
        Json(ProvisionCommunityResponse {
            status: status.to_string(),
            community_id,
            relay_url,
            relay_pubkey,
        }),
    ))
}

/// Admin diagnostics: the workspace's current mapping, including the pinned
/// relay pubkey. Never exposed on the user-facing status surface.
#[utoipa::path(
    get,
    path = "/api/v1/admin/applications/chat/workspaces/{workspace_id}/community",
    tag = "Chat (admin)",
    responses(
        (status = 200, description = "Mapping", body = AdminCommunityMappingResponse),
        (status = 404, description = "No mapping", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_community_mapping(
    AdminUser { user_id: admin_id }: AdminUser,
    auth: AuthenticatedUser,
    State(db): State<DatabaseState>,
    Path(workspace_id): Path<WorkspaceId>,
) -> Result<Json<AdminCommunityMappingResponse>, AppError> {
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
    ensure_workspace_scope(auth.tenant_id, workspace_id.0)?;
    let mapping = db
        .chat_identity_store
        .mapping(TenantId(auth.tenant_id), workspace_id)
        .await
        .map_err(internal_db)?
        .ok_or_else(|| AppError::not_found("no community mapping for this workspace"))?;
    Ok(Json(AdminCommunityMappingResponse {
        community_id: mapping.community_id,
        relay_url: mapping.relay_url,
        relay_pubkey: mapping.relay_pubkey,
        active: mapping.active,
    }))
}

/// Provisioning response — idempotent by design (`status` says which path).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProvisionCommunityResponse {
    pub status: String,
    pub community_id: String,
    pub relay_url: String,
    pub relay_pubkey: String,
}

/// Admin-only mapping diagnostics (includes the pin; not for user surfaces).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AdminCommunityMappingResponse {
    pub community_id: String,
    pub relay_url: String,
    pub relay_pubkey: Option<String>,
    pub active: bool,
}

fn bootstrap_error_to_app_error(error: ChatBootstrapError) -> AppError {
    match error {
        ChatBootstrapError::CommunityInUse { .. }
        | ChatBootstrapError::CommunityMismatch { .. } => AppError::conflict(error.to_string()),
        // 401 from the relay: the service identity used by Elembra is not
        // trusted by the relay. Surface the operator-actionable diagnostic
        // verbatim so the admin UI can display it.
        ChatBootstrapError::ServiceIdentityRejected => AppError::bad_gateway(error.to_string()),
        // The relay was reachable-but-invalid or unreachable: a gateway
        // failure of the upstream Buzz authority, not a server bug.
        ChatBootstrapError::Discovery(_) => {
            AppError::bad_gateway(format!("chat provisioning failed: {error}"))
        }
        other => AppError::internal(format!("chat provisioning failed: {other}")),
    }
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
    // Older rows may predate relay URL validation. Do not persist or return
    // credentials from such a mapping through a new challenge response.
    validate_relay_url(&mapping.relay_url).await?;
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
    if !db
        .chat_identity_store
        .insert_challenge_for_mapping(&challenge, WorkspaceId(input.workspace_id))
        .await
        .map_err(internal_db)?
    {
        return Err(AppError::conflict(
            "Chat workspace mapping changed while issuing the challenge",
        ));
    }
    Ok((
        StatusCode::CREATED,
        Json(ChallengeResponse {
            challenge_id: challenge.challenge_id,
            tenant_id: challenge.tenant_id.0,
            principal_id: challenge.principal_id.0,
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
    if challenge.rotation_of.is_some() {
        return Err(AppError::conflict(
            "identity replacement is unavailable until Buzz revocation and membership can be verified",
        ));
    }

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
            "binding challenge is consumed, expired, or no longer eligible",
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

/// Start the durable, two-sided Buzz identity replacement operation.
#[utoipa::path(
    post,
    path = "/api/v1/applications/chat/identity-replacements",
    tag = "Chat",
    request_body = ReplacementRequest,
    responses(
        (status = 202, description = "Replacement queued for revocation", body = ReplacementInitiationResponse),
        (status = 400, description = "Invalid replacement request or proof", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Replacement initiation is disabled or challenge unavailable", body = crate::handlers::ErrorResponse),
        (status = 410, description = "Challenge expired", body = crate::handlers::ErrorResponse),
        (status = 409, description = "Invalid or conflicting replacement", body = crate::handlers::ErrorResponse),
        (status = 500, description = "Storage failure", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn initiate_replacement(
    auth: AuthenticatedUser,
    State(app): State<AppState>,
    State(db): State<DatabaseState>,
    Json(input): Json<ReplacementRequest>,
) -> Result<(StatusCode, Json<ReplacementInitiationResponse>), AppError> {
    if !replacement_initiation_enabled(&app) {
        return Err(AppError::not_found("Chat identity replacement is disabled"));
    }
    ensure_workspace_scope(auth.tenant_id, input.workspace_id)?;
    ensure_active_principal(&db, &auth).await?;
    if input.operation_id.is_nil() || input.challenge_id.is_nil() || input.old_binding_id.is_nil() {
        return Err(AppError::bad_request(
            "replacement identifiers must be non-nil",
        ));
    }
    let start = rustshare_storage::ReplacementStart {
        operation_id: input.operation_id,
        replacement_binding_id: input.operation_id,
        old_binding_id: input.old_binding_id,
        tenant_id: TenantId(auth.tenant_id),
        principal_id: PrincipalId(auth.user_id),
        workspace_id: WorkspaceId(input.workspace_id),
        actor_id: auth.user_id,
    };
    let proof_event_id = if input.event.kind == Kind::Authentication && input.event.verify().is_ok()
    {
        input.event.id.to_hex()
    } else {
        return Err(AppError::bad_request("invalid NIP-42 proof"));
    };
    if let Some(matches) = db
        .chat_identity_store
        .replacement_request_matches(&start, input.challenge_id, &proof_event_id)
        .await
        .map_err(internal_db)?
    {
        if matches {
            return Ok((
                StatusCode::ACCEPTED,
                Json(ReplacementInitiationResponse {
                    operation_id: input.operation_id,
                    state: "pending_revocation".into(),
                }),
            ));
        }
        return Err(AppError::conflict("operation ID is already in use"));
    }

    let challenge = db
        .chat_identity_store
        .challenge(input.challenge_id)
        .await
        .map_err(internal_db)?
        .ok_or_else(|| AppError::not_found("binding challenge not found"))?;
    if challenge.rotation_of != Some(input.old_binding_id) {
        return Err(AppError::conflict(
            "challenge does not authorize replacement of this binding",
        ));
    }
    let proof = challenge
        .verify_proof(
            TenantId(auth.tenant_id),
            PrincipalId(auth.user_id),
            &input.event,
            Utc::now(),
        )
        .map_err(binding_error)?;
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
    let started = db
        .chat_identity_store
        .begin_replacement(&start, &proof)
        .await
        .map_err(internal_db)?;
    if !started {
        return Err(AppError::conflict(
            "replacement request is no longer eligible",
        ));
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(ReplacementInitiationResponse {
            operation_id: input.operation_id,
            state: "pending_revocation".into(),
        }),
    ))
}

/// Read operation status within the authenticated Principal's tenant scope.
#[utoipa::path(
    get,
    path = "/api/v1/applications/chat/identity-replacements/{operation_id}",
    tag = "Chat",
    params(("operation_id" = Uuid, Path, description = "Replacement operation id")),
    responses(
        (status = 200, description = "Replacement status", body = ReplacementStatusResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Operation unavailable", body = crate::handlers::ErrorResponse),
        (status = 500, description = "Storage failure", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn replacement_status(
    auth: AuthenticatedUser,
    State(db): State<DatabaseState>,
    Path(operation_id): Path<Uuid>,
) -> Result<Json<ReplacementStatusResponse>, AppError> {
    ensure_active_principal(&db, &auth).await?;
    let status = db
        .chat_identity_store
        .replacement_status(TenantId(auth.tenant_id), auth.user_id, operation_id)
        .await
        .map_err(internal_db)?
        .ok_or_else(|| AppError::not_found("replacement operation not found"))?;
    Ok(Json(ReplacementStatusResponse {
        operation_id: status.operation_id,
        state: if status.completed_at.is_some() {
            "completed".into()
        } else if status.blocked_at.is_some() {
            "blocked".into()
        } else {
            status.state
        },
        generation: status.generation,
        created_at: status.created_at,
        updated_at: status.updated_at,
        completed_at: status.completed_at,
        error_category: status.failure_code.map(|_| "operation_blocked".into()),
    }))
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

/// Validate a pinned relay public key: exactly 64 lowercase hex characters,
/// mirroring the DB CHECK on `chat_workspace_communities.relay_pubkey`
/// (`^[0-9a-f]{64}$`), so any value that passes validation also satisfies the
/// column constraint.
fn validate_relay_pubkey(value: &str) -> Result<(), AppError> {
    if crate::config::is_lowercase_hex_64(value) {
        Ok(())
    } else {
        Err(AppError::bad_request(
            "relay_pubkey must be 64 lowercase hexadecimal characters",
        ))
    }
}

async fn validate_relay_url(value: &str) -> Result<(), AppError> {
    let url = url::Url::parse(value)
        .map_err(|_| AppError::bad_request("relay_url must be a valid WebSocket URL"))?;
    if !matches!(url.scheme(), "wss" | "ws") || url.host_str().is_none() {
        return Err(AppError::bad_request(
            "relay_url must use ws:// or wss:// and include a host",
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::bad_request(
            "relay_url must not contain credentials, query parameters, or fragments",
        ));
    }
    url.host_str()
        .ok_or_else(|| AppError::bad_request("relay_url must include a host"))?;
    url.port_or_known_default()
        .ok_or_else(|| AppError::bad_request("relay_url must include a valid port"))?;
    rustshare_core::validation::resolve_chat_relay_socket_addrs_for_url(value)
        .await
        .map_err(|_| AppError::bad_request("relay_url must resolve to an allowed address"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_feature_flags_require_explicit_one() {
        assert!(!explicit_replacement_flag(None));
        assert!(!explicit_replacement_flag(Some("0")));
        assert!(!explicit_replacement_flag(Some("true")));
        assert!(explicit_replacement_flag(Some("1")));
    }

    #[test]
    fn unqualified_pinned_buzz_runtime_cannot_enable_replacement() {
        assert!(!buzz_replacement_runtime_qualified());
    }

    #[test]
    fn replacement_runtime_requires_the_approved_commit_digest_image_tuple() {
        let commit = "0123456789abcdef0123456789abcdef01234567";
        let digest = format!("sha256:{}", "a".repeat(64));
        let image = "ghcr.io/example/buzz";
        let config = format!(
            "BUZZ_SUPPORTED_COMMIT={commit}\nBUZZ_SUPPORTED_IMAGE={image}\nBUZZ_SUPPORTED_IMAGE_TAG=sha-{}\nBUZZ_SUPPORTED_IMAGE_DIGEST={digest}\nBUZZ_RELAY_IMAGE={image}@{digest}\nBUZZ_CONTRACT_VERSION=v1alpha2\nBUZZ_REPLACEMENT_V1ALPHA2_QUALIFIED=1\n",
            &commit[..7]
        );
        let approved = [(commit, digest.as_str())];
        assert!(buzz_runtime_qualified(&config, &approved));
        assert!(!buzz_runtime_qualified(
            &config.replace(&digest, &format!("sha256:{}", "b".repeat(64))),
            &approved
        ));
        assert!(!buzz_runtime_qualified(
            &config.replace(
                &format!("{image}@{digest}"),
                &format!("ghcr.io/example/other@{digest}")
            ),
            &approved
        ));
        assert!(!buzz_runtime_qualified(
            &config.replace(&digest, "sha256:invalid"),
            &approved
        ));
        assert!(!buzz_runtime_qualified(
            &format!("{config}BUZZ_RELAY_IMAGE=ghcr.io/example/other@{digest}\n"),
            &approved
        ));
    }

    #[test]
    fn relay_pubkey_accepts_64_lowercase_hex() {
        assert!(validate_relay_pubkey(&"ab".repeat(32)).is_ok());
    }

    #[test]
    fn relay_pubkey_rejects_uppercase_hex() {
        assert!(validate_relay_pubkey(&"AB".repeat(32)).is_err());
    }

    #[test]
    fn relay_pubkey_rejects_wrong_length() {
        assert!(validate_relay_pubkey(&"ab".repeat(31)).is_err());
        assert!(validate_relay_pubkey(&"ab".repeat(33)).is_err());
    }

    #[test]
    fn relay_pubkey_rejects_non_hex() {
        assert!(validate_relay_pubkey(&"g".repeat(64)).is_err());
    }

    #[tokio::test]
    async fn relay_url_accepts_ws_and_wss() {
        assert!(validate_relay_url("ws://1.1.1.1").await.is_ok());
        assert!(validate_relay_url("wss://1.1.1.1").await.is_ok());
    }

    #[tokio::test]
    async fn relay_url_rejects_http_scheme() {
        assert!(validate_relay_url("http://relay.example.test")
            .await
            .is_err());
        assert!(validate_relay_url("https://relay.example.test")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn relay_url_rejects_missing_host() {
        assert!(validate_relay_url("ws://").await.is_err());
    }

    #[tokio::test]
    async fn relay_url_rejects_non_url() {
        assert!(validate_relay_url("not a url").await.is_err());
    }

    #[tokio::test]
    async fn relay_url_rejects_private_or_credentialed_targets() {
        // Serialized with the buzz_gateway SSRF tests: they flip the
        // process-wide RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY flag, which this
        // validation path reads too.
        let _ssrf_guard = crate::buzz_gateway::SSRF_SERIAL.lock().await;
        assert!(validate_relay_url("ws://127.0.0.1:8080").await.is_err());
        assert!(validate_relay_url("wss://user:secret@1.1.1.1")
            .await
            .is_err());
        assert!(validate_relay_url("wss://1.1.1.1/?token=secret")
            .await
            .is_err());
    }
}
