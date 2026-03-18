use chrono::Utc;
use rustshare_core::domain::{Notification, NotificationId, NotificationType, ResourceType, UserId};
use sqlx::PgPool;
use uuid::Uuid;

/// Repository for notification database operations.
pub struct NotificationRepository {
    pool: PgPool,
}

impl NotificationRepository {
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

        sqlx::query_as::<_, Notification>(
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
        .bind(notification_type)
        .bind(title)
        .bind(message)
        .bind(resource_id)
        .bind(resource_type)
        .bind(action_url)
        .bind(created_at)
        .fetch_one(&self.pool)
        .await
    }

    /// Get a notification by ID.
    pub async fn find_by_id(
        &self,
        notification_id: NotificationId,
    ) -> Result<Option<Notification>, sqlx::Error> {
        let result = sqlx::query_as::<_, Notification>(
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

        Ok(result)
    }

    /// List notifications for a user (paginated, optional unread filter).
    pub async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, sqlx::Error> {
        let notifications = if unread_only {
            sqlx::query_as::<_, Notification>(
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
            sqlx::query_as::<_, Notification>(
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

        Ok(notifications)
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

    /// Mark a notification as read.
    pub async fn mark_as_read(
        &self,
        notification_id: NotificationId,
    ) -> Result<Notification, sqlx::Error> {
        sqlx::query_as::<_, Notification>(
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
        .await
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
