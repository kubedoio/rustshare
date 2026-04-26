//! Combined repository for permission resolver operations.
//!
//! This module provides a unified repository that implements the `PermissionResolverOps`
//! trait by combining share, file, folder, and user repositories.

use anyhow::Result;
use rustshare_core::domain::{File, FileId, Folder, FolderId, Share, UserId};
use rustshare_core::services::PermissionResolverOps;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::{FileRepository, FolderRepository, ShareRepository, UserRepository};

/// Combined repository for permission resolver operations.
///
/// This struct wraps the individual repositories needed for permission resolution
/// and implements the `PermissionResolverOps` trait.
pub struct PermissionResolverRepository {
    share_repo: ShareRepository,
    file_repo: FileRepository,
    folder_repo: FolderRepository,
    user_repo: UserRepository,
}

impl PermissionResolverRepository {
    /// Create a new PermissionResolverRepository with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self {
            share_repo: ShareRepository::new(pool.clone()),
            file_repo: FileRepository::new(pool.clone()),
            folder_repo: FolderRepository::new(pool.clone()),
            user_repo: UserRepository::new(pool),
        }
    }

    /// Create a new PermissionResolverRepository from individual repositories.
    pub fn from_repositories(
        share_repo: ShareRepository,
        file_repo: FileRepository,
        folder_repo: FolderRepository,
        user_repo: UserRepository,
    ) -> Self {
        Self {
            share_repo,
            file_repo,
            folder_repo,
            user_repo,
        }
    }
}

impl PermissionResolverOps for PermissionResolverRepository {
    async fn find_user_share(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        recipient_user_id: UserId,
    ) -> Result<Option<Share>> {
        self.share_repo
            .find_user_share(file_id, folder_id, recipient_user_id)
            .await
    }

    async fn find_group_shares(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        group_ids: &[Uuid],
    ) -> Result<Vec<Share>> {
        // For now, query shares and filter by group IDs in memory
        // This can be optimized with a more specific query if needed
        let rows = sqlx::query(
            r#"
            SELECT id, file_id, folder_id, share_token, permissions, password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE file_id IS NOT DISTINCT FROM $1::uuid
              AND folder_id IS NOT DISTINCT FROM $2::uuid
              AND recipient_group_id = ANY($3)
              AND revoked_at IS NULL
            "#,
        )
        .bind(file_id)
        .bind(folder_id)
        .bind(group_ids)
        .fetch_all(&self.share_repo.pool)
        .await?;

        rows.into_iter()
            .map(|row| map_share_row(row))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn find_user_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        recipient_user_id: UserId,
    ) -> Result<Vec<Share>> {
        let rows = sqlx::query(
            r#"
            SELECT id, file_id, folder_id, share_token, permissions, password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE folder_id = ANY($1)
              AND recipient_user_id = $2
              AND revoked_at IS NULL
            "#,
        )
        .bind(folder_ids)
        .bind(recipient_user_id)
        .fetch_all(&self.share_repo.pool)
        .await?;

        rows.into_iter()
            .map(|row| map_share_row(row))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn find_group_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        group_ids: &[Uuid],
    ) -> Result<Vec<Share>> {
        let rows = sqlx::query(
            r#"
            SELECT id, file_id, folder_id, share_token, permissions, password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE folder_id = ANY($1)
              AND recipient_group_id = ANY($2)
              AND revoked_at IS NULL
            "#,
        )
        .bind(folder_ids)
        .bind(group_ids)
        .fetch_all(&self.share_repo.pool)
        .await?;

        rows.into_iter()
            .map(|row| map_share_row(row))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn find_file_by_id(&self, id: FileId) -> Result<Option<File>> {
        self.file_repo.get_by_id(id).await
    }

    async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>> {
        self.folder_repo.get_by_id(id).await
    }

    async fn get_user_group_ids(&self, user_id: UserId) -> Result<Vec<Uuid>> {
        let rows = sqlx::query(
            r#"
            SELECT group_id FROM group_members WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.user_repo.pool)
        .await?;

        let group_ids: Vec<Uuid> = rows
            .into_iter()
            .map(|row| row.try_get("group_id"))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e: sqlx::Error| anyhow::anyhow!(e))?;

        Ok(group_ids)
    }
}

/// Helper function to map a database row to a Share.
fn map_share_row(row: sqlx::postgres::PgRow) -> Result<Share, sqlx::Error> {
    use rustshare_core::domain::SharePermissions;
    use sqlx::Row;

    let permissions_str: String = row.try_get("permissions")?;
    let permissions = match permissions_str.as_str() {
        "Edit" | "edit" => SharePermissions::Edit,
        "Admin" | "admin" => SharePermissions::Admin,
        _ => SharePermissions::View,
    };

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
