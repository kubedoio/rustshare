use chrono::Utc;
use rustshare_core::domain::{
    Notification, NotificationId, NotificationType, ResourceType, UserId,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Repository for notification database operations.
pub struct NotificationRepository {
    pool: PgPool,
}

impl NotificationRepository {
    fn map_notification_row(row: sqlx::postgres::PgRow) -> Result<Notification, sqlx::Error> {
        let notification_type = row
            .try_get::<String, _>("notification_type")?
            .parse()
            .map_err(|err: String| sqlx::Error::Decode(err.into()))?;
        let resource_type = row
            .try_get::<String, _>("resource_type")?
            .parse()
            .map_err(|err: String| sqlx::Error::Decode(err.into()))?;

        Ok(Notification {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            notification_type,
            title: row.try_get("title")?,
            message: row.try_get("message")?,
            resource_id: row.try_get("resource_id")?,
            resource_type,
            action_url: row.try_get("action_url")?,
            read: row.try_get("read")?,
            created_at: row.try_get("created_at")?,
        })
    }

    /// Create a new NotificationRepository with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a new notification.
    pub async fn create(
        &self,
        user_id: UserId,
        notification_type: NotificationType,
        title: String,
        message: String,
        resource_id: Uuid,
        resource_type: ResourceType,
        action_url: Option<String>,
    ) -> Result<Notification, sqlx::Error> {
        let id = Uuid::new_v4();
        let created_at = Utc::now();

        let row = sqlx::query(
            r#"
            INSERT INTO notifications (
                id, user_id, notification_type, title, message,
                resource_id, resource_type, action_url, read, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, FALSE, $9)
            RETURNING id, user_id, notification_type, title, message,
                      resource_id, resource_type, action_url, read, created_at
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(notification_type.to_string())
        .bind(title)
        .bind(message)
        .bind(resource_id)
        .bind(resource_type.to_string())
        .bind(action_url)
        .bind(created_at)
        .fetch_one(&self.pool)
        .await?;

        Self::map_notification_row(row)
    }

    /// Get a notification by ID.
    pub async fn find_by_id(
        &self,
        notification_id: NotificationId,
    ) -> Result<Option<Notification>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, user_id, notification_type, title, message,
                   resource_id, resource_type, action_url, read, created_at
            FROM notifications
            WHERE id = $1
            "#,
        )
        .bind(notification_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::map_notification_row).transpose()
    }

    /// List notifications for a user (paginated, optional unread filter).
    pub async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, sqlx::Error> {
        let rows = if unread_only {
            sqlx::query(
                r#"
                SELECT id, user_id, notification_type, title, message,
                       resource_id, resource_type, action_url, read, created_at
                FROM notifications
                WHERE user_id = $1 AND read = FALSE
                ORDER BY created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(user_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, user_id, notification_type, title, message,
                       resource_id, resource_type, action_url, read, created_at
                FROM notifications
                WHERE user_id = $1
                ORDER BY created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(user_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        };

        rows.into_iter().map(Self::map_notification_row).collect()
    }

    /// Count unread notifications for a user.
    pub async fn count_unread(&self, user_id: UserId) -> Result<i64, sqlx::Error> {
        let result: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM notifications
            WHERE user_id = $1 AND read = FALSE
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(result.0)
    }

    /// Count notifications for a user with optional unread filtering.
    pub async fn count_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
    ) -> Result<i64, sqlx::Error> {
        let result: (i64,) = if unread_only {
            sqlx::query_as(
                r#"
                SELECT COUNT(*)
                FROM notifications
                WHERE user_id = $1 AND read = FALSE
                "#,
            )
            .bind(user_id)
            .fetch_one(&self.pool)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT COUNT(*)
                FROM notifications
                WHERE user_id = $1
                "#,
            )
            .bind(user_id)
            .fetch_one(&self.pool)
            .await?
        };

        Ok(result.0)
    }

    /// Mark a notification as read.
    pub async fn mark_as_read(
        &self,
        notification_id: NotificationId,
    ) -> Result<Notification, sqlx::Error> {
        let row = sqlx::query(
            r#"
            UPDATE notifications
            SET read = TRUE
            WHERE id = $1
            RETURNING id, user_id, notification_type, title, message,
                      resource_id, resource_type, action_url, read, created_at
            "#,
        )
        .bind(notification_id)
        .fetch_one(&self.pool)
        .await?;

        Self::map_notification_row(row)
    }

    /// Delete a notification.
    pub async fn delete(&self, notification_id: NotificationId) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM notifications
            WHERE id = $1
            "#,
        )
        .bind(notification_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
