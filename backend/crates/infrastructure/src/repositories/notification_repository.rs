use chrono::Utc;
use rustshare_core::domain::{Notification, NotificationId, UserId};
use rustshare_core::services::{CreateNotification, NotificationError};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Repository for notification database operations.
pub struct NotificationRepository {
    pool: PgPool,
}

impl NotificationRepository {
    fn map_notification_row(row: sqlx::postgres::PgRow) -> Result<Notification, NotificationError> {
        let notification_type = row
            .try_get::<String, _>("notification_type")
            .map_err(|e| NotificationError::Database(e.to_string()))?
            .parse()
            .map_err(|err: String| NotificationError::Database(err))?;
        let resource_type = row
            .try_get::<String, _>("resource_type")
            .map_err(|e| NotificationError::Database(e.to_string()))?
            .parse()
            .map_err(|err: String| NotificationError::Database(err))?;

        Ok(Notification {
            id: row.try_get("id").map_err(|e| NotificationError::Database(e.to_string()))?,
            user_id: row.try_get("user_id").map_err(|e| NotificationError::Database(e.to_string()))?,
            notification_type,
            title: row.try_get("title").map_err(|e| NotificationError::Database(e.to_string()))?,
            message: row.try_get("message").map_err(|e| NotificationError::Database(e.to_string()))?,
            resource_id: row.try_get("resource_id").map_err(|e| NotificationError::Database(e.to_string()))?,
            resource_type,
            action_url: row.try_get("action_url").map_err(|e| NotificationError::Database(e.to_string()))?,
            read: row.try_get("read").map_err(|e| NotificationError::Database(e.to_string()))?,
            created_at: row.try_get("created_at").map_err(|e| NotificationError::Database(e.to_string()))?,
            tenant_id: row.try_get("tenant_id").map_err(|e| NotificationError::Database(e.to_string()))?,
        })
    }

    /// Create a new NotificationRepository with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a new notification.
    pub async fn create(&self, request: CreateNotification) -> Result<Notification, NotificationError> {
        let id = Uuid::new_v4();
        let created_at = Utc::now();

        let row = sqlx::query(
            r#"
            INSERT INTO notifications (
                id, user_id, notification_type, title, message,
                resource_id, resource_type, action_url, read, created_at, tenant_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, FALSE, $9, '00000000-0000-0000-0000-000000000000')
            RETURNING id, user_id, notification_type, title, message,
                      resource_id, resource_type, action_url, read, created_at, tenant_id
            "#,
        )
        .bind(id)
        .bind(request.user_id)
        .bind(request.notification_type.to_string())
        .bind(request.title)
        .bind(request.message)
        .bind(request.resource_id)
        .bind(request.resource_type.to_string())
        .bind(request.action_url)
        .bind(created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| NotificationError::Database(e.to_string()))?;

        Self::map_notification_row(row)
    }

    /// Get a notification by ID.
    pub async fn find_by_id(
        &self,
        notification_id: NotificationId,
    ) -> Result<Option<Notification>, NotificationError> {
        let row = sqlx::query(
            r#"
            SELECT id, user_id, notification_type, title, message,
                   resource_id, resource_type, action_url, read, created_at, tenant_id
            FROM notifications
            WHERE id = $1
            "#,
        )
        .bind(notification_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| NotificationError::Database(e.to_string()))?;

        row.map(Self::map_notification_row).transpose()
    }

    /// List notifications for a user (paginated, optional unread filter).
    pub async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, NotificationError> {
        let rows = if unread_only {
            sqlx::query(
                r#"
                SELECT id, user_id, notification_type, title, message,
                       resource_id, resource_type, action_url, read, created_at, tenant_id
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
            .await
            .map_err(|e| NotificationError::Database(e.to_string()))?
        } else {
            sqlx::query(
                r#"
                SELECT id, user_id, notification_type, title, message,
                       resource_id, resource_type, action_url, read, created_at, tenant_id
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
            .await
            .map_err(|e| NotificationError::Database(e.to_string()))?
        };

        rows.into_iter().map(Self::map_notification_row).collect()
    }

    /// Count unread notifications for a user.
    pub async fn count_unread(&self, user_id: UserId) -> Result<i64, NotificationError> {
        let result: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM notifications
            WHERE user_id = $1 AND read = FALSE
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| NotificationError::Database(e.to_string()))?;

        Ok(result.0)
    }

    /// Count notifications for a user with optional unread filtering.
    pub async fn count_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
    ) -> Result<i64, NotificationError> {
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
            .await
            .map_err(|e| NotificationError::Database(e.to_string()))?
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
            .await
            .map_err(|e| NotificationError::Database(e.to_string()))?
        };

        Ok(result.0)
    }

    /// Mark a notification as read.
    pub async fn mark_as_read(
        &self,
        notification_id: NotificationId,
    ) -> Result<Notification, NotificationError> {
        let row = sqlx::query(
            r#"
            UPDATE notifications
            SET read = TRUE
            WHERE id = $1
            RETURNING id, user_id, notification_type, title, message,
                      resource_id, resource_type, action_url, read, created_at, tenant_id
            "#,
        )
        .bind(notification_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| NotificationError::Database(e.to_string()))?;

        Self::map_notification_row(row)
    }

    /// Delete a notification.
    pub async fn delete(&self, notification_id: NotificationId) -> Result<(), NotificationError> {
        sqlx::query(
            r#"
            DELETE FROM notifications
            WHERE id = $1
            "#,
        )
        .bind(notification_id)
        .execute(&self.pool)
        .await
        .map_err(|e| NotificationError::Database(e.to_string()))?;

        Ok(())
    }
}
