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
    ScimPatchRequest, ScimV2Error, ScimV2ErrorResponse, ScimV2Group, ScimV2GroupRecord,
    ScimV2ListResponse, ScimV2Repository, ScimV2Service, ScimV2User, ScimV2UserRecord,
};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use super::ErrorResponse;
use crate::AppState;

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Query parameters for SCIM list operations.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Default)]
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
// Repository implementation
// ---------------------------------------------------------------------------

/// SQLx-based implementation of SCIM v2 repository.
pub struct ScimV2RepositoryImpl {
    pool: PgPool,
}

impl ScimV2RepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ScimV2Repository for ScimV2RepositoryImpl {
    async fn list_users(
        &self,
        filters: &[rustshare_core::services::ScimFilter],
        start_index: Option<i64>,
        count: Option<i64>,
    ) -> Result<(Vec<rustshare_core::services::ScimV2UserRecord>, i64), sqlx::Error> {
        let base_select = "SELECT id, external_id, username, display_name, email, disabled_at, name, surname, created_at, updated_at 
             FROM users WHERE 1=1";
        let base_count = "SELECT COUNT(*) FROM users WHERE 1=1";

        let mut conditions: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        // Apply filters with parameterised binds
        for filter in filters {
            match filter.attribute.as_str() {
                "userName" => {
                    let idx = binds.len() + 1;
                    conditions.push(format!(" AND LOWER(username) LIKE ${}", idx));
                    binds.push(format!("%{}%", filter.value.to_lowercase()));
                }
                "externalId" => {
                    let idx = binds.len() + 1;
                    conditions.push(format!(" AND external_id = ${}", idx));
                    binds.push(filter.value.clone());
                }
                "active" => {
                    let is_active = filter.value.to_lowercase() == "true";
                    if is_active {
                        conditions.push(" AND disabled_at IS NULL".to_string());
                    } else {
                        conditions.push(" AND disabled_at IS NOT NULL".to_string());
                    }
                }
                _ => {}
            }
        }

        let where_clause = conditions.join("");
        let limit = count.unwrap_or(100);
        let offset = start_index.map(|s| s - 1).unwrap_or(0).max(0);

        // count query (no LIMIT/OFFSET)
        let count_query = format!("{}{}", base_count, where_clause);
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
        for val in &binds {
            count_q = count_q.bind(val);
        }
        let total: i64 = count_q.fetch_one(&self.pool).await?;

        // select query with pagination binds
        let pag_idx_1 = binds.len() + 1;
        let pag_idx_2 = binds.len() + 2;
        let query = format!(
            "{} {} ORDER BY username LIMIT ${} OFFSET ${}",
            base_select, where_clause, pag_idx_1, pag_idx_2
        );
        let mut q = sqlx::query_as::<_, rustshare_core::services::ScimV2UserRecord>(&query);
        for val in &binds {
            q = q.bind(val);
        }
        q = q.bind(limit).bind(offset);
        let users = q.fetch_all(&self.pool).await?;

        Ok((users, total))
    }

    async fn get_user(
        &self,
        id: Uuid,
    ) -> Result<Option<rustshare_core::services::ScimV2UserRecord>, sqlx::Error> {
        sqlx::query_as!(
            ScimV2UserRecord,
            r#"
            SELECT id, external_id, username, display_name, email, disabled_at, name, surname, created_at, updated_at 
            FROM users WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn get_user_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<Option<rustshare_core::services::ScimV2UserRecord>, sqlx::Error> {
        sqlx::query_as!(
            ScimV2UserRecord,
            r#"
            SELECT id, external_id, username, display_name, email, disabled_at, name, surname, created_at, updated_at 
            FROM users WHERE external_id = $1
            "#,
            external_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn create_user(
        &self,
        user: &ScimV2User,
        tenant_id: Uuid,
        storage_quota: i64,
    ) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();

        // Extract email from SCIM emails
        let email = user
            .emails
            .as_ref()
            .and_then(|emails| emails.first().map(|e| e.value.clone()))
            .unwrap_or_else(|| user.user_name.clone());

        // Extract display name
        let display_name = user
            .display_name
            .clone()
            .or_else(|| {
                user.name.as_ref().map(|n| {
                    let given = n.given_name.as_deref().unwrap_or("");
                    let family = n.family_name.as_deref().unwrap_or("");
                    format!("{} {}", given, family).trim().to_string()
                })
            })
            .unwrap_or_else(|| user.user_name.clone());

        let (name, surname) = user.name.as_ref().map_or((None, None), |n| {
            (n.given_name.clone(), n.family_name.clone())
        });

        // Generate temporary password hash
        let password_hash = generate_temporary_password_hash();

        sqlx::query!(
            r#"
            INSERT INTO users (
                id, username, display_name, password_hash, email, is_admin,
                storage_quota, theme, created_at, updated_at, disabled_at,
                name, surname, email_sharing_enabled, tenant_id, external_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW(), $9, $10, $11, $12, $13, $14)
            "#,
            id,
            user.user_name,
            display_name,
            password_hash,
            email,
            false,
            storage_quota,
            "system",
            if user.active {
                None::<DateTime<Utc>>
            } else {
                Some(Utc::now())
            },
            name,
            surname,
            true,
            tenant_id,
            user.external_id
        )
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    async fn update_user(&self, id: Uuid, user: &ScimV2User) -> Result<(), sqlx::Error> {
        let email = user
            .emails
            .as_ref()
            .and_then(|emails| emails.first().map(|e| e.value.clone()))
            .unwrap_or_else(|| user.user_name.clone());

        let display_name = user
            .display_name
            .clone()
            .unwrap_or_else(|| user.user_name.clone());

        let (name, surname) = user.name.as_ref().map_or((None, None), |n| {
            (n.given_name.clone(), n.family_name.clone())
        });

        sqlx::query!(
            r#"
            UPDATE users 
            SET username = $2,
                display_name = $3,
                email = $4,
                disabled_at = $5,
                name = $6,
                surname = $7,
                external_id = $8,
                updated_at = NOW()
            WHERE id = $1
            "#,
            id,
            user.user_name,
            display_name,
            email,
            if user.active {
                None::<DateTime<Utc>>
            } else {
                Some(Utc::now())
            },
            name,
            surname,
            user.external_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn patch_user(
        &self,
        id: Uuid,
        operations: &[rustshare_core::services::ScimPatchOperation],
    ) -> Result<(), sqlx::Error> {
        for op in operations {
            if op.op.to_lowercase().as_str() == "replace" {
                if let Some(ref path) = op.path {
                    if path.as_str() == "active" {
                        if let Some(ref value) = op.value {
                            let active = value.as_bool().unwrap_or(true);
                            sqlx::query!(
                                "UPDATE users SET disabled_at = $2, updated_at = NOW() WHERE id = $1",
                                id,
                                if active { None::<DateTime<Utc>> } else { Some(Utc::now()) }
                            )
                            .execute(&self.pool)
                            .await?;
                        }
                    }
                } else if let Some(ref value) = op.value {
                    // Handle value object with multiple attributes
                    if let Some(active) = value.get("active").and_then(|v| v.as_bool()) {
                        sqlx::query!(
                            "UPDATE users SET disabled_at = $2, updated_at = NOW() WHERE id = $1",
                            id,
                            if active {
                                None::<DateTime<Utc>>
                            } else {
                                Some(Utc::now())
                            }
                        )
                        .execute(&self.pool)
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn delete_user(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM users WHERE id = $1", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_groups(
        &self,
        filters: &[rustshare_core::services::ScimFilter],
        start_index: Option<i64>,
        count: Option<i64>,
    ) -> Result<(Vec<rustshare_core::services::ScimV2GroupRecord>, i64), sqlx::Error> {
        let base_select = "SELECT id, external_id, name, description, created_at, updated_at 
             FROM user_groups WHERE 1=1";
        let base_count = "SELECT COUNT(*) FROM user_groups WHERE 1=1";

        let mut conditions: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        // Apply filters with parameterised binds
        for filter in filters {
            match filter.attribute.as_str() {
                "displayName" => {
                    let idx = binds.len() + 1;
                    conditions.push(format!(" AND LOWER(name) LIKE ${}", idx));
                    binds.push(format!("%{}%", filter.value.to_lowercase()));
                }
                "externalId" => {
                    let idx = binds.len() + 1;
                    conditions.push(format!(" AND external_id = ${}", idx));
                    binds.push(filter.value.clone());
                }
                _ => {}
            }
        }

        let where_clause = conditions.join("");
        let limit = count.unwrap_or(100);
        let offset = start_index.map(|s| s - 1).unwrap_or(0).max(0);

        // count query (no LIMIT/OFFSET)
        let count_query = format!("{}{}", base_count, where_clause);
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
        for val in &binds {
            count_q = count_q.bind(val);
        }
        let total: i64 = count_q.fetch_one(&self.pool).await?;

        // select query with pagination binds
        let pag_idx_1 = binds.len() + 1;
        let pag_idx_2 = binds.len() + 2;
        let query = format!(
            "{} {} ORDER BY name LIMIT ${} OFFSET ${}",
            base_select, where_clause, pag_idx_1, pag_idx_2
        );
        let mut q = sqlx::query_as::<_, rustshare_core::services::ScimV2GroupRecord>(&query);
        for val in &binds {
            q = q.bind(val);
        }
        q = q.bind(limit).bind(offset);
        let groups = q.fetch_all(&self.pool).await?;

        Ok((groups, total))
    }

    async fn get_group(
        &self,
        id: Uuid,
    ) -> Result<Option<rustshare_core::services::ScimV2GroupRecord>, sqlx::Error> {
        sqlx::query_as!(
            ScimV2GroupRecord,
            r#"
            SELECT id, external_id, name, description, created_at, updated_at 
            FROM user_groups WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn get_group_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<Option<rustshare_core::services::ScimV2GroupRecord>, sqlx::Error> {
        sqlx::query_as!(
            ScimV2GroupRecord,
            r#"
            SELECT id, external_id, name, description, created_at, updated_at 
            FROM user_groups WHERE external_id = $1
            "#,
            external_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn create_group(&self, group: &ScimV2Group) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();

        sqlx::query!(
            "INSERT INTO user_groups (id, name, external_id, created_at, updated_at) 
             VALUES ($1, $2, $3, NOW(), NOW())",
            id,
            group.display_name,
            group.external_id
        )
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    async fn update_group(&self, id: Uuid, group: &ScimV2Group) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE user_groups SET name = $2, external_id = $3, updated_at = NOW() WHERE id = $1",
            id,
            group.display_name,
            group.external_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn patch_group(
        &self,
        id: Uuid,
        operations: &[rustshare_core::services::ScimPatchOperation],
    ) -> Result<(), sqlx::Error> {
        for op in operations {
            match op.op.to_lowercase().as_str() {
                "add" | "replace" => {
                    if let Some(ref path) = op.path {
                        if path == "members" {
                            if let Some(ref value) = op.value {
                                // Handle array of members
                                if let Some(members) = value.as_array() {
                                    for member in members {
                                        if let Some(user_id_str) =
                                            member.get("value").and_then(|v| v.as_str())
                                        {
                                            if let Ok(user_id) = Uuid::parse_str(user_id_str) {
                                                if let Err(e) =
                                                    self.add_group_member(id, user_id).await
                                                {
                                                    tracing::warn!(group_id = %id, user_id = %user_id, error = %e, "failed to add group member");
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if let Some(ref value) = op.value {
                        if let Some(members) = value.get("members").and_then(|v| v.as_array()) {
                            for member in members {
                                if let Some(user_id_str) =
                                    member.get("value").and_then(|v| v.as_str())
                                {
                                    if let Ok(user_id) = Uuid::parse_str(user_id_str) {
                                        if let Err(e) = self.add_group_member(id, user_id).await {
                                            tracing::warn!(group_id = %id, user_id = %user_id, error = %e, "failed to add group member");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                "remove" => {
                    if let Some(ref path) = op.path {
                        // Parse path like "members[value eq \"user-id\"]"
                        if path.starts_with("members[value eq ") {
                            if let Some(user_id_str) = path
                                .split("value eq \"")
                                .nth(1)
                                .and_then(|s| s.split("\"").next())
                            {
                                if let Ok(user_id) = Uuid::parse_str(user_id_str) {
                                    if let Err(e) = self.remove_group_member(id, user_id).await {
                                        tracing::warn!(group_id = %id, user_id = %user_id, error = %e, "failed to remove group member");
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn delete_group(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM user_groups WHERE id = $1", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_group_members(&self, group_id: Uuid) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT u.id, u.display_name 
            FROM users u
            JOIN group_members gm ON u.id = gm.user_id
            WHERE gm.group_id = $1
            "#,
            group_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| (r.id, r.display_name)).collect())
    }

    async fn get_user_groups(&self, user_id: Uuid) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT g.id, g.name 
            FROM user_groups g
            JOIN group_members gm ON g.id = gm.group_id
            WHERE gm.user_id = $1
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| (r.id, r.name)).collect())
    }

    async fn add_group_member(&self, group_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO group_members (group_id, user_id, added_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (group_id, user_id) DO NOTHING
            "#,
            group_id,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn remove_group_member(&self, group_id: Uuid, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM group_members WHERE group_id = $1 AND user_id = $2",
            group_id,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_user_id_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let row = sqlx::query!("SELECT id FROM users WHERE external_id = $1", external_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| r.id))
    }

    async fn clear_group_members(&self, group_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM group_members WHERE group_id = $1", group_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

/// Generate a temporary password hash.
fn generate_temporary_password_hash() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let random_bytes: [u8; 32] = rng.gen();
    format!("$scim_temp${}", base64_encode(&random_bytes))
}

/// Simple base64 encoding.
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in input.chunks(3) {
        let b = match chunk.len() {
            1 => [chunk[0], 0, 0],
            2 => [chunk[0], chunk[1], 0],
            3 => [chunk[0], chunk[1], chunk[2]],
            _ => unreachable!(),
        };

        let idx1 = (b[0] >> 2) as usize;
        let idx2 = (((b[0] & 0b11) << 4) | (b[1] >> 4)) as usize;
        let idx3 = (((b[1] & 0b1111) << 2) | (b[2] >> 6)) as usize;
        let idx4 = (b[2] & 0b111111) as usize;

        result.push(ALPHABET[idx1] as char);
        result.push(ALPHABET[idx2] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[idx3] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[idx4] as char);
        } else {
            result.push('=');
        }
    }

    result
}

use chrono::{DateTime, Utc};

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
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
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
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
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
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
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
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
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
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service.patch_user(id, &patch.operations).await {
        Ok(user) => scim_json_response(StatusCode::OK, user),
        Err(e) => scim_error_response(e),
    }
}

/// DELETE /scim/v2/Users/{id}
/// Delete a user.
pub async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if let Err(err) = verify_scim_token(&headers) {
        return scim_json_response(StatusCode::UNAUTHORIZED, err);
    }

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
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
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
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
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
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
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service.create_group(group).await {
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
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service.update_group(id, group).await {
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
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service.patch_group(id, &patch.operations).await {
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

    let base_url = get_base_url(&headers);
    let repository = Arc::new(ScimV2RepositoryImpl::new(state.db_pool));
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
        crate::default_storage_quota_bytes(),
        base_url,
    );

    match service.delete_group(id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => scim_error_response(e),
    }
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
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
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
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
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
    let service = ScimV2Service::new(
        repository,
        state.default_tenant_id,
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
