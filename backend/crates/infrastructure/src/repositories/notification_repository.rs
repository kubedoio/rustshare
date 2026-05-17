use chrono::Utc;
use rustshare_core::domain::{Notification, NotificationId, UserId};
use rustshare_core::services::{CreateNotification, NotificationError};
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
        request: CreateNotification,
    ) -> Result<Notification, NotificationError> {
        let id = Uuid::new_v4();
        let created_at = Utc::now();

        sqlx::query_as!(
            Notification,
            r#"
            INSERT INTO notifications (
                id, user_id, notification_type, title, message,
                resource_id, resource_type, action_url, read, created_at, tenant_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, FALSE, $9, '00000000-0000-0000-0000-000000000000')
            RETURNING id, user_id,
                      notification_type as "notification_type: _",
                      title, message,
                      resource_id,
                      resource_type as "resource_type: _",
                      action_url, read, created_at, tenant_id
            "#,
            id,
            request.user_id,
            request.notification_type.to_string(),
            request.title,
            request.message,
            request.resource_id,
            request.resource_type.to_string(),
            request.action_url,
            created_at
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| NotificationError::Database(e.to_string()))
    }

    /// Get a notification by ID.
    pub async fn find_by_id(
        &self,
        notification_id: NotificationId,
    ) -> Result<Option<Notification>, NotificationError> {
        sqlx::query_as!(
            Notification,
            r#"
            SELECT id, user_id,
                   notification_type as "notification_type: _",
                   title, message,
                   resource_id,
                   resource_type as "resource_type: _",
                   action_url, read, created_at, tenant_id
            FROM notifications
            WHERE id = $1
            "#,
            notification_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| NotificationError::Database(e.to_string()))
    }

    /// List notifications for a user (paginated, optional unread filter).
    pub async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, NotificationError> {
        if unread_only {
            sqlx::query_as!(
                Notification,
                r#"
                SELECT id, user_id,
                       notification_type as "notification_type: _",
                       title, message,
                       resource_id,
                       resource_type as "resource_type: _",
                       action_url, read, created_at, tenant_id
                FROM notifications
                WHERE user_id = $1 AND read = FALSE
                ORDER BY created_at DESC
                LIMIT $2 OFFSET $3
                "#,
                user_id,
                limit,
                offset
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| NotificationError::Database(e.to_string()))
        } else {
            sqlx::query_as!(
                Notification,
                r#"
                SELECT id, user_id,
                       notification_type as "notification_type: _",
                       title, message,
                       resource_id,
                       resource_type as "resource_type: _",
                       action_url, read, created_at, tenant_id
                FROM notifications
                WHERE user_id = $1
                ORDER BY created_at DESC
                LIMIT $2 OFFSET $3
                "#,
                user_id,
                limit,
                offset
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| NotificationError::Database(e.to_string()))
        }
    }

    /// Count unread notifications for a user.
    pub async fn count_unread(&self, user_id: UserId) -> Result<i64, NotificationError> {
        let row = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM notifications
            WHERE user_id = $1 AND read = FALSE
            "#,
            user_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| NotificationError::Database(e.to_string()))?;

        Ok(row.count.unwrap_or(0))
    }

    /// Count notifications for a user with optional unread filtering.
    pub async fn count_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
    ) -> Result<i64, NotificationError> {
        if unread_only {
            let row = sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM notifications
                WHERE user_id = $1 AND read = FALSE
                "#,
                user_id
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|e| NotificationError::Database(e.to_string()))?;

            Ok(row.count.unwrap_or(0))
        } else {
            let row = sqlx::query!(
                r#"
                SELECT COUNT(*) as count
                FROM notifications
                WHERE user_id = $1
                "#,
                user_id
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|e| NotificationError::Database(e.to_string()))?;

            Ok(row.count.unwrap_or(0))
        }
    }

    /// Mark a notification as read.
    pub async fn mark_as_read(
        &self,
        notification_id: NotificationId,
    ) -> Result<Notification, NotificationError> {
        sqlx::query_as!(
            Notification,
            r#"
            UPDATE notifications
            SET read = TRUE
            WHERE id = $1
            RETURNING id, user_id,
                      notification_type as "notification_type: _",
                      title, message,
                      resource_id,
                      resource_type as "resource_type: _",
                      action_url, read, created_at, tenant_id
            "#,
            notification_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| NotificationError::Database(e.to_string()))
    }

    /// Delete a notification.
    pub async fn delete(&self, notification_id: NotificationId) -> Result<(), NotificationError> {
        sqlx::query!(
            r#"
            DELETE FROM notifications
            WHERE id = $1
            "#,
            notification_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| NotificationError::Database(e.to_string()))?;

        Ok(())
    }
}
