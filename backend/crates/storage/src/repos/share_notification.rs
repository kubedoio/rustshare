//! Repository for share access notification tracking

use async_trait::async_trait;
use rustshare_core::domain::UserId;
use sqlx::PgPool;
use uuid::Uuid;

pub struct ShareNotificationRepoImpl {
    pool: PgPool,
}

impl ShareNotificationRepoImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl rustshare_core::services::ShareNotificationRepo for ShareNotificationRepoImpl {
    async fn was_notified(&self, user_id: UserId, share_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM share_access_notifications 
                WHERE user_id = $1 AND share_id = $2
            )
            "#,
        )
        .bind(user_id)
        .bind(share_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    async fn record_notification(
        &self,
        user_id: UserId,
        share_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO share_access_notifications (user_id, share_id, notified_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (user_id, share_id) DO NOTHING
            "#,
            user_id,
            share_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
