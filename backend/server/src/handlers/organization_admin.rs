//! Bounded tenant administration for explicitly granted organization admins.
pub mod invitations;

use crate::{
    handlers::{
        admin::organization_administrators::{
            lock_tenant_admin_grants, removal_would_leave_no_enabled_admin, revoke_for_lifecycle,
        },
        admin::{
            admin_bad_request, admin_conflict, admin_internal_error, admin_not_found,
            insert_scoped_admin_action, request_correlation_id,
        },
        AppError, OrganizationAdmin,
    },
    user_disable::{set_user_disabled_at, DisableUserTarget},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

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
    #[serde(default, deserialize_with = "deserialize_present_nullable_string")]
    pub description: Option<Option<String>>,
}

fn deserialize_present_nullable_string<'de, D>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrganizationAdminCapability {
    pub allowed: bool,
}

/// Live authorization probe for the organization-administration shell.
/// The extractor resolves the active tenant grant from PostgreSQL on each request.
#[utoipa::path(
    get,
    path = "/api/v1/organization-admin/capability",
    tag = "Organization Admin",
    responses(
        (status = 200, body = OrganizationAdminCapability),
        (status = 403, body = crate::handlers::ErrorResponse)
    )
)]
pub async fn capability(
    OrganizationAdmin { .. }: OrganizationAdmin,
) -> Json<OrganizationAdminCapability> {
    Json(OrganizationAdminCapability { allowed: true })
}

const MAX_BULK_INVITES: usize = 100;
const MAX_BULK_INVITE_DELIVERY_CONCURRENCY: usize = 10;
// shortcut: this cap is per process; use a shared queue for fleet-wide delivery coordination.
static BULK_INVITE_DELIVERY_SEMAPHORE: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(MAX_BULK_INVITE_DELIVERY_CONCURRENCY);

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct BulkInviteRequest {
    pub dry_run: bool,
    pub users: Vec<BulkInviteUser>,
}

#[derive(Debug, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct BulkInviteUser {
    #[validate(email)]
    pub email: String,
    pub display_name: String,
    #[serde(default)]
    pub group_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BulkInviteRow {
    pub row: usize,
    pub outcome: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BulkInviteResponse {
    pub dry_run: bool,
    pub rows: Vec<BulkInviteRow>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OffboardUsersRequest {
    pub dry_run: bool,
    pub user_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OffboardUserRow {
    pub user_id: Uuid,
    pub outcome: String,
    pub shares_revoked: u64,
    pub group_memberships_removed: u64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OffboardUsersResponse {
    pub dry_run: bool,
    pub rows: Vec<OffboardUserRow>,
}

#[derive(sqlx::FromRow)]
struct PendingBulkInvite {
    tenant_id: Uuid,
    id: Uuid,
    bulk_delivery_attempted_at: Option<chrono::DateTime<chrono::Utc>>,
    bulk_delivery_status: String,
    recipient_display_name: Option<String>,
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
    let total:i64=sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE tenant_id=$1 AND ($2::text IS NULL OR username ILIKE $2 OR email ILIKE $2 OR display_name ILIKE $2) AND ($3='all' OR ($3='active' AND disabled_at IS NULL) OR ($3='disabled' AND disabled_at IS NOT NULL))").bind(tenant_id).bind(&pat).bind(status).fetch_one(&s.db_pool).await.map_err(db_error)?;
    let rows = if off >= total {
        Vec::new()
    } else {
        sqlx::query_as::<_,(Uuid,String,String,String,Option<chrono::DateTime<chrono::Utc>>)>("SELECT id,username,email,display_name,disabled_at FROM users WHERE tenant_id=$1 AND ($2::text IS NULL OR username ILIKE $2 OR email ILIKE $2 OR display_name ILIKE $2) AND ($3='all' OR ($3='active' AND disabled_at IS NULL) OR ($3='disabled' AND disabled_at IS NOT NULL)) ORDER BY created_at DESC,id LIMIT $4 OFFSET $5").bind(tenant_id).bind(pat).bind(status).bind(per).bind(off).fetch_all(&s.db_pool).await.map_err(db_error)?
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

/// Disable up to 100 tenant users. Active shares issued by or directly granted
/// to each user are revoked, and the user's group memberships are removed.
/// Group shares issued by others remain available to remaining group members.
#[utoipa::path(post, path="/api/v1/organization-admin/users/offboard", request_body=OffboardUsersRequest, responses((status=200, body=OffboardUsersResponse),(status=400),(status=403)))]
pub async fn offboard_users(
    State(state): State<AppState>,
    OrganizationAdmin {
        user_id: actor_id,
        tenant_id,
    }: OrganizationAdmin,
    headers: HeaderMap,
    Json(request): Json<OffboardUsersRequest>,
) -> Result<Json<OffboardUsersResponse>, AppError> {
    if request.user_ids.is_empty() || request.user_ids.len() > MAX_BULK_INVITES {
        return Err(admin_bad_request("Provide between 1 and 100 user IDs"));
    }

    let mut rows = Vec::with_capacity(MAX_BULK_INVITES);
    let mut seen = std::collections::HashSet::new();
    let mut active_admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM organization_administrators oa JOIN users u ON u.id=oa.user_id AND u.tenant_id=oa.tenant_id WHERE oa.tenant_id=$1 AND oa.revoked_at IS NULL AND u.disabled_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(db_error)?;

    for user_id in request.user_ids {
        let mut row = OffboardUserRow {
            user_id,
            outcome: String::new(),
            shares_revoked: 0,
            group_memberships_removed: 0,
        };
        if !seen.insert(user_id) {
            row.outcome = "duplicate".into();
        } else if user_id == actor_id {
            row.outcome = "self_offboarding_rejected".into();
        } else {
            let target = sqlx::query_as::<_, (bool, bool)>(
                "SELECT u.disabled_at IS NOT NULL, EXISTS(SELECT 1 FROM organization_administrators oa WHERE oa.tenant_id=u.tenant_id AND oa.user_id=u.id AND oa.revoked_at IS NULL) FROM users u WHERE u.id=$1 AND u.tenant_id=$2",
            )
            .bind(user_id)
            .bind(tenant_id)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(db_error)?;
            match target {
                None => row.outcome = "not_found".into(),
                Some((true, _)) => {
                    row.outcome = if request.dry_run {
                        "already_disabled".into()
                    } else {
                        match apply_offboarding(&state, tenant_id, actor_id, user_id, &headers)
                            .await
                        {
                            Ok((outcome, _, _)) => outcome,
                            Err(_) => "apply_failed".into(),
                        }
                    };
                }
                Some((false, is_admin)) if is_admin && active_admins <= 1 => {
                    row.outcome = "last_enabled_admin".into();
                }
                Some((false, is_admin)) => {
                    if request.dry_run {
                        if is_admin {
                            active_admins -= 1;
                        }
                        row.outcome = "ready".into();
                    } else {
                        match apply_offboarding(&state, tenant_id, actor_id, user_id, &headers)
                            .await
                        {
                            Ok((outcome, shares, memberships)) => {
                                if is_admin && outcome == "disabled" {
                                    active_admins -= 1;
                                }
                                row.outcome = outcome;
                                row.shares_revoked = shares;
                                row.group_memberships_removed = memberships;
                            }
                            Err(_) => row.outcome = "apply_failed".into(),
                        }
                    }
                }
            }
        }
        rows.push(row);
    }

    Ok(Json(OffboardUsersResponse {
        dry_run: request.dry_run,
        rows,
    }))
}

async fn apply_offboarding(
    state: &AppState,
    tenant_id: Uuid,
    actor_id: Uuid,
    user_id: Uuid,
    headers: &HeaderMap,
) -> Result<(String, u64, u64), sqlx::Error> {
    let mut tx = state.db_pool.begin().await?;
    lock_tenant_admin_grants(&mut tx, tenant_id).await?;
    let target = sqlx::query_as::<_, (Option<chrono::DateTime<chrono::Utc>>, bool)>(
        "SELECT u.disabled_at, EXISTS(SELECT 1 FROM organization_administrators oa WHERE oa.tenant_id=u.tenant_id AND oa.user_id=u.id AND oa.revoked_at IS NULL) FROM users u WHERE u.id=$1 AND u.tenant_id=$2 FOR UPDATE",
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((disabled_at, is_admin)) = target else {
        tx.rollback().await?;
        return Ok(("not_found".into(), 0, 0));
    };
    if disabled_at.is_some() {
        insert_scoped_admin_action(
            &mut tx,
            tenant_id,
            actor_id,
            "organization_admin.user_offboard_retry",
            Some("user"),
            Some(user_id),
            json!({"outcome":"already_disabled"}),
            request_correlation_id(headers),
        )
        .await?;
        tx.commit().await?;
        return Ok(("already_disabled".into(), 0, 0));
    }
    if is_admin && removal_would_leave_no_enabled_admin(&mut tx, tenant_id, user_id).await? {
        tx.rollback().await?;
        return Ok(("last_enabled_admin".into(), 0, 0));
    }
    revoke_for_lifecycle(
        &mut tx,
        tenant_id,
        Some(actor_id),
        user_id,
        "organization_admin",
        request_correlation_id(headers),
    )
    .await?;
    let shares = sqlx::query(
        "UPDATE shares SET revoked_at=COALESCE(revoked_at,NOW()) WHERE tenant_id=$1 AND revoked_at IS NULL AND (created_by=$2 OR recipient_user_id=$2)",
    ).bind(tenant_id).bind(user_id).execute(&mut *tx).await?.rows_affected();
    let removed_memberships = sqlx::query_as::<
        _,
        (Uuid, Option<Uuid>, chrono::DateTime<chrono::Utc>),
    >(
        "DELETE FROM group_members gm USING user_groups g WHERE gm.group_id=g.id AND g.tenant_id=$1 AND gm.user_id=$2 RETURNING gm.group_id,gm.added_by,gm.added_at",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;
    let memberships = removed_memberships.len() as u64;
    let affected_groups = removed_memberships
        .iter()
        .map(|(group_id, _, _)| *group_id)
        .collect::<Vec<_>>();
    set_user_disabled_at(
        &mut *tx,
        DisableUserTarget::Id(user_id),
        None,
        Some(chrono::Utc::now()),
    )
    .await?;
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        actor_id,
        "organization_admin.user_offboarded",
        Some("user"),
        Some(user_id),
        json!({
            "shares_revoked":shares,
            "group_memberships_removed":memberships,
            "group_memberships":removed_memberships.iter().map(|(group_id, added_by, added_at)| {
                json!({"group_id":group_id,"added_by":added_by,"added_at":added_at})
            }).collect::<Vec<_>>(),
        }),
        request_correlation_id(headers),
    )
    .await?;
    tx.commit().await?;
    for group_id in affected_groups {
        crate::services::note_service::dispatch_group_index_acl_refresh(
            Arc::clone(&state.note_service),
            group_id,
        );
    }
    Ok(("disabled".into(), shares, memberships))
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
#[utoipa::path(post,path="/api/v1/organization-admin/groups",request_body=GroupInput,responses((status=201,body=Group),(status=409)))]
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
    let r=sqlx::query_as::<_,(Uuid,String,Option<String>,chrono::DateTime<chrono::Utc>,chrono::DateTime<chrono::Utc>)>("INSERT INTO user_groups(id,name,description,created_by,tenant_id) VALUES(gen_random_uuid(),$1,$2,$3,$4) RETURNING id,name,description,created_at,updated_at").bind(&input.name).bind(&input.description).bind(user_id).bind(tenant_id).fetch_one(&mut *tx).await.map_err(group_db_error)?;
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
#[utoipa::path(patch,path="/api/v1/organization-admin/groups/{id}",params(("id"=Uuid,Path)),request_body=UpdateGroupInput,responses((status=200,body=Group),(status=404),(status=409)))]
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
    let r=sqlx::query_as::<_,(Uuid,String,Option<String>,chrono::DateTime<chrono::Utc>,chrono::DateTime<chrono::Utc>,i64)>("UPDATE user_groups g SET name=$3,description=$4,updated_at=now() WHERE g.id=$1 AND g.tenant_id=$2 RETURNING g.id,g.name,g.description,g.created_at,g.updated_at,(SELECT count(*) FROM group_members m JOIN users u ON u.id=m.user_id AND u.tenant_id=$2 WHERE m.group_id=g.id)").bind(id).bind(tenant_id).bind(name).bind(description).fetch_one(&mut *tx).await.map_err(group_db_error)?;
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
#[utoipa::path(delete,path="/api/v1/organization-admin/groups/{id}",params(("id"=Uuid,Path)),responses((status=204),(status=404)))]
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
#[utoipa::path(get,path="/api/v1/organization-admin/groups/{id}/members",params(("id"=Uuid,Path),PaginationQuery),responses((status=200,body=OrgMembers)))]
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
#[utoipa::path(post,path="/api/v1/organization-admin/groups/{id}/members",params(("id"=Uuid,Path)),request_body=MemberInput,responses((status=204),(status=404)))]
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
#[utoipa::path(delete,path="/api/v1/organization-admin/groups/{id}/members/{user_id}",params(("id"=Uuid,Path),("user_id"=Uuid,Path)),responses((status=204),(status=404)))]
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

fn group_db_error(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db_error) = &error {
        if db_error.constraint() == Some("user_groups_name_key") {
            return admin_conflict("Group name is unavailable");
        }
    }
    db_error(error)
}

#[utoipa::path(
    post,
    path = "/api/v1/organization-admin/users/bulk",
    request_body = BulkInviteRequest,
    responses((status = 200, body = BulkInviteResponse), (status = 400), (status = 403))
)]
pub async fn bulk_invite_users(
    State(s): State<AppState>,
    OrganizationAdmin { user_id, tenant_id }: OrganizationAdmin,
    h: HeaderMap,
    Json(request): Json<BulkInviteRequest>,
) -> Result<Json<BulkInviteResponse>, AppError> {
    if request.users.is_empty() || request.users.len() > MAX_BULK_INVITES {
        return Err(admin_bad_request(
            "Batch must contain between 1 and 100 users",
        ));
    }

    let mut seen = std::collections::HashSet::new();
    let mut rows = Vec::new();
    for (index, row) in request.users.iter().enumerate() {
        let email = row.email.trim().to_ascii_lowercase();
        let outcome = if !seen.insert(email) {
            "duplicate_in_request"
        } else {
            let mut conn = s.db_pool.acquire().await.map_err(db_error)?;
            bulk_invite_validation(&mut conn, tenant_id, row).await?
        };
        rows.push(BulkInviteRow {
            row: index + 1,
            outcome: outcome.to_string(),
        });
    }

    if request.dry_run {
        return Ok(Json(BulkInviteResponse {
            dry_run: true,
            rows,
        }));
    }

    let workflow = sqlx::query_as::<_, (Uuid, Option<String>, Option<String>)>(
        "SELECT id, subject, body FROM workflows WHERE key='invite_email' AND status='active'",
    )
    .fetch_optional(&s.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_bad_request("Invite workflow is not active"))?;
    let sender_name: String =
        sqlx::query_scalar("SELECT display_name FROM users WHERE id=$1 AND tenant_id=$2")
            .bind(user_id)
            .bind(tenant_id)
            .fetch_optional(&s.db_pool)
            .await
            .map_err(db_error)?
            .ok_or_else(|| admin_not_found("Organization administrator not found"))?;

    let email_service = Arc::new(rustshare_core::services::EmailService::new(
        s.db_pool.clone(),
        s.secret_key.clone(),
    ));
    let correlation_id = request_correlation_id(&h);
    let delivery_results = run_bounded(
        request.users.into_iter().enumerate().collect(),
        MAX_BULK_INVITE_DELIVERY_CONCURRENCY,
        |(index, row)| {
            let initial_outcome = rows[index].outcome.clone();
            let email_service = Arc::clone(&email_service);
            let db_pool = s.db_pool.clone();
            let sender_name = sender_name.clone();
            let public_base_url = s.public_base_url.clone();
            let workflow = workflow.clone();
            async move {
                if !matches!(initial_outcome.as_str(), "ready" | "delivery_retry") {
                    return (index, initial_outcome);
                }
                let Ok(_permit) = BULK_INVITE_DELIVERY_SEMAPHORE.acquire().await else {
                    return (index, "apply_failed".to_string());
                };
                let email = row.email.trim().to_ascii_lowercase();
                let mut commit_attempted = false;
                let prepared: Result<Result<(Uuid, String, bool), &'static str>, AppError> = async {
                    let mut tx = db_pool.begin().await.map_err(db_error)?;
                    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended(lower($1), 331))")
                        .bind(&email)
                        .execute(&mut *tx)
                        .await
                        .map_err(db_error)?;
                    let outcome = bulk_invite_validation(&mut tx, tenant_id, &row).await?;
                    if !matches!(outcome, "ready" | "delivery_retry") {
                        tx.commit().await.map_err(db_error)?;
                        return Ok(Err(outcome));
                    }

                    let prepared = if outcome == "ready" {
                        let mut token_bytes = [0u8; 32];
                        rand::rng().fill_bytes(&mut token_bytes);
                        let token = hex::encode(token_bytes);
                        let invite_id = Uuid::new_v4();
                        sqlx::query(
                            "INSERT INTO invite_tokens (id, tenant_id, token, sender_id, recipient_email, recipient_display_name, workflow_id, expires_at, bulk_delivery_status, bulk_delivery_attempted_at) VALUES ($1,$2,$3,$4,$5,$6,$7,now()+interval '7 days','sending',now())",
                        )
                        .bind(invite_id)
                        .bind(tenant_id)
                        .bind(&token)
                        .bind(user_id)
                        .bind(&email)
                        .bind(row.display_name.trim())
                        .bind(workflow.0)
                        .execute(&mut *tx)
                        .await
                        .map_err(db_error)?;
                        let assigned = sqlx::query(
                            "INSERT INTO invite_token_group_assignments (invite_token_id,group_id) SELECT $1,g.id FROM user_groups g WHERE g.tenant_id=$2 AND g.id=ANY($3)",
                        )
                        .bind(invite_id)
                        .bind(tenant_id)
                        .bind(&row.group_ids)
                        .execute(&mut *tx)
                        .await
                        .map_err(db_error)?;
                        if assigned.rows_affected() != row.group_ids.len() as u64 {
                            return Err(admin_conflict("One or more groups are unavailable"));
                        }
                        let expired_invite_count: i64 = sqlx::query_scalar(
                            "WITH expired AS (UPDATE invite_tokens SET revoked_at=now() WHERE tenant_id=$2 AND lower(recipient_email)=lower($1) AND used_at IS NULL AND revoked_at IS NULL AND expires_at<=now() RETURNING id), removed_assignments AS (DELETE FROM invite_token_group_assignments a USING expired e WHERE a.invite_token_id=e.id RETURNING a.invite_token_id) SELECT count(*) FROM expired",
                        )
                        .bind(&email)
                        .bind(tenant_id)
                        .fetch_one(&mut *tx)
                        .await
                        .map_err(db_error)?;
                        if expired_invite_count > 0 {
                            insert_scoped_admin_action(
                                &mut tx,
                                tenant_id,
                                user_id,
                                "organization_admin.expired_invites_replaced",
                                Some("invite"),
                                Some(invite_id),
                                json!({"count": expired_invite_count}),
                                correlation_id,
                            )
                            .await
                            .map_err(db_error)?;
                        }
                        insert_scoped_admin_action(
                            &mut tx,
                            tenant_id,
                            user_id,
                            "organization_admin.user_invited",
                            Some("invite"),
                            Some(invite_id),
                            json!({"row": index + 1}),
                            correlation_id,
                        )
                        .await
                        .map_err(db_error)?;
                        (invite_id, token, true)
                    } else {
                        let existing = sqlx::query_as::<_, (Uuid, String, String)>(
                            "SELECT id,token,recipient_display_name FROM invite_tokens WHERE tenant_id=$1 AND lower(recipient_email)=lower($2) AND used_at IS NULL AND revoked_at IS NULL AND bulk_delivery_status IN ('failed','sending') ORDER BY created_at DESC LIMIT 1 FOR UPDATE",
                        )
                        .bind(tenant_id)
                        .bind(&email)
                        .fetch_optional(&mut *tx)
                        .await
                        .map_err(db_error)?
                        .ok_or_else(|| admin_conflict("Invitation is no longer retryable"))?;
                        if existing.2 != row.display_name.trim() {
                            return Err(admin_conflict("Pending invitation details do not match"));
                        }
                        sqlx::query("UPDATE invite_tokens SET bulk_delivery_status='sending',bulk_delivery_attempted_at=now() WHERE id=$1")
                            .bind(existing.0)
                            .execute(&mut *tx)
                            .await
                            .map_err(db_error)?;
                        (existing.0, existing.1, false)
                    };
                    commit_attempted = true;
                    tx.commit().await.map_err(db_error)?;
                    Ok(Ok(prepared))
                }
                .await;
                let (invite_id, token, is_new) = match prepared {
                    Ok(Ok(prepared)) => prepared,
                    Ok(Err(outcome)) => return (index, outcome.to_string()),
                    Err(_) => {
                        tracing::error!(row = index + 1, "bulk invitation apply transaction failed");
                        return (
                            index,
                            if commit_attempted { "apply_pending" } else { "apply_failed" }
                                .to_string(),
                        );
                    }
                };

                let invite_link = format!(
                    "{}/invite/{token}",
                    public_base_url.trim_end_matches('/')
                );
                let sent = email_service
                    .send_invite_email(
                        &sender_name,
                        row.display_name.trim(),
                        &email,
                        &invite_link,
                        workflow
                            .1
                            .as_deref()
                            .unwrap_or("You've been invited to RustShare"),
                        workflow.2.as_deref().unwrap_or(""),
                    )
                    .await
                    .is_ok();

                let delivery_recorded: Result<(), AppError> = async {
                    let mut tx = db_pool.begin().await.map_err(db_error)?;
                    sqlx::query("UPDATE invite_tokens SET bulk_delivery_status=$2 WHERE id=$1 AND used_at IS NULL AND revoked_at IS NULL")
                        .bind(invite_id)
                        .bind(if sent { "sent" } else { "failed" })
                        .execute(&mut *tx)
                        .await
                        .map_err(db_error)?;
                    insert_scoped_admin_action(
                        &mut tx,
                        tenant_id,
                        user_id,
                        if sent {
                            "organization_admin.invite_delivery_sent"
                        } else {
                            "organization_admin.invite_delivery_failed"
                        },
                        Some("invite"),
                        Some(invite_id),
                        json!({}),
                        correlation_id,
                    )
                    .await
                    .map_err(db_error)?;
                    tx.commit().await.map_err(db_error)?;
                    Ok(())
                }
                .await;
                let outcome = if delivery_recorded.is_err() {
                    tracing::error!(row = index + 1, "bulk invitation delivery status transaction failed");
                    "delivery_pending"
                } else if sent && is_new {
                    "invited"
                } else if sent {
                    "delivery_retried"
                } else {
                    "delivery_failed"
                };
                (index, outcome.to_string())
            }
        },
    )
    .await;

    for (index, outcome) in delivery_results {
        rows[index].outcome = outcome;
    }

    Ok(Json(BulkInviteResponse {
        dry_run: false,
        rows,
    }))
}

async fn run_bounded<T, F, Fut, R>(items: Vec<T>, limit: usize, f: F) -> Vec<R>
where
    F: FnMut(T) -> Fut,
    Fut: std::future::Future<Output = R>,
{
    use futures_util::StreamExt;

    futures_util::stream::iter(items.into_iter().map(f))
        .buffer_unordered(limit)
        .collect()
        .await
}

async fn bulk_invite_validation(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    row: &BulkInviteUser,
) -> Result<&'static str, AppError> {
    let email = row.email.trim().to_ascii_lowercase();
    if row.validate().is_err() || email.len() > 255 {
        return Ok("invalid_email");
    }
    if row.display_name.trim().is_empty()
        || row.display_name.len() > 255
        || row.display_name.chars().any(char::is_control)
    {
        return Ok("invalid_display_name");
    }
    if row.group_ids.len() > 20 {
        return Ok("too_many_groups");
    }
    let mut groups = row.group_ids.clone();
    groups.sort_unstable();
    groups.dedup();
    if groups.len() != row.group_ids.len() {
        return Ok("duplicate_group");
    }
    let group_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM user_groups WHERE tenant_id=$1 AND id=ANY($2)")
            .bind(tenant_id)
            .bind(&row.group_ids)
            .fetch_one(&mut *conn)
            .await
            .map_err(db_error)?;
    if group_count != row.group_ids.len() as i64 {
        return Ok("group_unavailable");
    }

    let (local_user, any_user): (bool, bool) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM users WHERE tenant_id=$1 AND (lower(email)=lower($2) OR lower(username)=lower($2))), EXISTS(SELECT 1 FROM users WHERE lower(email)=lower($2) OR lower(username)=lower($2))",
    )
    .bind(tenant_id)
    .bind(&email)
    .fetch_one(&mut *conn)
    .await
    .map_err(db_error)?;
    if local_user {
        return Ok("already_exists");
    }
    if any_user {
        return Ok("identifier_unavailable");
    }

    let pending: Option<PendingBulkInvite> =
        sqlx::query_as("SELECT tenant_id,id,bulk_delivery_attempted_at,bulk_delivery_status,recipient_display_name FROM invite_tokens WHERE lower(recipient_email)=lower($1) AND used_at IS NULL AND revoked_at IS NULL AND expires_at>now() ORDER BY created_at DESC LIMIT 1")
            .bind(&email)
            .fetch_optional(&mut *conn)
            .await
            .map_err(db_error)?;
    if let Some(pending) = pending {
        if pending.tenant_id != tenant_id {
            return Ok("identifier_unavailable");
        }
        if let Some(display_name) = pending.recipient_display_name {
            let mut assigned: Vec<Uuid> = sqlx::query_scalar(
                "SELECT group_id FROM invite_token_group_assignments WHERE invite_token_id=$1",
            )
            .bind(pending.id)
            .fetch_all(&mut *conn)
            .await
            .map_err(db_error)?;
            let mut requested = row.group_ids.clone();
            assigned.sort_unstable();
            requested.sort_unstable();
            if display_name != row.display_name.trim() || assigned != requested {
                return Ok("pending_invite_mismatch");
            }
        } else if !row.group_ids.is_empty() {
            return Ok("pending_invite_mismatch");
        }
        return Ok(match pending.bulk_delivery_status.as_str() {
            "failed" => "delivery_retry",
            "sending"
                if pending
                    .bulk_delivery_attempted_at
                    .is_some_and(|at| at < chrono::Utc::now() - chrono::Duration::minutes(5)) =>
            {
                "delivery_retry"
            }
            "sending" => "delivery_pending",
            _ => "already_pending",
        });
    }
    Ok("ready")
}

#[cfg(test)]
mod bulk_invite_delivery_tests {
    use super::{
        run_bounded, BULK_INVITE_DELIVERY_SEMAPHORE, MAX_BULK_INVITE_DELIVERY_CONCURRENCY,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn concurrent_batches_keep_bulk_invite_deliveries_globally_bounded() {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let run_batch = |start| {
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            run_bounded(
                (start..start + 100).collect(),
                MAX_BULK_INVITE_DELIVERY_CONCURRENCY,
                move |item| {
                    let active = Arc::clone(&active);
                    let maximum = Arc::clone(&maximum);
                    async move {
                        let _permit = BULK_INVITE_DELIVERY_SEMAPHORE.acquire().await.unwrap();
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        maximum.fetch_max(current, Ordering::SeqCst);
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                        active.fetch_sub(1, Ordering::SeqCst);
                        item
                    }
                },
            )
        };
        let (first, second) = tokio::join!(run_batch(0), run_batch(100));

        assert_eq!(first.len(), 100);
        assert_eq!(second.len(), 100);
        assert!(maximum.load(Ordering::SeqCst) > 1);
        assert!(maximum.load(Ordering::SeqCst) <= MAX_BULK_INVITE_DELIVERY_CONCURRENCY);
    }
}
