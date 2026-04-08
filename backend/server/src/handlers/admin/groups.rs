//! Admin group management handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::log_admin_action;
use crate::{handlers::AdminUser, AppState};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct GroupResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub member_count: i64,
}

#[derive(Debug, Serialize)]
pub struct GroupDetailResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub members: Vec<GroupMemberResponse>,
}

#[derive(Debug, Serialize)]
pub struct GroupMemberResponse {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub added_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Internal row types
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct GroupRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    created_by: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    member_count: i64,
}

#[derive(sqlx::FromRow)]
struct GroupDetailRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    created_by: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    user_id: Uuid,
    username: String,
    email: String,
    added_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/groups
pub async fn list_groups(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<Vec<GroupResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let rows = sqlx::query_as::<_, GroupRow>(
        r#"
        SELECT
            g.id,
            g.name,
            g.description,
            g.created_by,
            g.created_at,
            g.updated_at,
            COUNT(m.id) AS member_count
        FROM user_groups g
        LEFT JOIN group_members m ON m.group_id = g.id
        GROUP BY g.id
        ORDER BY g.created_at DESC
        "#,
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(db_error)?;

    let groups = rows
        .into_iter()
        .map(|r| GroupResponse {
            id: r.id.to_string(),
            name: r.name,
            description: r.description,
            created_by: r.created_by.map(|u| u.to_string()),
            created_at: r.created_at,
            updated_at: r.updated_at,
            member_count: r.member_count,
        })
        .collect();

    Ok(Json(groups))
}

/// POST /api/v1/admin/groups
pub async fn create_group(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<GroupResponse>), (StatusCode, Json<serde_json::Value>)> {
    if req.name.trim().is_empty() {
        return Err(bad_request("Group name must not be empty"));
    }

    let new_id = Uuid::new_v4();

    let row = sqlx::query_as::<_, GroupDetailRow>(
        r#"
        INSERT INTO user_groups (id, name, description, created_by)
        VALUES ($1, $2, $3, $4)
        RETURNING id, name, description, created_by, created_at, updated_at
        "#,
    )
    .bind(new_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(actor_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("user_groups_name_key") {
                return conflict_error("A group with that name already exists");
            }
        }
        db_error(e)
    })?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "group.created",
        Some("group"),
        Some(new_id),
        json!({"name": req.name}),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(GroupResponse {
            id: row.id.to_string(),
            name: row.name,
            description: row.description,
            created_by: row.created_by.map(|u| u.to_string()),
            created_at: row.created_at,
            updated_at: row.updated_at,
            member_count: 0,
        }),
    ))
}

/// GET /api/v1/admin/groups/:id
pub async fn get_group(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(group_id): Path<Uuid>,
) -> Result<Json<GroupDetailResponse>, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, GroupDetailRow>(
        "SELECT id, name, description, created_by, created_at, updated_at
         FROM user_groups WHERE id = $1",
    )
    .bind(group_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("Group not found"))?;

    let members = sqlx::query_as::<_, MemberRow>(
        r#"
        SELECT u.id AS user_id, u.username, u.email, m.added_at
        FROM group_members m
        JOIN users u ON u.id = m.user_id
        WHERE m.group_id = $1
        ORDER BY m.added_at ASC
        "#,
    )
    .bind(group_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(db_error)?;

    Ok(Json(GroupDetailResponse {
        id: row.id.to_string(),
        name: row.name,
        description: row.description,
        created_by: row.created_by.map(|u| u.to_string()),
        created_at: row.created_at,
        updated_at: row.updated_at,
        members: members
            .into_iter()
            .map(|m| GroupMemberResponse {
                user_id: m.user_id.to_string(),
                username: m.username,
                email: m.email,
                added_at: m.added_at,
            })
            .collect(),
    }))
}

/// PATCH /api/v1/admin/groups/:id
pub async fn update_group(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(group_id): Path<Uuid>,
    Json(req): Json<UpdateGroupRequest>,
) -> Result<Json<GroupResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Fetch current group
    let current = sqlx::query_as::<_, GroupDetailRow>(
        "SELECT id, name, description, created_by, created_at, updated_at
         FROM user_groups WHERE id = $1",
    )
    .bind(group_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("Group not found"))?;

    let new_name = req.name.as_deref().unwrap_or(&current.name).to_string();
    if new_name.trim().is_empty() {
        return Err(bad_request("Group name must not be empty"));
    }
    // description: None means "keep current"; Some(None) not possible with this type,
    // so we use Option<String>: None = keep, Some(v) = set (including clearing to empty).
    let new_description = match &req.description {
        Some(v) => Some(v.clone()),
        None => current.description.clone(),
    };

    let updated = sqlx::query_as::<_, GroupDetailRow>(
        r#"
        UPDATE user_groups
        SET name = $2, description = $3, updated_at = NOW()
        WHERE id = $1
        RETURNING id, name, description, created_by, created_at, updated_at
        "#,
    )
    .bind(group_id)
    .bind(&new_name)
    .bind(&new_description)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("user_groups_name_key") {
                return conflict_error("A group with that name already exists");
            }
        }
        db_error(e)
    })?
    .ok_or_else(|| not_found("Group not found"))?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "group.updated",
        Some("group"),
        Some(group_id),
        json!({"name": updated.name}),
    )
    .await;

    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = $1")
            .bind(group_id)
            .fetch_one(&state.db_pool)
            .await
            .map_err(db_error)?;

    Ok(Json(GroupResponse {
        id: updated.id.to_string(),
        name: updated.name,
        description: updated.description,
        created_by: updated.created_by.map(|u| u.to_string()),
        created_at: updated.created_at,
        updated_at: updated.updated_at,
        member_count,
    }))
}

/// DELETE /api/v1/admin/groups/:id
pub async fn delete_group(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(group_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Verify group exists and grab name for audit log
    let row = sqlx::query_as::<_, GroupDetailRow>(
        "SELECT id, name, description, created_by, created_at, updated_at
         FROM user_groups WHERE id = $1",
    )
    .bind(group_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("Group not found"))?;

    // Log before deletion so the target still exists in the DB
    log_admin_action(
        &state.db_pool,
        actor_id,
        "group.deleted",
        Some("group"),
        Some(group_id),
        json!({"name": row.name}),
    )
    .await;

    // CASCADE on group_members handles membership rows
    sqlx::query("DELETE FROM user_groups WHERE id = $1")
        .bind(group_id)
        .execute(&state.db_pool)
        .await
        .map_err(db_error)?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/admin/groups/:id/members
pub async fn add_member(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(group_id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Verify group exists
    let group_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_groups WHERE id = $1)")
            .bind(group_id)
            .fetch_one(&state.db_pool)
            .await
            .map_err(db_error)?;
    if !group_exists {
        return Err(not_found("Group not found"));
    }

    // Verify user exists
    let user_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(req.user_id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(db_error)?;
    if !user_exists {
        return Err(not_found("User not found"));
    }

    let result = sqlx::query(
        r#"
        INSERT INTO group_members (group_id, user_id, added_by)
        VALUES ($1, $2, $3)
        ON CONFLICT (group_id, user_id) DO NOTHING
        "#,
    )
    .bind(group_id)
    .bind(req.user_id)
    .bind(actor_id)
    .execute(&state.db_pool)
    .await
    .map_err(db_error)?;

    if result.rows_affected() == 0 {
        return Err(conflict_error("User is already a member of this group"));
    }

    log_admin_action(
        &state.db_pool,
        actor_id,
        "group.member_added",
        Some("group"),
        Some(group_id),
        json!({"user_id": req.user_id}),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/admin/groups/:id/members/:user_id
pub async fn remove_member(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path((group_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let result = sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
        .bind(group_id)
        .bind(user_id)
        .execute(&state.db_pool)
        .await
        .map_err(db_error)?;

    if result.rows_affected() == 0 {
        return Err(not_found("Membership not found"));
    }

    log_admin_action(
        &state.db_pool,
        actor_id,
        "group.member_removed",
        Some("group"),
        Some(group_id),
        json!({"user_id": user_id}),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn db_error(e: sqlx::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("Database error: {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "Database error" })),
    )
}

fn not_found(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": msg })))
}

fn bad_request(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg })))
}

fn conflict_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::CONFLICT, Json(json!({ "error": msg })))
}
