use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, FromRow)]
pub struct DeviceRecord {
    pub id: Uuid,
    pub device_name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Tenant- and owner-scoped persistence for user device tokens.
#[derive(Clone)]
pub struct DeviceRepository {
    pool: PgPool,
}

impl DeviceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<Vec<DeviceRecord>> {
        Ok(sqlx::query_as(
            "SELECT d.id, d.device_name, d.created_at, d.last_used_at
             FROM device_tokens d
             JOIN users u ON u.id = d.user_id
             WHERE u.tenant_id = $1 AND d.user_id = $2 AND d.revoked_at IS NULL
             ORDER BY d.created_at DESC",
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn revoke_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        device_id: Uuid,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE device_tokens d
             SET revoked_at = NOW()
             WHERE d.id = $1 AND d.user_id = $2 AND d.revoked_at IS NULL
               AND EXISTS (
                   SELECT 1 FROM users u WHERE u.id = d.user_id AND u.tenant_id = $3
               )",
        )
        .bind(device_id)
        .bind(user_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}
