use chrono::Utc;
use rustshare_core::domain::{FileId, FolderId, Share, ShareId, SharePermissions, UserId};
use sqlx::PgPool;
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

    /// Create a new ShareRepository with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find a user share by resource and recipient, scoped to a tenant.
    pub async fn find_user_share(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        recipient_user_id: UserId,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<Share>> {
        let share = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                   created_at, revoked_at, tenant_id
            FROM shares
            WHERE recipient_user_id = $1
              AND tenant_id = $2
              AND file_id IS NOT DISTINCT FROM $3::uuid
              AND folder_id IS NOT DISTINCT FROM $4::uuid
              AND revoked_at IS NULL
            LIMIT 1
            "#,
            recipient_user_id,
            tenant_id,
            file_id,
            folder_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(share)
    }

    /// List all shares received by a user (paginated), scoped to a tenant.
    pub async fn list_received_shares(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                   created_at, revoked_at, tenant_id
            FROM shares
            WHERE tenant_id = $2
              AND (recipient_user_id = $1 OR recipient_group_id IN (
                  SELECT gm.group_id
                  FROM group_members gm
                  JOIN users u ON u.id = gm.user_id
                  WHERE gm.user_id = $1 AND u.tenant_id = $2
              ))
              AND revoked_at IS NULL
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
            user_id,
            tenant_id,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(shares)
    }

    /// List all recipients of a resource (for multi-user shares on same resource), scoped to a tenant.
    pub async fn list_share_recipients(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        tenant_id: Uuid,
    ) -> anyhow::Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                   created_at, revoked_at, tenant_id
            FROM shares
            WHERE tenant_id = $3
              AND recipient_user_id IS NOT NULL
              AND file_id IS NOT DISTINCT FROM $1::uuid
              AND folder_id IS NOT DISTINCT FROM $2::uuid
              AND revoked_at IS NULL
            ORDER BY created_at ASC
            "#,
            file_id,
            folder_id,
            tenant_id
        )
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
        tenant_id: Uuid,
    ) -> anyhow::Result<Share> {
        let id = Uuid::new_v4();
        let created_at = Utc::now();
        let permissions_str = Self::permission_to_db_value(permissions);

        let share = sqlx::query_as!(
            Share,
            r#"
            INSERT INTO shares (
                id, file_id, folder_id, share_token, permissions,
                password_hash, expires_at, upload_only, access_count,
                recipient_user_id, recipient_group_id, created_by, created_at, revoked_at, tenant_id
            )
            VALUES ($1, $2, $3, NULL, $4, NULL, NULL, FALSE, 0, $5, NULL, $6, $7, NULL, $8)
            RETURNING id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                      expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                      created_at, revoked_at, tenant_id
            "#,
            id,
            file_id,
            folder_id,
            permissions_str,
            recipient_user_id,
            created_by,
            created_at,
            tenant_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(share)
    }

    /// Update recipient permission on a user share, scoped to a tenant.
    pub async fn update_share_permission(
        &self,
        share_id: ShareId,
        tenant_id: Uuid,
        new_permission: SharePermissions,
    ) -> anyhow::Result<Share> {
        let permissions_str = Self::permission_to_db_value(new_permission);

        let share = sqlx::query_as!(
            Share,
            r#"
            UPDATE shares
            SET permissions = $3
            WHERE id = $1 AND tenant_id = $2
            RETURNING id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                      expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                      created_at, revoked_at, tenant_id
            "#,
            share_id,
            tenant_id,
            permissions_str
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(share)
    }

    /// Delete (revoke) a share by setting revoked_at timestamp, scoped to a tenant.
    /// This is a soft delete operation.
    pub async fn delete_share(&self, share_id: ShareId, tenant_id: Uuid) -> anyhow::Result<()> {
        let revoked_at = Utc::now();

        sqlx::query!(
            r#"
            UPDATE shares
            SET revoked_at = $3
            WHERE id = $1 AND tenant_id = $2
            "#,
            share_id,
            tenant_id,
            revoked_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find a share by its ID, scoped to a tenant.
    pub async fn find_share_by_id(
        &self,
        share_id: ShareId,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<Share>> {
        let share = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id, created_by,
                   created_at, revoked_at, tenant_id
            FROM shares
            WHERE id = $1 AND tenant_id = $2
            "#,
            share_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(share)
    }

    /// Get a share by ID (alias for find_share_by_id).
    pub async fn get_by_id(
        &self,
        share_id: ShareId,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<Share>> {
        self.find_share_by_id(share_id, tenant_id).await
    }

    /// Revoke a share (alias for delete_share).
    pub async fn revoke_share(&self, share_id: ShareId, tenant_id: Uuid) -> anyhow::Result<()> {
        self.delete_share(share_id, tenant_id).await
    }
}

// Implement ShareOps trait for ShareRepository
impl rustshare_core::services::ShareOps for ShareRepository {
    async fn find_user_share(
        &self,
        file_id: Option<rustshare_core::domain::FileId>,
        folder_id: Option<rustshare_core::domain::FolderId>,
        recipient_user_id: rustshare_core::domain::UserId,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Share>> {
        self.find_user_share(file_id, folder_id, recipient_user_id, tenant_id)
            .await
    }

    async fn create_user_share(
        &self,
        file_id: Option<rustshare_core::domain::FileId>,
        folder_id: Option<rustshare_core::domain::FolderId>,
        recipient_user_id: rustshare_core::domain::UserId,
        permissions: rustshare_core::domain::SharePermissions,
        created_by: rustshare_core::domain::UserId,
        tenant_id: Uuid,
    ) -> anyhow::Result<rustshare_core::domain::Share> {
        self.create_user_share(
            file_id,
            folder_id,
            recipient_user_id,
            permissions,
            created_by,
            tenant_id,
        )
        .await
    }

    async fn update_share_permission(
        &self,
        share_id: rustshare_core::domain::ShareId,
        tenant_id: Uuid,
        new_permission: rustshare_core::domain::SharePermissions,
    ) -> anyhow::Result<rustshare_core::domain::Share> {
        self.update_share_permission(share_id, tenant_id, new_permission)
            .await
    }

    async fn get_by_id(
        &self,
        share_id: rustshare_core::domain::ShareId,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Share>> {
        self.get_by_id(share_id, tenant_id).await
    }

    async fn list_received_shares(
        &self,
        user_id: rustshare_core::domain::UserId,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Share>> {
        self.list_received_shares(user_id, tenant_id, limit, offset)
            .await
    }

    async fn list_share_recipients(
        &self,
        file_id: Option<rustshare_core::domain::FileId>,
        folder_id: Option<rustshare_core::domain::FolderId>,
        tenant_id: Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Share>> {
        self.list_share_recipients(file_id, folder_id, tenant_id)
            .await
    }

    async fn revoke_share(
        &self,
        share_id: rustshare_core::domain::ShareId,
        tenant_id: Uuid,
    ) -> anyhow::Result<()> {
        self.revoke_share(share_id, tenant_id).await
    }
}
