use chrono::Utc;
use rustshare_core::domain::{FileId, FolderId, Share, ShareId, SharePermissions, UserId};
use sqlx::PgPool;
use uuid::Uuid;

/// Repository for share database operations.
pub struct ShareRepository {
    pool: PgPool,
}

impl ShareRepository {
    /// Create a new ShareRepository with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find a user share by resource and recipient.
    pub async fn find_user_share(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        recipient_user_id: UserId,
    ) -> Result<Option<Share>, sqlx::Error> {
        let result = sqlx::query_as::<_, Share>(
            r#"
            SELECT id, file_id, folder_id, share_token, permissions, password_hash,
                   expires_at, access_count, recipient_user_id, created_by,
                   created_at, revoked_at
            FROM shares
            WHERE recipient_user_id = $1
              AND file_id IS NOT DISTINCT FROM $2
              AND folder_id IS NOT DISTINCT FROM $3
              AND revoked_at IS NULL
            LIMIT 1
            "#,
        )
        .bind(recipient_user_id)
        .bind(file_id)
        .bind(folder_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    /// List all shares received by a user (paginated).
    pub async fn list_received_shares(
        &self,
        user_id: UserId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Share>, sqlx::Error> {
        let shares = sqlx::query_as::<_, Share>(
            r#"
            SELECT id, file_id, folder_id, share_token, permissions, password_hash,
                   expires_at, access_count, recipient_user_id, created_by,
                   created_at, revoked_at
            FROM shares
            WHERE recipient_user_id = $1
              AND revoked_at IS NULL
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(shares)
    }

    /// List all recipients of a resource (for multi-user shares on same resource).
    pub async fn list_share_recipients(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
    ) -> Result<Vec<Share>, sqlx::Error> {
        let shares = sqlx::query_as::<_, Share>(
            r#"
            SELECT id, file_id, folder_id, share_token, permissions, password_hash,
                   expires_at, access_count, recipient_user_id, created_by,
                   created_at, revoked_at
            FROM shares
            WHERE recipient_user_id IS NOT NULL
              AND file_id IS NOT DISTINCT FROM $1
              AND folder_id IS NOT DISTINCT FROM $2
              AND revoked_at IS NULL
            ORDER BY created_at ASC
            "#,
        )
        .bind(file_id)
        .bind(folder_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(shares)
    }

    /// Create a new user share.
    pub async fn create_user_share(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        recipient_user_id: UserId,
        permissions: SharePermissions,
        created_by: UserId,
    ) -> Result<Share, sqlx::Error> {
        let id = Uuid::new_v4();
        let created_at = Utc::now();

        sqlx::query_as::<_, Share>(
            r#"
            INSERT INTO shares (
                id, file_id, folder_id, share_token, permissions,
                password_hash, expires_at, access_count,
                recipient_user_id, created_by, created_at, revoked_at
            )
            VALUES ($1, $2, $3, NULL, $4, NULL, NULL, 0, $5, $6, $7, NULL)
            RETURNING id, file_id, folder_id, share_token, permissions, password_hash,
                      expires_at, access_count, recipient_user_id, created_by,
                      created_at, revoked_at
            "#,
        )
        .bind(id)
        .bind(file_id)
        .bind(folder_id)
        .bind(permissions)
        .bind(recipient_user_id)
        .bind(created_by)
        .bind(created_at)
        .fetch_one(&self.pool)
        .await
    }

    /// Update recipient permission on a user share.
    pub async fn update_share_permission(
        &self,
        share_id: ShareId,
        new_permission: SharePermissions,
    ) -> Result<Share, sqlx::Error> {
        sqlx::query_as::<_, Share>(
            r#"
            UPDATE shares
            SET permissions = $2
            WHERE id = $1
            RETURNING id, file_id, folder_id, share_token, permissions, password_hash,
                      expires_at, access_count, recipient_user_id, created_by,
                      created_at, revoked_at
            "#,
        )
        .bind(share_id)
        .bind(new_permission)
        .fetch_one(&self.pool)
        .await
    }

    /// Delete (revoke) a share by setting revoked_at timestamp.
    /// This is a soft delete operation.
    pub async fn delete_share(&self, share_id: ShareId) -> Result<(), sqlx::Error> {
        let revoked_at = Utc::now();

        sqlx::query(
            r#"
            UPDATE shares
            SET revoked_at = $2
            WHERE id = $1
            "#,
        )
        .bind(share_id)
        .bind(revoked_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find a share by its ID.
    pub async fn find_share_by_id(&self, share_id: ShareId) -> Result<Option<Share>, sqlx::Error> {
        let result = sqlx::query_as::<_, Share>(
            r#"
            SELECT id, file_id, folder_id, share_token, permissions, password_hash,
                   expires_at, access_count, recipient_user_id, created_by,
                   created_at, revoked_at
            FROM shares
            WHERE id = $1
            "#,
        )
        .bind(share_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }
}
