//! Tenant-scoped organization group queries.

use crate::services::organization_administrators::insert_scoped_admin_action;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

pub(crate) enum AddMemberOutcome {
    Added,
    GroupNotFound,
    UserNotFound,
    DifferentTenant,
    AlreadyMember,
}

pub(crate) enum RemoveMemberOutcome {
    Removed,
    GroupNotFound,
    UserNotFound,
    DifferentTenant,
    MembershipNotFound,
}

pub(crate) async fn add_member(
    pool: &PgPool,
    group_id: Uuid,
    user_id: Uuid,
    actor_id: Uuid,
    correlation_id: Option<&str>,
) -> Result<AddMemberOutcome, sqlx::Error> {
    let tenant_id: Option<Uuid> =
        sqlx::query_scalar("SELECT tenant_id FROM user_groups WHERE id = $1")
            .bind(group_id)
            .fetch_optional(pool)
            .await?;
    let Some(tenant_id) = tenant_id else {
        return Ok(AddMemberOutcome::GroupNotFound);
    };

    let user_tenant: Option<Uuid> = sqlx::query_scalar("SELECT tenant_id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    let Some(user_tenant) = user_tenant else {
        return Ok(AddMemberOutcome::UserNotFound);
    };
    if user_tenant != tenant_id {
        return Ok(AddMemberOutcome::DifferentTenant);
    }

    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "INSERT INTO group_members (group_id, user_id, added_by)
         VALUES ($1, $2, $3)
         ON CONFLICT (group_id, user_id) DO NOTHING",
    )
    .bind(group_id)
    .bind(user_id)
    .bind(actor_id)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(AddMemberOutcome::AlreadyMember);
    }

    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        actor_id,
        "group.member_added",
        Some("group"),
        Some(group_id),
        json!({"user_id": user_id}),
        correlation_id,
    )
    .await?;
    tx.commit().await?;

    Ok(AddMemberOutcome::Added)
}

pub(crate) async fn remove_member(
    pool: &PgPool,
    group_id: Uuid,
    user_id: Uuid,
    actor_id: Uuid,
    correlation_id: Option<&str>,
) -> Result<RemoveMemberOutcome, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let tenant_id: Option<Uuid> =
        sqlx::query_scalar("SELECT tenant_id FROM user_groups WHERE id = $1 FOR UPDATE")
            .bind(group_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(tenant_id) = tenant_id else {
        return Ok(RemoveMemberOutcome::GroupNotFound);
    };

    let user_tenant: Option<Uuid> =
        sqlx::query_scalar("SELECT tenant_id FROM users WHERE id = $1 FOR UPDATE")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(user_tenant) = user_tenant else {
        return Ok(RemoveMemberOutcome::UserNotFound);
    };
    if user_tenant != tenant_id {
        return Ok(RemoveMemberOutcome::DifferentTenant);
    }

    let result = sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
        .bind(group_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        return Ok(RemoveMemberOutcome::MembershipNotFound);
    }

    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        actor_id,
        "group.member_removed",
        Some("group"),
        Some(group_id),
        json!({"user_id": user_id}),
        correlation_id,
    )
    .await?;
    tx.commit().await?;

    Ok(RemoveMemberOutcome::Removed)
}

#[derive(sqlx::FromRow)]
pub(crate) struct OrganizationGroupSummary {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) updated_at: chrono::DateTime<chrono::Utc>,
    pub(crate) archived_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) member_count: i64,
}

pub(crate) async fn list_groups(
    pool: &PgPool,
    tenant_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(i64, Vec<OrganizationGroupSummary>), sqlx::Error> {
    let total = sqlx::query_scalar("SELECT COUNT(*) FROM user_groups WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;
    let groups = if offset >= total {
        Vec::new()
    } else {
        sqlx::query_as::<_, OrganizationGroupSummary>(
            "SELECT g.id, g.name, g.description, g.created_at, g.updated_at, g.archived_at,
                    count(u.id) AS member_count
             FROM user_groups g
             LEFT JOIN group_members m ON m.group_id = g.id
             LEFT JOIN users u ON u.id = m.user_id AND u.tenant_id = g.tenant_id
             WHERE g.tenant_id = $1
             GROUP BY g.id
             ORDER BY g.created_at DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?
    };
    Ok((total, groups))
}
