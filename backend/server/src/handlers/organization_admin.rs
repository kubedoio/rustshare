//! Bounded tenant administration for explicitly granted organization admins.
use crate::{
    handlers::{
        admin::{
            admin_bad_request, admin_conflict, admin_internal_error, admin_not_found,
            insert_scoped_admin_action, request_correlation_id,
        },
        AppError, OrganizationAdmin,
    },
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub search: Option<String>,
    pub status: Option<String>,
}
#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PaginationQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrgUser {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub disabled_at: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Member {
    pub user_id: Uuid,
    pub username: String,
    pub email: String,
    pub added_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrgUsers {
    pub users: Vec<OrgUser>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrgGroups {
    pub groups: Vec<Group>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrgMembers {
    pub members: Vec<Member>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct GroupInput {
    pub name: String,
    pub description: Option<String>,
}
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MemberInput {
    pub user_id: Uuid,
}
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub member_count: i64,
}
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct GroupDetail {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub members: Vec<Member>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateGroupInput {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
}

fn pagination(page: Option<i64>, per_page: Option<i64>) -> Result<(i64, i64, i64), AppError> {
    let page = page.unwrap_or(1).max(1);
    let per_page = per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1)
        .checked_mul(per_page)
        .filter(|offset| *offset <= 100_000)
        .ok_or_else(|| admin_bad_request("Page exceeds the supported range"))?;
    Ok((page, per_page, offset))
}
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditEvent {
    pub id: Uuid,
    pub actor_id: Option<Uuid>,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub action_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<Uuid>,
    pub detail: serde_json::Value,
}
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditPage {
    pub entries: Vec<AuditEvent>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[utoipa::path(get, path="/api/v1/organization-admin/users", params(PageQuery), responses((status=200, body=OrgUsers)))]
pub async fn users(
    State(s): State<AppState>,
    OrganizationAdmin { tenant_id, .. }: OrganizationAdmin,
    Query(q): Query<PageQuery>,
) -> Result<Json<OrgUsers>, AppError> {
    let (page, per, off) = pagination(q.page, q.per_page)?;
    let status = q.status.as_deref().unwrap_or("all");
    if !matches!(status, "all" | "active" | "disabled") {
        return Err(admin_bad_request("Invalid status"));
    }
    let pat = q.search.map(|x| format!("%{x}%"));
    let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id=$1 AND ($2::text IS NULL OR username ILIKE $2 OR email ILIKE $2) AND ($3='all' OR ($3='active' AND disabled_at IS NULL) OR ($3='disabled' AND disabled_at IS NOT NULL))").bind(tenant_id).bind(&pat).bind(status).fetch_one(&s.db_pool).await.map_err(db_error)?;
    let rows = if off >= total {
        Vec::new()
    } else {
        sqlx::query_as::<_,(Uuid,String,String,String,Option<chrono::DateTime<chrono::Utc>>)>("SELECT id,username,email,display_name,disabled_at FROM users WHERE tenant_id=$1 AND ($2::text IS NULL OR username ILIKE $2 OR email ILIKE $2) AND ($3='all' OR ($3='active' AND disabled_at IS NULL) OR ($3='disabled' AND disabled_at IS NOT NULL)) ORDER BY created_at DESC,id LIMIT $4 OFFSET $5").bind(tenant_id).bind(pat).bind(status).bind(per).bind(off).fetch_all(&s.db_pool).await.map_err(db_error)?
    };
    Ok(Json(OrgUsers {
        users: rows
            .into_iter()
            .map(|r| OrgUser {
                id: r.0,
                username: r.1,
                email: r.2,
                display_name: r.3,
                disabled_at: r.4,
            })
            .collect(),
        total,
        page,
        per_page: per,
    }))
}
#[utoipa::path(get, path="/api/v1/organization-admin/users/{id}", params(("id"=Uuid,Path)), responses((status=200,body=OrgUser),(status=404)))]
pub async fn user(
    State(s): State<AppState>,
    OrganizationAdmin { tenant_id, .. }: OrganizationAdmin,
    Path(id): Path<Uuid>,
) -> Result<Json<OrgUser>, AppError> {
    let r = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        "SELECT id,username,email,display_name,disabled_at FROM users WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&s.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("User not found"))?;
    Ok(Json(OrgUser {
        id: r.0,
        username: r.1,
        email: r.2,
        display_name: r.3,
        disabled_at: r.4,
    }))
}

#[utoipa::path(get,path="/api/v1/organization-admin/groups",params(PaginationQuery),responses((status=200,body=OrgGroups)))]
pub async fn groups(
    State(s): State<AppState>,
    OrganizationAdmin { tenant_id, .. }: OrganizationAdmin,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<OrgGroups>, AppError> {
    let (page, per_page, offset) = pagination(q.page, q.per_page)?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_groups WHERE tenant_id=$1")
        .bind(tenant_id)
        .fetch_one(&s.db_pool)
        .await
        .map_err(db_error)?;
    let r = if offset >= total {
        Vec::new()
    } else {
        sqlx::query_as::<_, (Uuid, String, Option<String>, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, i64)>("SELECT g.id,g.name,g.description,g.created_at,g.updated_at,count(u.id) FROM user_groups g LEFT JOIN group_members m ON m.group_id=g.id LEFT JOIN users u ON u.id=m.user_id AND u.tenant_id=g.tenant_id WHERE g.tenant_id=$1 GROUP BY g.id ORDER BY g.created_at DESC LIMIT $2 OFFSET $3")
            .bind(tenant_id).bind(per_page).bind(offset).fetch_all(&s.db_pool).await.map_err(db_error)?
    };
    Ok(Json(OrgGroups {
        groups: r
            .into_iter()
            .map(|x| Group {
                id: x.0,
                name: x.1,
                description: x.2,
                created_at: x.3,
                updated_at: x.4,
                member_count: x.5,
            })
            .collect(),
        total,
        page,
        per_page,
    }))
}
#[utoipa::path(get,path="/api/v1/organization-admin/groups/{id}",params(("id"=Uuid,Path),PaginationQuery),responses((status=200,body=GroupDetail),(status=404)))]
pub async fn group(
    State(s): State<AppState>,
    OrganizationAdmin { tenant_id, .. }: OrganizationAdmin,
    Path(id): Path<Uuid>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<GroupDetail>, AppError> {
    let (page, per_page, offset) = pagination(q.page, q.per_page)?;
    let row = sqlx::query_as::<_, (Uuid, String, Option<String>)>(
        "SELECT id,name,description FROM user_groups WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&s.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("Group not found"))?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM group_members m JOIN users u ON u.id=m.user_id WHERE m.group_id=$1 AND u.tenant_id=$2")
        .bind(id).bind(tenant_id).fetch_one(&s.db_pool).await.map_err(db_error)?;
    let members = if offset >= total {
        Vec::new()
    } else {
        sqlx::query_as::<_,(Uuid,String,String,chrono::DateTime<chrono::Utc>)>("SELECT u.id,u.username,u.email,m.added_at FROM group_members m JOIN users u ON u.id=m.user_id WHERE m.group_id=$1 AND u.tenant_id=$2 ORDER BY m.added_at,u.id LIMIT $3 OFFSET $4")
            .bind(id).bind(tenant_id).bind(per_page).bind(offset).fetch_all(&s.db_pool).await.map_err(db_error)?
    };
    Ok(Json(GroupDetail {
        id: row.0,
        name: row.1,
        description: row.2,
        members: members
            .into_iter()
            .map(|m| Member {
                user_id: m.0,
                username: m.1,
                email: m.2,
                added_at: m.3,
            })
            .collect(),
        total,
        page,
        per_page,
    }))
}
#[utoipa::path(post,path="/api/v1/organization-admin/groups",request_body=GroupInput,responses((status=201,body=Group)))]
pub async fn create_group(
    State(s): State<AppState>,
    OrganizationAdmin { user_id, tenant_id }: OrganizationAdmin,
    h: HeaderMap,
    Json(input): Json<GroupInput>,
) -> Result<(StatusCode, Json<Group>), AppError> {
    if input.name.trim().is_empty() {
        return Err(admin_bad_request("Group name must not be empty"));
    }
    let mut tx = s.db_pool.begin().await.map_err(db_error)?;
    let r=sqlx::query_as::<_,(Uuid,String,Option<String>,chrono::DateTime<chrono::Utc>,chrono::DateTime<chrono::Utc>)>("INSERT INTO user_groups(id,name,description,created_by,tenant_id) VALUES(gen_random_uuid(),$1,$2,$3,$4) RETURNING id,name,description,created_at,updated_at").bind(&input.name).bind(&input.description).bind(user_id).bind(tenant_id).fetch_one(&mut *tx).await.map_err(db_error)?;
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        user_id,
        "group.created",
        Some("group"),
        Some(r.0),
        json!({"name":r.1}),
        request_correlation_id(&h),
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok((
        StatusCode::CREATED,
        Json(Group {
            id: r.0,
            name: r.1,
            description: r.2,
            created_at: r.3,
            updated_at: r.4,
            member_count: 0,
        }),
    ))
}
#[utoipa::path(patch,path="/api/v1/organization-admin/groups/{id}",request_body=UpdateGroupInput,responses((status=200,body=Group),(status=404)))]
pub async fn update_group(
    State(s): State<AppState>,
    OrganizationAdmin { user_id, tenant_id }: OrganizationAdmin,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateGroupInput>,
) -> Result<Json<Group>, AppError> {
    let mut tx = s.db_pool.begin().await.map_err(db_error)?;
    let current = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT name,description FROM user_groups WHERE id=$1 AND tenant_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("Group not found"))?;
    let name = input.name.unwrap_or(current.0);
    if name.trim().is_empty() {
        return Err(admin_bad_request("Group name must not be empty"));
    }
    let description = input.description.unwrap_or(current.1);
    let r=sqlx::query_as::<_,(Uuid,String,Option<String>,chrono::DateTime<chrono::Utc>,chrono::DateTime<chrono::Utc>,i64)>("UPDATE user_groups g SET name=$3,description=$4,updated_at=now() WHERE g.id=$1 AND g.tenant_id=$2 RETURNING g.id,g.name,g.description,g.created_at,g.updated_at,(SELECT count(*) FROM group_members m JOIN users u ON u.id=m.user_id AND u.tenant_id=$2 WHERE m.group_id=g.id)").bind(id).bind(tenant_id).bind(name).bind(description).fetch_one(&mut *tx).await.map_err(db_error)?;
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        user_id,
        "group.updated",
        Some("group"),
        Some(id),
        json!({"name":r.1}),
        request_correlation_id(&h),
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(Json(Group {
        id: r.0,
        name: r.1,
        description: r.2,
        created_at: r.3,
        updated_at: r.4,
        member_count: r.5,
    }))
}
#[utoipa::path(delete,path="/api/v1/organization-admin/groups/{id}",responses((status=204),(status=404)))]
pub async fn delete_group(
    State(s): State<AppState>,
    OrganizationAdmin { user_id, tenant_id }: OrganizationAdmin,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = s.db_pool.begin().await.map_err(db_error)?;
    let name: Option<String> =
        sqlx::query_scalar("SELECT name FROM user_groups WHERE id=$1 AND tenant_id=$2 FOR UPDATE")
            .bind(id)
            .bind(tenant_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_error)?;
    let name = name.ok_or_else(|| admin_not_found("Group not found"))?;
    let targets = crate::services::note_service::capture_group_share_targets(&mut tx, id)
        .await
        .map_err(db_error)?;
    sqlx::query("DELETE FROM user_groups WHERE id=$1 AND tenant_id=$2")
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        user_id,
        "group.deleted",
        Some("group"),
        Some(id),
        json!({"name":name}),
        request_correlation_id(&h),
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    crate::services::note_service::dispatch_share_target_acl_refresh(
        Arc::clone(&s.note_service),
        targets,
    );
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(get,path="/api/v1/organization-admin/groups/{id}/members",params(PaginationQuery),responses((status=200,body=OrgMembers)))]
pub async fn members(
    State(s): State<AppState>,
    OrganizationAdmin { tenant_id, .. }: OrganizationAdmin,
    Path(id): Path<Uuid>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<OrgMembers>, AppError> {
    let (page, per_page, offset) = pagination(q.page, q.per_page)?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_groups WHERE id=$1 AND tenant_id=$2)")
            .bind(id)
            .bind(tenant_id)
            .fetch_one(&s.db_pool)
            .await
            .map_err(db_error)?;
    if !exists {
        return Err(admin_not_found("Group not found"));
    }
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM group_members m JOIN users u ON u.id=m.user_id WHERE m.group_id=$1 AND u.tenant_id=$2")
        .bind(id).bind(tenant_id).fetch_one(&s.db_pool).await.map_err(db_error)?;
    let rows = if offset >= total {
        Vec::new()
    } else {
        sqlx::query_as::<_,(Uuid,String,String,chrono::DateTime<chrono::Utc>)>("SELECT u.id,u.username,u.email,m.added_at FROM group_members m JOIN users u ON u.id=m.user_id WHERE m.group_id=$1 AND u.tenant_id=$2 ORDER BY m.added_at,u.id LIMIT $3 OFFSET $4")
            .bind(id).bind(tenant_id).bind(per_page).bind(offset).fetch_all(&s.db_pool).await.map_err(db_error)?
    };
    Ok(Json(OrgMembers {
        members: rows
            .into_iter()
            .map(|r| Member {
                user_id: r.0,
                username: r.1,
                email: r.2,
                added_at: r.3,
            })
            .collect(),
        total,
        page,
        per_page,
    }))
}
#[utoipa::path(post,path="/api/v1/organization-admin/groups/{id}/members",request_body=MemberInput,responses((status=204),(status=404)))]
pub async fn add_member(
    State(s): State<AppState>,
    OrganizationAdmin { user_id, tenant_id }: OrganizationAdmin,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<MemberInput>,
) -> Result<StatusCode, AppError> {
    if input.user_id == user_id {
        return Err(AppError::forbidden(
            "Organization administrators cannot add themselves to groups",
        ));
    }
    let mut tx = s.db_pool.begin().await.map_err(db_error)?;
    let valid=sqlx::query("SELECT g.id FROM user_groups g JOIN users u ON u.tenant_id=g.tenant_id WHERE g.id=$1 AND g.tenant_id=$2 AND u.id=$3 FOR UPDATE OF g,u").bind(id).bind(tenant_id).bind(input.user_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
    if valid.is_none() {
        return Err(admin_not_found("Group or user not found"));
    }
    let res=sqlx::query("INSERT INTO group_members(group_id,user_id,added_by) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(id).bind(input.user_id).bind(user_id).execute(&mut *tx).await.map_err(db_error)?;
    if res.rows_affected() == 0 {
        return Err(admin_conflict("User is already a member of this group"));
    }
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        user_id,
        "group.member_added",
        Some("group"),
        Some(id),
        json!({"user_id":input.user_id}),
        request_correlation_id(&h),
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    crate::services::note_service::dispatch_group_index_acl_refresh(
        Arc::clone(&s.note_service),
        id,
    );
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(delete,path="/api/v1/organization-admin/groups/{id}/members/{user_id}",responses((status=204),(status=404)))]
pub async fn remove_member(
    State(s): State<AppState>,
    OrganizationAdmin {
        user_id: actor,
        tenant_id,
    }: OrganizationAdmin,
    h: HeaderMap,
    Path((id, user)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let mut tx = s.db_pool.begin().await.map_err(db_error)?;
    let res=sqlx::query("DELETE FROM group_members m USING user_groups g, users u WHERE m.group_id=g.id AND m.user_id=u.id AND g.id=$1 AND g.tenant_id=$2 AND u.id=$3 AND u.tenant_id=$2").bind(id).bind(tenant_id).bind(user).execute(&mut *tx).await.map_err(db_error)?;
    if res.rows_affected() == 0 {
        return Err(admin_not_found("Group, user, or membership not found"));
    }
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        actor,
        "group.member_removed",
        Some("group"),
        Some(id),
        json!({"user_id":user}),
        request_correlation_id(&h),
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    crate::services::note_service::dispatch_group_index_acl_refresh(
        Arc::clone(&s.note_service),
        id,
    );
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get,path="/api/v1/organization-admin/audit",params(PaginationQuery),responses((status=200,body=AuditPage)))]
pub async fn audit(
    State(s): State<AppState>,
    OrganizationAdmin { tenant_id, .. }: OrganizationAdmin,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<AuditPage>, AppError> {
    let (page, per, off) = pagination(q.page, q.per_page)?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM admin_actions WHERE tenant_id=$1 AND (action_type LIKE 'group.%' OR action_type LIKE 'organization_admin.%')")
        .bind(tenant_id)
        .fetch_one(&s.db_pool)
        .await
        .map_err(db_error)?;
    let rows = if off >= total {
        Vec::new()
    } else {
        sqlx::query_as::<_,(Uuid,Option<Uuid>,chrono::DateTime<chrono::Utc>,String,Option<String>,Option<Uuid>,Option<serde_json::Value>)>("SELECT id,COALESCE(actor_principal_id,actor_id),performed_at,action_type,target_type,target_id,detail FROM admin_actions WHERE tenant_id=$1 AND (action_type LIKE 'group.%' OR action_type LIKE 'organization_admin.%') ORDER BY performed_at DESC,id DESC LIMIT $2 OFFSET $3").bind(tenant_id).bind(per).bind(off).fetch_all(&s.db_pool).await.map_err(db_error)?
    };
    Ok(Json(AuditPage {
        entries: rows
            .into_iter()
            .map(|r| AuditEvent {
                id: r.0,
                actor_id: r.1,
                occurred_at: r.2,
                action_type: r.3.clone(),
                target_type: r.4,
                target_id: r.5,
                detail: super::admin::audit::project_audit_detail(
                    "admin_action",
                    &r.3,
                    &r.6.unwrap_or_else(|| json!({})),
                ),
            })
            .collect(),
        total,
        page,
        per_page: per,
    }))
}

fn db_error(e: sqlx::Error) -> AppError {
    tracing::error!("Organization admin database error: {e:?}");
    admin_internal_error("Database error")
}
