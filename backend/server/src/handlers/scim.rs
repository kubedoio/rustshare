//! SCIM-lite provisioning webhook handlers.
//!
//! Provides webhook-style SCIM endpoints for enterprise IdP integration.
//! These endpoints require bearer token authentication via RUSTSHARE_SCIM_BEARER_TOKEN.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum_extra::headers::{authorization::Bearer, Authorization, HeaderMapExt};
use rustshare_core::services::{ScimError, ScimGroup, ScimRepository, ScimService, ScimUser};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use super::admin::request_correlation_id;
use crate::services::scim_repository::delete_group_and_refresh;
pub use crate::services::scim_repository::ScimRepositoryImpl;
use crate::AppState;

// ---------------------------------------------------------------------------
// Authentication middleware for SCIM endpoints
// ---------------------------------------------------------------------------

/// Resolve the single organization this SCIM integration is bound to
/// (ADR-0040 D5).
///
/// `RUSTSHARE_SCIM_TENANT_ID` supplies the scope; when it is unset, an
/// explicitly configured `RUSTSHARE_DEFAULT_TENANT_ID` supplies it. Any other
/// state — unset with no explicit default, empty, or not a valid UUID — fails
/// closed: a SCIM token without exactly one valid organization must never
/// operate installation-wide.
pub(crate) fn resolve_scim_tenant_scope() -> Result<Uuid, String> {
    if let Ok(raw) = std::env::var("RUSTSHARE_SCIM_TENANT_ID") {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("RUSTSHARE_SCIM_TENANT_ID is set but empty".to_string());
        }
        return raw
            .parse::<Uuid>()
            .map_err(|_| format!("RUSTSHARE_SCIM_TENANT_ID is not a valid UUID: {raw:?}"));
    }
    if let Ok(raw) = std::env::var("RUSTSHARE_DEFAULT_TENANT_ID") {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("RUSTSHARE_DEFAULT_TENANT_ID is set but empty".to_string());
        }
        return raw
            .parse::<Uuid>()
            .map_err(|_| format!("RUSTSHARE_DEFAULT_TENANT_ID is not a valid UUID: {raw:?}"));
    }
    Err(
        "no SCIM organization scope configured: set RUSTSHARE_SCIM_TENANT_ID \
         to the organization this integration provisions"
            .to_string(),
    )
}

/// Resolve the SCIM organization scope or map the misconfiguration to a
/// fail-closed 500 response. The specific misconfiguration is logged
/// server-side; the response body stays generic.
fn scim_scope_or_error() -> Result<Uuid, (StatusCode, Json<serde_json::Value>)> {
    resolve_scim_tenant_scope().map_err(|detail| {
        tracing::error!("SCIM organization scope misconfigured: {}", detail);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "SCIM misconfigured",
                "detail": "SCIM organization scope is not configured correctly"
            })),
        )
    })
}

/// Verify SCIM bearer token from Authorization header.
fn verify_scim_token(
    headers: &axum::http::HeaderMap,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let expected_token = match std::env::var("RUSTSHARE_SCIM_BEARER_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => {
            tracing::error!("RUSTSHARE_SCIM_BEARER_TOKEN not configured");
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": "SCIM not configured",
                    "detail": "RUSTSHARE_SCIM_BEARER_TOKEN environment variable not set"
                })),
            ));
        }
    };

    let auth_header = headers
        .typed_get::<Authorization<Bearer>>()
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "Unauthorized",
                    "detail": "Missing Authorization header with Bearer token"
                })),
            )
        })?;

    // Use constant-time comparison to prevent timing attacks
    if !constant_time_eq::constant_time_eq(
        auth_header.token().as_bytes(),
        expected_token.as_bytes(),
    ) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "Unauthorized",
                "detail": "Invalid bearer token"
            })),
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
/// POST /api/v1/scim/users
/// Provision or update a user from SCIM data.
pub async fn provision_user(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(scim_user): Json<ScimUser>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    verify_scim_token(&headers)?;

    // Validate required fields
    if scim_user.external_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": "externalId is required"
            })),
        ));
    }

    if scim_user.user_name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": "userName is required"
            })),
        ));
    }

    // Create repository and service
    let repository = Arc::new(ScimRepositoryImpl::with_correlation_id(
        state.db_pool.clone(),
        request_correlation_id(&headers).map(str::to_owned),
    ));
    let scope = scim_scope_or_error()?;
    let service = ScimService::new(repository, scope, crate::default_storage_quota_bytes());

    match service.provision_user(scim_user).await {
        Ok(result) => {
            let status = match result.action {
                rustshare_core::services::ScimAction::Created => StatusCode::CREATED,
                rustshare_core::services::ScimAction::Updated => StatusCode::OK,
            };

            Ok((
                status,
                Json(json!({
                    "id": result.id.to_string(),
                    "external_id": result.external_id,
                    "action": match result.action {
                        rustshare_core::services::ScimAction::Created => "created",
                        rustshare_core::services::ScimAction::Updated => "updated",
                    }
                })),
            ))
        }
        Err(rustshare_core::services::ScimError::InvalidRequest(detail)) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": detail
            })),
        )),
        Err(rustshare_core::services::ScimError::Conflict(detail)) => Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "Conflict",
                "detail": detail
            })),
        )),
        Err(e) => {
            tracing::error!("SCIM provision_user error: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Internal server error",
                    "detail": e.to_string()
                })),
            ))
        }
    }
}

/// DELETE /api/v1/scim/users/{external_id}
/// Deprovision (disable) a user.
pub async fn deprovision_user(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(external_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    verify_scim_token(&headers)?;

    // URL decode the external_id
    let external_id = percent_encoding::percent_decode_str(&external_id)
        .decode_utf8()
        .map(|s| s.into_owned())
        .unwrap_or(external_id);

    let repository = Arc::new(ScimRepositoryImpl::with_correlation_id(
        state.db_pool.clone(),
        request_correlation_id(&headers).map(str::to_owned),
    ));
    let scope = scim_scope_or_error()?;
    let service = ScimService::new(repository, scope, crate::default_storage_quota_bytes());

    match service.deprovision_user(&external_id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(rustshare_core::services::ScimError::UserNotFound(_)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "Not Found",
                "detail": format!("User with external_id '{}' not found", external_id)
            })),
        )),
        Err(ScimError::Conflict(detail)) => Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "Conflict", "detail": detail })),
        )),
        Err(e) => {
            tracing::error!("SCIM deprovision_user error: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Internal server error",
                    "detail": e.to_string()
                })),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Group handlers
// ---------------------------------------------------------------------------

/// POST /api/v1/scim/groups
/// Provision or update a group from SCIM data.
pub async fn provision_group(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(scim_group): Json<ScimGroup>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    verify_scim_token(&headers)?;

    // Validate required fields
    if scim_group.external_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": "externalId is required"
            })),
        ));
    }

    if scim_group.display_name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": "displayName is required"
            })),
        ));
    }

    let repository = Arc::new(ScimRepositoryImpl::new(state.db_pool.clone()));
    let scope = scim_scope_or_error()?;
    let service = ScimService::new(repository, scope, crate::default_storage_quota_bytes());

    // Keep the directory key: on a failure after committed writes the group
    // is re-resolved so the membership mutation still gets its ACL refresh.
    let external_id = scim_group.external_id.clone();
    let outcome = service.provision_group(scim_group).await;

    // AR-04: SCIM commits membership writes statement by statement, so a
    // failure can follow committed changes. Refresh the group when the
    // provision succeeded or when it failed after writes committed; a
    // rejection that committed nothing must not dispatch (issue #367). The
    // service reports the boundary explicitly via `writes_committed`.
    let affected_group = match &outcome {
        Ok(result) => Some(result.id),
        Err(write_error) if write_error.writes_committed => {
            let repository = Arc::new(ScimRepositoryImpl::new(state.db_pool.clone()));
            match repository
                .find_group_by_external_id(scope, &external_id)
                .await
            {
                Ok(Some(group)) => Some(group.id),
                Ok(None) => {
                    tracing::warn!(
                        external_id = %external_id,
                        error = %write_error.error,
                        "SCIM provision failed after committed writes but re-resolved no group; skipping ACL refresh"
                    );
                    None
                }
                Err(resolution_error) => {
                    tracing::warn!(
                        external_id = %external_id,
                        error = %resolution_error,
                        "SCIM provision failed after committed writes and group re-resolution errored; skipping ACL refresh"
                    );
                    None
                }
            }
        }
        Err(write_error) => {
            tracing::debug!(
                external_id = %external_id,
                error = %write_error.error,
                "SCIM provision rejected before any write; skipping ACL refresh"
            );
            None
        }
    };
    if let Some(group_id) = affected_group {
        crate::services::note_service::dispatch_group_index_acl_refresh(
            Arc::clone(&state.note_service),
            group_id,
        );
    }

    match outcome.map_err(|write_error| write_error.error) {
        Ok(result) => {
            let status = match result.action {
                rustshare_core::services::ScimAction::Created => StatusCode::CREATED,
                rustshare_core::services::ScimAction::Updated => StatusCode::OK,
            };

            Ok((
                status,
                Json(json!({
                    "id": result.id.to_string(),
                    "external_id": result.external_id,
                    "action": match result.action {
                        rustshare_core::services::ScimAction::Created => "created",
                        rustshare_core::services::ScimAction::Updated => "updated",
                    }
                })),
            ))
        }
        Err(rustshare_core::services::ScimError::InvalidRequest(detail)) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": detail
            })),
        )),
        Err(rustshare_core::services::ScimError::Conflict(detail)) => Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "Conflict",
                "detail": detail
            })),
        )),
        Err(e) => {
            tracing::error!("SCIM provision_group error: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Internal server error",
                    "detail": e.to_string()
                })),
            ))
        }
    }
}

/// DELETE /api/v1/scim/groups/{external_id}
/// Delete a group.
pub async fn delete_group(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(external_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    verify_scim_token(&headers)?;

    // URL decode the external_id
    let external_id = percent_encoding::percent_decode_str(&external_id)
        .decode_utf8()
        .map(|s| s.into_owned())
        .unwrap_or(external_id);

    let scope = scim_scope_or_error()?;

    // Lock, capture, and delete stay atomic; ACL refresh starts only after commit.
    let deleted = delete_group_and_refresh(
        &state.db_pool,
        Arc::clone(&state.note_service),
        scope,
        &external_id,
    )
    .await
    .map_err(|(step, error)| scim_internal_error(step, error))?;
    if !deleted {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "Not Found",
                "detail": format!("Group with external_id '{}' not found", external_id)
            })),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map an internal SCIM group-deletion failure to a generic 500 response.
fn scim_internal_error(step: &str, error: sqlx::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("SCIM group deletion failed during {}: {:?}", step, error);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": "Internal server error",
            "detail": format!("Failed to delete group during {}", step)
        })),
    )
}
