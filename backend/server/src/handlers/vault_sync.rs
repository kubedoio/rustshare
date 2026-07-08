//! HTTP handlers for RustShare Vault Sync operations.
//!
//! Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with,
//! endorsed by, or sponsored by Obsidian.
//!
//! # Safety Guardrails
//!
//! - All write operations require `X-RustShare-Base-Server-Rev` and
//!   `X-RustShare-Device-ID` headers.
//! - Uploads require `X-RustShare-SHA256` for content-addressed storage.
//! - Stale writes return 409 Conflict via optimistic revision locking.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AppError, AuthenticatedUser};
use crate::AppState;
use rustshare_core::domain::{
    CreateVaultRequest, DeleteVaultFileRequest, RenameVaultFileRequest,
    SaveVaultFileContentRequest, UpdateVaultWritePolicyRequest, Vault, VaultDevice,
    VaultFileContentResponse, VaultFileContentSavedResponse,
};

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to register a new vault sync device.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateVaultDeviceRequest {
    pub device_name: String,
    pub client_type: String,
    pub client_version: Option<String>,
    pub vault_id: Option<Uuid>,
}

/// Response for listing vaults.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListVaultsResponse {
    pub vaults: Vec<Vault>,
}

/// Response for a successful vault file upload.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct VaultFileUploadResponse {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub relative_path: String,
    pub server_rev: i64,
    pub sha256: Option<String>,
    pub size: Option<i64>,
    pub updated_at: String,
}

impl From<rustshare_core::domain::VaultFile> for VaultFileUploadResponse {
    fn from(file: rustshare_core::domain::VaultFile) -> Self {
        Self {
            id: file.id,
            vault_id: file.vault_id,
            relative_path: file.relative_path,
            server_rev: file.server_rev,
            sha256: file.sha256,
            size: file.size,
            updated_at: file.updated_at.to_rfc3339(),
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn extract_header(headers: &HeaderMap, name: &str) -> Result<String, AppError> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::bad_request(format!("Missing header: {}", name)))
}

fn extract_header_i64(headers: &HeaderMap, name: &str) -> Result<i64, AppError> {
    let val: i64 = extract_header(headers, name)?
        .parse()
        .map_err(|_| AppError::bad_request(format!("Invalid integer header: {}", name)))?;
    if val < 0 {
        return Err(AppError::bad_request(format!(
            "Header {} cannot be negative",
            name
        )));
    }
    Ok(val)
}

fn extract_header_uuid(headers: &HeaderMap, name: &str) -> Result<Uuid, AppError> {
    let s = extract_header(headers, name)?;
    Uuid::parse_str(&s).map_err(|_| AppError::bad_request(format!("Invalid UUID header: {}", name)))
}

// ============================================================================
// Vault Management
// ============================================================================

/// Create a new vault.
///
/// POST /api/vault-sync/v1/vaults
#[utoipa::path(
    post,
    path = "/api/vault-sync/v1/vaults",
    tag = "Vault Sync",
    request_body = CreateVaultRequest,
    responses(
        (status = 200, description = "Success", body = Vault),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_vault(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateVaultRequest>,
) -> Result<(StatusCode, Json<Vault>), AppError> {
    let vault = state
        .vault_sync_service
        .create_vault(req, auth.tenant_id, auth.user_id)
        .await?;
    Ok((StatusCode::CREATED, Json(vault)))
}

/// List vaults for the authenticated user.
///
/// GET /api/vault-sync/v1/vaults
#[utoipa::path(
    get,
    path = "/api/vault-sync/v1/vaults",
    tag = "Vault Sync",
    responses(
        (status = 200, description = "Success", body = ListVaultsResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_vaults(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<ListVaultsResponse>, AppError> {
    let vaults = state
        .vault_sync_service
        .list_vaults(auth.tenant_id, auth.user_id)
        .await?;
    Ok(Json(ListVaultsResponse { vaults }))
}

/// Get a vault by ID.
///
/// GET /api/vault-sync/v1/vaults/:vault_id
#[utoipa::path(
    get,
    path = "/api/vault-sync/v1/vaults/{vault_id}",
    tag = "Vault Sync",
    params(("vault_id" = Uuid, Path, description = "Vault Id")),
    responses(
        (status = 200, description = "Success", body = Vault),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_vault(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(vault_id): Path<Uuid>,
) -> Result<Json<Vault>, AppError> {
    let vault = state
        .vault_sync_service
        .get_vault(vault_id, auth.tenant_id, auth.user_id)
        .await?;
    Ok(Json(vault))
}

/// Update the write policy for a vault.
///
/// PATCH /api/vault-sync/v1/vaults/:vault_id/write-policy
#[utoipa::path(
    patch,
    path = "/api/vault-sync/v1/vaults/{vault_id}/write-policy",
    tag = "Vault Sync",
    request_body = UpdateVaultWritePolicyRequest,
    params(("vault_id" = Uuid, Path, description = "Vault Id")),
    responses(
        (status = 200, description = "Success", body = Vault),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_vault_write_policy(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(vault_id): Path<Uuid>,
    Json(req): Json<UpdateVaultWritePolicyRequest>,
) -> Result<Json<Vault>, AppError> {
    let vault = state
        .vault_sync_service
        .update_vault_write_policy(vault_id, req.write_policy, auth.tenant_id, auth.user_id)
        .await?;
    Ok(Json(vault))
}

/// Get the manifest for a vault.
///
/// GET /api/vault-sync/v1/vaults/:vault_id/manifest
#[utoipa::path(
    get,
    path = "/api/vault-sync/v1/vaults/{vault_id}/manifest",
    tag = "Vault Sync",
    params(("vault_id" = Uuid, Path, description = "Vault Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_manifest(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(vault_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .vault_sync_service
        .get_manifest(vault_id, auth.tenant_id, auth.user_id)
        .await?;
    let mut headers = HeaderMap::new();
    if result.truncated {
        headers.insert(
            HeaderName::from_static("x-rustshare-manifest-truncated"),
            HeaderValue::from_static("true"),
        );
    }
    Ok((headers, Json(result.manifest)))
}

// ============================================================================
// File Operations
// ============================================================================

/// Download a file from a vault.
///
/// GET /api/vault-sync/v1/vaults/:vault_id/files/:path
#[utoipa::path(
    get,
    path = "/api/vault-sync/v1/vaults/{vault_id}/files/{*path}",
    tag = "Vault Sync",
    params(("vault_id" = Uuid, Path, description = "Vault Id"), ("path" = String, Path, description = "Path")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn download_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((vault_id, path)): Path<(Uuid, String)>,
) -> Result<Response, AppError> {
    let (bytes, content_type) = state
        .vault_sync_service
        .download_file(vault_id, &path, auth.tenant_id, auth.user_id)
        .await?;

    let mut headers = HeaderMap::new();
    let ct = content_type
        .as_deref()
        .unwrap_or("application/octet-stream");
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(ct)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&bytes.len().to_string()).unwrap(),
    );

    Ok((StatusCode::OK, headers, bytes).into_response())
}

/// Upload a file into a vault.
///
/// PUT /api/vault-sync/v1/vaults/:vault_id/files/:path
#[utoipa::path(
    put,
    path = "/api/vault-sync/v1/vaults/{vault_id}/files/{*path}",
    tag = "Vault Sync",
    params(("vault_id" = Uuid, Path, description = "Vault Id"), ("path" = String, Path, description = "Path")),
    responses(
        (status = 200, description = "Success", body = VaultFileUploadResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn upload_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((vault_id, path)): Path<(Uuid, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<VaultFileUploadResponse>), AppError> {
    let base_server_rev = extract_header_i64(&headers, "X-RustShare-Base-Server-Rev")?;
    let sha256 = extract_header(&headers, "X-RustShare-SHA256")?.to_lowercase();
    let device_id = extract_header_uuid(&headers, "X-RustShare-Device-ID")?.to_string();

    // Verify SHA256 of uploaded body against header
    let computed_sha256 = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(&body))
    };
    if computed_sha256 != sha256 {
        return Err(AppError::bad_request(format!(
            "SHA256 mismatch: header={}, computed={}",
            sha256, computed_sha256
        )));
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let req = rustshare_core::domain::UploadVaultFileRequest {
        vault_id,
        relative_path: path,
        content_type,
        sha256,
        size: body.len() as i64,
        base_server_rev,
        device_id,
        content: body,
    };

    let file = state
        .vault_sync_service
        .upload_file(req, auth.tenant_id, auth.user_id)
        .await?;
    Ok((StatusCode::OK, Json(file.into())))
}

/// Delete (tombstone) a file in a vault.
///
/// DELETE /api/vault-sync/v1/vaults/:vault_id/files/:path
#[utoipa::path(
    delete,
    path = "/api/vault-sync/v1/vaults/{vault_id}/files/{*path}",
    tag = "Vault Sync",
    params(("vault_id" = Uuid, Path, description = "Vault Id"), ("path" = String, Path, description = "Path")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn delete_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((vault_id, path)): Path<(Uuid, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    let base_server_rev = extract_header_i64(&headers, "X-RustShare-Base-Server-Rev")?;
    let device_id = extract_header_uuid(&headers, "X-RustShare-Device-ID")?.to_string();

    let req = DeleteVaultFileRequest {
        vault_id,
        relative_path: path,
        base_server_rev,
        device_id,
    };

    state
        .vault_sync_service
        .delete_file(req, auth.tenant_id, auth.user_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Rename a file within a vault.
///
/// POST /api/vault-sync/v1/vaults/:vault_id/rename
#[utoipa::path(
    post,
    path = "/api/vault-sync/v1/vaults/{vault_id}/rename",
    tag = "Vault Sync",
    params(("vault_id" = Uuid, Path, description = "Vault Id")),
    responses(
        (status = 200, description = "Success", body = VaultFileUploadResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn rename_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(vault_id): Path<Uuid>,
    headers: HeaderMap,
    Json(mut req): Json<RenameVaultFileRequest>,
) -> Result<Json<VaultFileUploadResponse>, AppError> {
    req.vault_id = vault_id;
    // Enforce header-based auth fields for consistency with upload/delete
    req.base_server_rev = extract_header_i64(&headers, "X-RustShare-Base-Server-Rev")?;
    req.device_id = extract_header_uuid(&headers, "X-RustShare-Device-ID")?.to_string();
    let file = state
        .vault_sync_service
        .rename_file(req, auth.tenant_id, auth.user_id)
        .await?;
    Ok(Json(file.into()))
}

/// Get file content for WebUI editing.
///
/// GET /api/vault-sync/v1/vaults/:vault_id/content/:path
#[utoipa::path(
    get,
    path = "/api/vault-sync/v1/vaults/{vault_id}/content/{*path}",
    tag = "Vault Sync",
    params(("vault_id" = Uuid, Path, description = "Vault Id"), ("path" = String, Path, description = "Path")),
    responses(
        (status = 200, description = "Success", body = VaultFileContentResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden/not editable", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_file_content(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((vault_id, path)): Path<(Uuid, String)>,
) -> Result<Json<VaultFileContentResponse>, AppError> {
    let response = state
        .vault_sync_service
        .get_file_content_for_webui(vault_id, &path, auth.tenant_id, auth.user_id)
        .await?;
    Ok(Json(response))
}

/// Save file content from the WebUI.
///
/// PUT /api/vault-sync/v1/vaults/:vault_id/content/:path
#[utoipa::path(
    put,
    path = "/api/vault-sync/v1/vaults/{vault_id}/content/{*path}",
    tag = "Vault Sync",
    request_body = SaveVaultFileContentRequest,
    params(("vault_id" = Uuid, Path, description = "Vault Id"), ("path" = String, Path, description = "Path")),
    responses(
        (status = 200, description = "Success", body = VaultFileContentSavedResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden/not editable", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
        (status = 409, description = "Conflict", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn save_file_content(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((vault_id, path)): Path<(Uuid, String)>,
    Json(req): Json<SaveVaultFileContentRequest>,
) -> Result<Json<VaultFileContentSavedResponse>, AppError> {
    let response = state
        .vault_sync_service
        .save_file_content_for_webui(vault_id, &path, req, auth.tenant_id, auth.user_id)
        .await?;
    Ok(Json(response))
}

// ============================================================================
// Device Management
// ============================================================================

/// Register a device for vault sync.
///
/// POST /api/vault-sync/v1/devices/register
#[utoipa::path(
    post,
    path = "/api/vault-sync/v1/devices/register",
    tag = "Vault Sync",
    request_body = CreateVaultDeviceRequest,
    responses(
        (status = 200, description = "Success", body = VaultDevice),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn register_device(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateVaultDeviceRequest>,
) -> Result<(StatusCode, Json<VaultDevice>), AppError> {
    if req.device_name.trim().is_empty() || req.device_name.len() > 255 {
        return Err(AppError::bad_request(
            "device_name must be 1-255 characters",
        ));
    }
    if req.client_type.trim().is_empty() || req.client_type.len() > 100 {
        return Err(AppError::bad_request(
            "client_type must be 1-100 characters",
        ));
    }

    let device = VaultDevice {
        id: Uuid::new_v4(),
        tenant_id: auth.tenant_id,
        user_id: auth.user_id,
        vault_id: req.vault_id,
        device_name: req.device_name,
        client_type: req.client_type,
        client_version: req.client_version,
        last_sync_rev: None,
        revoked_at: None,
        created_at: chrono::Utc::now(),
        last_seen_at: chrono::Utc::now(),
    };

    let device = state
        .vault_sync_service
        .register_device(device, auth.user_id)
        .await?;
    Ok((StatusCode::CREATED, Json(device)))
}

/// Revoke a vault sync device.
///
/// DELETE /api/vault-sync/v1/devices/{device_id}
#[utoipa::path(
    delete,
    path = "/api/vault-sync/v1/devices/{device_id}",
    tag = "Vault Sync",
    params(("device_id" = Uuid, Path, description = "Device Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn revoke_device(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(device_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .vault_sync_service
        .revoke_device(device_id, auth.tenant_id, auth.user_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
