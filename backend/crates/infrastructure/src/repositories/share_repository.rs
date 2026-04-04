use chrono::Utc;
use rustshare_core::domain::{FileId, FolderId, Share, ShareId, SharePermissions, UserId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Repository for share database operations.
pub struct ShareRepository {
    /// The database pool (public for use by other repositories).
    pub pool: PgPool,
}

impl ShareRepository {
    fn permission_to_db_value(permission: SharePermissions) -> &'static str {
        match permission {
            SharePermissions::View => "View",
            SharePermissions::Edit => "Edit",
            SharePermissions::Admin => "Admin",
        }
    }

    fn permission_from_db_value(value: &str) -> SharePermissions {
        match value {
            "Edit" | "edit" => SharePermissions::Edit,
            "Admin" | "admin" => SharePermissions::Admin,
            _ => SharePermissions::View,
        }
    }

    fn map_share_row(row: sqlx::postgres::PgRow) -> Result<Share, sqlx::Error> {
        let permissions = Self::permission_from_db_value(&row.try_get::<String, _>("permissions")?);

        Ok(Share {
            id: row.try_get("id")?,
            file_id: row.try_get("file_id")?,
            folder_id: row.try_get("folder_id")?,
            share_token: row.try_get("share_token")?,
            permissions,
            password_hash: row.try_get("password_hash")?,
            expires_at: row.try_get("expires_at")?,
            upload_only: row.try_get("upload_only")?,
            access_count: row.try_get("access_count")?,
            recipient_user_id: row.try_get("recipient_user_id")?,
            recipient_group_id: row.try_get("recipient_group_id")?,
            created_by: row.try_get("created_by")?,
            created_at: row.try_get("created_at")?,
            revoked_at: row.try_get("revoked_at")?,
            tenant_id: row.try_get("tenant_id")?,
        })
    }

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
        let result = sqlx::query(
            r#"
            SELECT id, file_id, folder_id, share_token, permissions, password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                   created_at, revoked_at, tenant_id
            FROM shares
            WHERE recipient_user_id = $1
              AND file_id IS NOT DISTINCT FROM $2::uuid
              AND folder_id IS NOT DISTINCT FROM $3::uuid
              AND revoked_at IS NULL
            LIMIT 1
            "#,
        )
        .bind(recipient_user_id)
        .bind(file_id)
        .bind(folder_id)
        .fetch_optional(&self.pool)
        .await?;

        result.map(Self::map_share_row).transpose()
    }

    /// List all shares received by a user (paginated).
    pub async fn list_received_shares(
        &self,
        user_id: UserId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Share>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, file_id, folder_id, share_token, permissions, password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                   created_at, revoked_at, tenant_id
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

        rows.into_iter().map(Self::map_share_row).collect()
    }

    /// List all recipients of a resource (for multi-user shares on same resource).
    pub async fn list_share_recipients(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
    ) -> Result<Vec<Share>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, file_id, folder_id, share_token, permissions, password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                   created_at, revoked_at, tenant_id
            FROM shares
            WHERE recipient_user_id IS NOT NULL
              AND file_id IS NOT DISTINCT FROM $1::uuid
              AND folder_id IS NOT DISTINCT FROM $2::uuid
              AND revoked_at IS NULL
            ORDER BY created_at ASC
            "#,
        )
        .bind(file_id)
        .bind(folder_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(Self::map_share_row).collect()
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

        let row = sqlx::query(
            r#"
            INSERT INTO shares (
                id, file_id, folder_id, share_token, permissions,
                password_hash, expires_at, upload_only, access_count,
                recipient_user_id, recipient_group_id, created_by, created_at, revoked_at, tenant_id
            )
            VALUES ($1, $2, $3, NULL, $4, NULL, NULL, FALSE, 0, $5, NULL, $6, $7, NULL, '00000000-0000-0000-0000-000000000000')
            RETURNING id, file_id, folder_id, share_token, permissions, password_hash,
                      expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                      created_at, revoked_at, tenant_id
            "#,
        )
        .bind(id)
        .bind(file_id)
        .bind(folder_id)
        .bind(Self::permission_to_db_value(permissions))
        .bind(recipient_user_id)
        .bind(created_by)
        .bind(created_at)
        .fetch_one(&self.pool)
        .await?;

        Self::map_share_row(row)
    }

    /// Update recipient permission on a user share.
    pub async fn update_share_permission(
        &self,
        share_id: ShareId,
        new_permission: SharePermissions,
    ) -> Result<Share, sqlx::Error> {
        let row = sqlx::query(
            r#"
            UPDATE shares
            SET permissions = $2
            WHERE id = $1
            RETURNING id, file_id, folder_id, share_token, permissions, password_hash,
                      expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                      created_at, revoked_at, tenant_id
            "#,
        )
        .bind(share_id)
        .bind(Self::permission_to_db_value(new_permission))
        .fetch_one(&self.pool)
        .await?;

        Self::map_share_row(row)
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
        let result = sqlx::query(
            r#"
            SELECT id, file_id, folder_id, share_token, permissions, password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                   created_at, revoked_at, tenant_id
            FROM shares
            WHERE id = $1
            "#,
        )
        .bind(share_id)
        .fetch_optional(&self.pool)
        .await?;

        result.map(Self::map_share_row).transpose()
    }

    /// Get a share by ID (alias for find_share_by_id).
    pub async fn get_by_id(&self, share_id: ShareId) -> Result<Option<Share>, sqlx::Error> {
        self.find_share_by_id(share_id).await
    }

    /// Revoke a share (alias for delete_share).
    pub async fn revoke_share(&self, share_id: ShareId) -> Result<(), sqlx::Error> {
        self.delete_share(share_id).await
    }
}

// Implement ShareOps trait for ShareRepository
impl rustshare_core::services::ShareOps for ShareRepository {
    async fn find_user_share(
        &self,
        file_id: Option<rustshare_core::domain::FileId>,
        folder_id: Option<rustshare_core::domain::FolderId>,
        recipient_user_id: rustshare_core::domain::UserId,
    ) -> Result<Option<rustshare_core::domain::Share>, sqlx::Error> {
        self.find_user_share(file_id, folder_id, recipient_user_id)
            .await
    }

    async fn create_user_share(
        &self,
        file_id: Option<rustshare_core::domain::FileId>,
        folder_id: Option<rustshare_core::domain::FolderId>,
        recipient_user_id: rustshare_core::domain::UserId,
        permissions: rustshare_core::domain::SharePermissions,
        created_by: rustshare_core::domain::UserId,
    ) -> Result<rustshare_core::domain::Share, sqlx::Error> {
        self.create_user_share(
            file_id,
            folder_id,
            recipient_user_id,
            permissions,
            created_by,
        )
        .await
    }

    async fn update_share_permission(
        &self,
        share_id: rustshare_core::domain::ShareId,
        new_permission: rustshare_core::domain::SharePermissions,
    ) -> Result<rustshare_core::domain::Share, sqlx::Error> {
        self.update_share_permission(share_id, new_permission).await
    }

    async fn get_by_id(
        &self,
        share_id: rustshare_core::domain::ShareId,
    ) -> Result<Option<rustshare_core::domain::Share>, sqlx::Error> {
        self.get_by_id(share_id).await
    }

    async fn list_received_shares(
        &self,
        user_id: rustshare_core::domain::UserId,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<rustshare_core::domain::Share>, sqlx::Error> {
        self.list_received_shares(user_id, limit, offset).await
    }

    async fn list_share_recipients(
        &self,
        file_id: Option<rustshare_core::domain::FileId>,
        folder_id: Option<rustshare_core::domain::FolderId>,
    ) -> Result<Vec<rustshare_core::domain::Share>, sqlx::Error> {
        self.list_share_recipients(file_id, folder_id).await
    }

    async fn revoke_share(
        &self,
        share_id: rustshare_core::domain::ShareId,
    ) -> Result<(), sqlx::Error> {
        self.revoke_share(share_id).await
    }
}
