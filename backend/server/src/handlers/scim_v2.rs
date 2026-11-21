//! SCIM v2 REST API handlers - Full RFC 7644 compliance.
//!
//! Provides complete SCIM v2 endpoints at /scim/v2/ for enterprise IdP integration.
//! Supports filtering, pagination, and standard schemas.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::headers::{authorization::Bearer, Authorization, HeaderMapExt};
use rustshare_core::services::{
    ScimPatchRequest, ScimV2Error, ScimV2ErrorResponse, ScimV2Group, ScimV2ListResponse,
    ScimV2Repository, ScimV2Service, ScimV2User,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use super::admin::request_correlation_id;
use super::scim::resolve_scim_tenant_scope;
use super::ErrorResponse;
use crate::services::scim_v2_repository::ScimV2RepositoryImpl;
use crate::AppState;

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Query parameters for SCIM list operations.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Default, utoipa::ToSchema)]
pub struct ListQuery {
    /// SCIM filter expression
    pub filter: Option<String>,
    /// 1-based index of the first result (SCIM uses 1-based indexing)
    #[serde(rename = "startIndex")]
    pub start_index: Option<i64>,
    /// Maximum number of results per page
    pub count: Option<i64>,
    /// Comma-separated list of attributes to return
    pub attributes: Option<String>,
    /// Comma-separated list of attributes to exclude
    #[serde(rename = "excludedAttributes")]
    pub excluded_attributes: Option<String>,
    /// Sort by attribute (not implemented)
    pub sort_by: Option<String>,
    /// Sort order (not implemented)
    pub sort_order: Option<String>,
}

// ---------------------------------------------------------------------------
// Authentication middleware
// ---------------------------------------------------------------------------

/// Verify SCIM bearer token from Authorization header.
fn verify_scim_token(headers: &HeaderMap) -> Result<(), ScimV2ErrorResponse> {
    let expected_token = match std::env::var("RUSTSHARE_SCIM_BEARER_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => {
            return Err(ScimV2ErrorResponse::new(
                503,
                "SCIM not configured: RUSTSHARE_SCIM_BEARER_TOKEN environment variable not set",
            ));
        }
    };

    let auth_header = headers
        .typed_get::<Authorization<Bearer>>()
        .ok_or_else(|| {
            ScimV2ErrorResponse::new(401, "Missing Authorization header with Bearer token")
        })?;

    // Use constant-time comparison to prevent timing attacks
    if !constant_time_eq::constant_time_eq(
        auth_header.token().as_bytes(),
        expected_token.as_bytes(),
    ) {
        return Err(ScimV2ErrorResponse::new(401, "Invalid bearer token"));
    }

    Ok(())
}

/// Resolve the SCIM organization scope or map the misconfiguration to a
/// fail-closed 500 response. The specific misconfiguration is logged
/// server-side; the response body stays generic.
fn scim_scope_or_response() -> Result<Uuid, Box<Response>> {
    resolve_scim_tenant_scope().map_err(|detail| {
        tracing::error!("SCIM organization scope misconfigured: {}", detail);
        Box::new(scim_json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ScimV2ErrorResponse::new(
                500,
                "SCIM organization scope is not configured correctly".to_string(),
            ),
        ))
    })
}

/// Create a SCIM JSON response.
fn scim_json_response<T: serde::Serialize>(status: StatusCode, body: T) -> Response {
    let json = match serde_json::to_string(&body) {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "JSON serialization error: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    (
        status,
        [(header::CONTENT_TYPE, "application/scim+json")],
        json,
    )
        .into_response()
}

/// Convert SCIM v2 error to HTTP response.
fn scim_error_response(err: ScimV2Error) -> Response {
    let status = err.status_code();
    let error_response = ScimV2ErrorResponse::new(status, err.to_string());
    scim_json_response(
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        error_response,
    )
}

// ---------------------------------------------------------------------------
// User handlers
// ---------------------------------------------------------------------------

/// GET /scim/v2/Users
/// List users with filtering and pagination.
pub async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service
        .list_users(query.filter.as_deref(), query.start_index, query.count)
        .await
    {
        Ok(response) => scim_json_response(StatusCode::OK, response),
        Err(e) => scim_error_response(e),
    }
}

/// GET /scim/v2/Users/{id}
/// Get a user by ID.
pub async fn get_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service.get_user(id).await {
        Ok(user) => scim_json_response(StatusCode::OK, user),
        Err(e) => scim_error_response(e),
    }
}

/// POST /scim/v2/Users
/// Create a new user.
pub async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(user): Json<ScimV2User>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service.create_user(user).await {
        Ok(user) => scim_json_response(StatusCode::CREATED, user),
        Err(e) => scim_error_response(e),
    }
}

/// PUT /scim/v2/Users/{id}
/// Update a user (full replacement).
pub async fn update_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(user): Json<ScimV2User>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::with_correlation_id(
        state.db_pool,
        request_correlation_id(&headers).map(str::to_owned),
    ));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service.update_user(id, user).await {
        Ok(user) => scim_json_response(StatusCode::OK, user),
        Err(e) => scim_error_response(e),
    }
}

/// PATCH /scim/v2/Users/{id}
/// Partially update a user.
pub async fn patch_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(patch): Json<ScimPatchRequest>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::with_correlation_id(
        state.db_pool,
        request_correlation_id(&headers).map(str::to_owned),
    ));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service.patch_user(id, &patch.operations).await {
        Ok(user) => scim_json_response(StatusCode::OK, user),
        Err(e) => scim_error_response(e),
    }
}

/// DELETE /scim/v2/Users/{id}
/// Deprovision a user while retaining the account and its owned data.
pub async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::with_correlation_id(
        state.db_pool,
        request_correlation_id(&headers).map(str::to_owned),
    ));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service.delete_user(id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => scim_error_response(e),
    }
}

// ---------------------------------------------------------------------------
// Group handlers
// ---------------------------------------------------------------------------

/// GET /scim/v2/Groups
/// List groups with filtering and pagination.
pub async fn list_groups(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service
        .list_groups(query.filter.as_deref(), query.start_index, query.count)
        .await
    {
        Ok(response) => scim_json_response(StatusCode::OK, response),
        Err(e) => scim_error_response(e),
    }
}

/// GET /scim/v2/Groups/{id}
/// Get a group by ID.
pub async fn get_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service.get_group(id).await {
        Ok(group) => scim_json_response(StatusCode::OK, group),
        Err(e) => scim_error_response(e),
    }
}

/// POST /scim/v2/Groups
/// Create a new group.
pub async fn create_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(group): Json<ScimV2Group>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository.clone(),
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    // AR-04: membership writes commit per statement. On a failure, refresh
    // only when writes committed (the service reports the boundary via
    // `writes_committed`); a rejection that committed nothing must not
    // dispatch (issue #367).
    let external_id = group.external_id.clone();
    let outcome = service.create_group(group).await;
    let affected_group = match &outcome {
        Ok(created) => created.id.clone(),
        Err(write_error) if write_error.writes_committed => {
            match repository
                .get_group_by_external_id(scope, external_id.as_deref().unwrap_or_default())
                .await
            {
                Ok(Some(record)) => Some(record.id.to_string()),
                Ok(None) => {
                    tracing::warn!(
                        external_id = external_id.as_deref().unwrap_or_default(),
                        error = %write_error.error,
                        "SCIM v2 group create failed after committed writes but re-resolved no group; skipping ACL refresh"
                    );
                    None
                }
                Err(resolution_error) => {
                    tracing::warn!(
                        external_id = external_id.as_deref().unwrap_or_default(),
                        error = %resolution_error,
                        "SCIM v2 group create failed after committed writes and group re-resolution errored; skipping ACL refresh"
                    );
                    None
                }
            }
        }
        Err(write_error) => {
            tracing::debug!(
                external_id = external_id.as_deref().unwrap_or_default(),
                error = %write_error.error,
                "SCIM v2 group create rejected before any write; skipping ACL refresh"
            );
            None
        }
    };
    if let Some(group_id) = affected_group.and_then(|id| Uuid::parse_str(&id).ok()) {
        crate::services::note_service::dispatch_group_index_acl_refresh(
            Arc::clone(&state.note_service),
            group_id,
        );
    }

    match outcome.map_err(|write_error| write_error.error) {
        Ok(group) => scim_json_response(StatusCode::CREATED, group),
        Err(e) => scim_error_response(e),
    }
}

/// PUT /scim/v2/Groups/{id}
/// Update a group (full replacement).
pub async fn update_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(group): Json<ScimV2Group>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    // AR-04: PUT replaces membership statement by statement. Refresh on
    // success or when the failure came after writes committed (the service
    // reports the boundary via `writes_committed`); a rejection that
    // committed nothing must not dispatch (issue #367). The group id is
    // known, so no re-resolution is needed — a concurrent deletion leaves
    // discovery with nothing to refresh.
    let outcome = service.update_group(id, group).await;
    match &outcome {
        Ok(_) => {
            crate::services::note_service::dispatch_group_index_acl_refresh(
                Arc::clone(&state.note_service),
                id,
            );
        }
        Err(write_error) if write_error.writes_committed => {
            // Intentional: no re-resolution here. If a concurrent deletion
            // removed the group, the refresh is a harmless no-op — share
            // discovery by group id finds nothing to refresh.
            crate::services::note_service::dispatch_group_index_acl_refresh(
                Arc::clone(&state.note_service),
                id,
            );
        }
        Err(write_error) => {
            tracing::debug!(
                group_id = %id,
                error = %write_error.error,
                "SCIM v2 group replacement rejected before any write; skipping ACL refresh"
            );
        }
    }

    match outcome.map_err(|write_error| write_error.error) {
        Ok(group) => scim_json_response(StatusCode::OK, group),
        Err(e) => scim_error_response(e),
    }
}

/// PATCH /scim/v2/Groups/{id}
/// Partially update a group (typically for membership changes).
pub async fn patch_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(patch): Json<ScimPatchRequest>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    // AR-04: PATCH mutates membership statement by statement. Refresh on
    // success or when the failure came after writes committed (see
    // `update_group`).
    let outcome = service.patch_group(id, &patch.operations).await;
    match &outcome {
        Ok(_) => {
            crate::services::note_service::dispatch_group_index_acl_refresh(
                Arc::clone(&state.note_service),
                id,
            );
        }
        Err(write_error) if write_error.writes_committed => {
            // Intentional: no re-resolution here. If a concurrent deletion
            // removed the group, the refresh is a harmless no-op — share
            // discovery by group id finds nothing to refresh.
            crate::services::note_service::dispatch_group_index_acl_refresh(
                Arc::clone(&state.note_service),
                id,
            );
        }
        Err(write_error) => {
            tracing::debug!(
                group_id = %id,
                error = %write_error.error,
                "SCIM v2 group patch rejected before any write; skipping ACL refresh"
            );
        }
    }

    match outcome.map_err(|write_error| write_error.error) {
        Ok(group) => scim_json_response(StatusCode::OK, group),
        Err(e) => scim_error_response(e),
    }
}

/// DELETE /scim/v2/Groups/{id}
/// Delete a group.
pub async fn delete_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };

    // AR-04: lock, capture, and delete in one transaction (same observable
    // semantics as `ScimV2Service::delete_group`, scoped to the organization).
    // The row lock serializes concurrent share creation, and the refresh of
    // the captured targets is dispatched only after a successful commit.
    let mut tx = match state.db_pool.begin().await {
        Ok(tx) => tx,
        Err(error) => return scim_error_response(ScimV2Error::Database(error.to_string())),
    };
    let locked = sqlx::query_scalar!(
        "SELECT id FROM user_groups WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
        id,
        scope
    )
    .fetch_optional(&mut *tx)
    .await;
    let locked = match locked {
        Ok(locked) => locked,
        Err(error) => return scim_error_response(ScimV2Error::Database(error.to_string())),
    };
    if locked.is_none() {
        return scim_error_response(ScimV2Error::GroupNotFound(id.to_string()));
    }
    let targets =
        match crate::services::note_service::capture_group_share_targets(&mut tx, id).await {
            Ok(targets) => targets,
            Err(error) => return scim_error_response(ScimV2Error::Database(error.to_string())),
        };
    let delete_result = sqlx::query!(
        "DELETE FROM user_groups WHERE id = $1 AND tenant_id = $2",
        id,
        scope
    )
    .execute(&mut *tx)
    .await;
    if let Err(error) = delete_result {
        return scim_error_response(ScimV2Error::Database(error.to_string()));
    }
    if let Err(error) = tx.commit().await {
        return scim_error_response(ScimV2Error::Database(error.to_string()));
    }

    crate::services::note_service::dispatch_share_target_acl_refresh(
        Arc::clone(&state.note_service),
        targets,
    );

    StatusCode::NO_CONTENT.into_response()
}

// ---------------------------------------------------------------------------
// Discovery handlers
// ---------------------------------------------------------------------------

/// GET /scim/v2/ServiceProviderConfig
/// Get service provider configuration.
pub async fn get_service_provider_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    let config = service.get_service_provider_config();
    scim_json_response(StatusCode::OK, config)
}

/// GET /scim/v2/ResourceTypes
/// Get supported resource types.
pub async fn get_resource_types(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    let resource_types = service.get_resource_types();

    let response = ScimV2ListResponse::new(resource_types, 2, Some(1), Some(2));

    scim_json_response(StatusCode::OK, response)
}

/// GET /scim/v2/Schemas
/// Get supported schemas.
pub async fn get_schemas(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let scope = match scim_scope_or_response() {
        Ok(scope) => scope,
        Err(response) => return *response,
    };
    let service = ScimV2Service::new(
        repository,
        scope,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    let schemas = service.get_schemas();

    let response = ScimV2ListResponse::new(schemas, 2, Some(1), Some(2));

    scim_json_response(StatusCode::OK, response)
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Get base URL for constructing resource locations.
fn get_base_url(_headers: &HeaderMap) -> String {
    // Try to construct from request headers, fallback to env or default
    std::env::var("RUSTSHARE_SCIM_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}
