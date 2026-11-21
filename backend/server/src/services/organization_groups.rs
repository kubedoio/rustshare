//! Tenant-scoped organization group queries.

use sqlx::PgPool;
use uuid::Uuid;

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
