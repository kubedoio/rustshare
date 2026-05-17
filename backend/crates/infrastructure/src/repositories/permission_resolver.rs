//! Combined repository for permission resolver operations.
//!
//! This module provides a unified repository that implements the `PermissionResolverOps`
//! trait by combining share, file, folder, and user repositories.

use anyhow::Result;
use rustshare_core::domain::{File, FileId, Folder, FolderId, Share, SharePermissions, UserId};
use rustshare_core::services::PermissionResolverOps;
use sqlx::PgPool;
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
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE file_id IS NOT DISTINCT FROM $1::uuid
              AND folder_id IS NOT DISTINCT FROM $2::uuid
              AND recipient_group_id = ANY($3)
              AND revoked_at IS NULL
            "#,
            file_id,
            folder_id,
            group_ids
        )
        .fetch_all(&self.share_repo.pool)
        .await?;

        Ok(shares)
    }

    async fn find_user_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        recipient_user_id: UserId,
    ) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE folder_id = ANY($1)
              AND recipient_user_id = $2
              AND revoked_at IS NULL
            "#,
            folder_ids,
            recipient_user_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;

        Ok(shares)
    }

    async fn find_group_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        group_ids: &[Uuid],
    ) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE folder_id = ANY($1)
              AND recipient_group_id = ANY($2)
              AND revoked_at IS NULL
            "#,
            folder_ids,
            group_ids
        )
        .fetch_all(&self.share_repo.pool)
        .await?;

        Ok(shares)
    }

    async fn find_file_by_id(&self, id: FileId) -> Result<Option<File>> {
        self.file_repo.get_by_id(id).await
    }

    async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>> {
        self.folder_repo.get_by_id(id).await
    }

    async fn get_user_group_ids(&self, user_id: UserId) -> Result<Vec<Uuid>> {
        let group_ids = sqlx::query_scalar!(
            "SELECT group_id FROM group_members WHERE user_id = $1",
            user_id
        )
        .fetch_all(&self.user_repo.pool)
        .await?;

        Ok(group_ids)
    }
}
