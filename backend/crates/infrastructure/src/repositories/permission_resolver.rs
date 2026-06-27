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
        tenant_id: Uuid,
    ) -> Result<Option<Share>> {
        self.share_repo
            .find_user_share(file_id, folder_id, recipient_user_id, tenant_id)
            .await
    }

    async fn find_group_shares(
        &self,
        file_id: Option<FileId>,
        folder_id: Option<FolderId>,
        group_ids: &[Uuid],
        tenant_id: Uuid,
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
              AND tenant_id = $4
              AND revoked_at IS NULL
            "#,
            file_id,
            folder_id,
            group_ids,
            tenant_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;

        Ok(shares)
    }

    async fn find_user_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        recipient_user_id: UserId,
        tenant_id: Uuid,
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
              AND tenant_id = $3
              AND revoked_at IS NULL
            "#,
            folder_ids,
            recipient_user_id,
            tenant_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;

        Ok(shares)
    }

    async fn find_group_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        group_ids: &[Uuid],
        tenant_id: Uuid,
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
              AND tenant_id = $3
              AND revoked_at IS NULL
            "#,
            folder_ids,
            group_ids,
            tenant_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;

        Ok(shares)
    }

    async fn find_file_by_id(&self, id: FileId, tenant_id: Uuid) -> Result<Option<File>> {
        self.file_repo.get_by_id(id, tenant_id).await
    }

    async fn find_folder_by_id(&self, id: FolderId, tenant_id: Uuid) -> Result<Option<Folder>> {
        self.folder_repo.get_by_id(id, tenant_id).await
    }

    async fn get_user_group_ids(&self, user_id: UserId, tenant_id: Uuid) -> Result<Vec<Uuid>> {
        let group_ids = sqlx::query_scalar!(
            r#"
            SELECT gm.group_id
            FROM group_members gm
            JOIN users u ON u.id = gm.user_id
            WHERE gm.user_id = $1 AND u.tenant_id = $2
            "#,
            user_id,
            tenant_id
        )
        .fetch_all(&self.user_repo.pool)
        .await?;

        Ok(group_ids)
    }

    async fn find_all_user_shares_for_file(
        &self,
        file_id: FileId,
        tenant_id: Uuid,
    ) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE file_id = $1
              AND folder_id IS NULL
              AND recipient_user_id IS NOT NULL
              AND tenant_id = $2
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
            file_id,
            tenant_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;
        Ok(shares)
    }

    async fn find_all_group_shares_for_file(
        &self,
        file_id: FileId,
        tenant_id: Uuid,
    ) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE file_id = $1
              AND folder_id IS NULL
              AND recipient_group_id IS NOT NULL
              AND tenant_id = $2
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
            file_id,
            tenant_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;
        Ok(shares)
    }

    async fn find_all_user_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        tenant_id: Uuid,
    ) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE folder_id = ANY($1)
              AND recipient_user_id IS NOT NULL
              AND tenant_id = $2
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
            folder_ids,
            tenant_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;
        Ok(shares)
    }

    async fn find_all_group_shares_for_folders(
        &self,
        folder_ids: &[FolderId],
        tenant_id: Uuid,
    ) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, permissions as "permissions: SharePermissions", password_hash,
                   expires_at, upload_only, access_count, recipient_user_id, recipient_group_id,
                   created_by, created_at, revoked_at, tenant_id
            FROM shares
            WHERE folder_id = ANY($1)
              AND recipient_group_id IS NOT NULL
              AND tenant_id = $2
              AND revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
            folder_ids,
            tenant_id
        )
        .fetch_all(&self.share_repo.pool)
        .await?;
        Ok(shares)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::services::PermissionResolverOps;
    use sqlx::PgPool;
    use uuid::Uuid;

    const TEST_DATABASE_URL: &str = "postgres://rustshare:changeme@localhost:5432/rustshare";

    async fn test_pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());
        PgPool::connect(&url).await.expect("DB connect failed")
    }

    #[test]
    fn trait_is_implemented() {
        fn assert_ops<T: PermissionResolverOps>() {}
        assert_ops::<PermissionResolverRepository>();
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn repository_queries_are_scoped_by_tenant() {
        let pool = test_pool().await;
        let repo = PermissionResolverRepository::new(pool.clone());
        let suffix = Uuid::new_v4();

        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(tenant_a)
            .bind(format!("tenant-a-{suffix}"))
            .execute(&pool)
            .await
            .expect("create tenant a");

        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(tenant_b)
            .bind(format!("tenant-b-{suffix}"))
            .execute(&pool)
            .await
            .expect("create tenant b");

        let owner_a = Uuid::new_v4();
        let recipient_a = Uuid::new_v4();
        let group_a = Uuid::new_v4();

        for (user_id, username, tenant_id) in [
            (owner_a, format!("owner-a-{suffix}"), tenant_a),
            (recipient_a, format!("recipient-a-{suffix}"), tenant_a),
        ] {
            sqlx::query(
                r#"
                INSERT INTO users (
                    id, username, email, password_hash, display_name, is_admin, storage_quota, tenant_id
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(user_id)
            .bind(&username)
            .bind(format!("{username}@example.com"))
            .bind("hash")
            .bind("Test User")
            .bind(false)
            .bind(1024_i64)
            .bind(tenant_id)
            .execute(&pool)
            .await
            .expect("create user");
        }

        sqlx::query(
            r#"
            INSERT INTO user_groups (id, name, created_by, tenant_id)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(group_a)
        .bind(format!("group-a-{suffix}"))
        .bind(owner_a)
        .bind(tenant_a)
        .execute(&pool)
        .await
        .expect("create group");

        sqlx::query(
            r#"
            INSERT INTO group_members (id, group_id, user_id, added_by)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(group_a)
        .bind(recipient_a)
        .bind(owner_a)
        .execute(&pool)
        .await
        .expect("create group member");

        let folder_a = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO folders (id, name, path, owner_id, parent_folder_id, tenant_id, starred_at, deleted_at)
            VALUES ($1, $2, $3, $4, NULL, $5, NULL, NULL)
            "#,
        )
        .bind(folder_a)
        .bind(format!("folder-a-{suffix}"))
        .bind(format!("/folder-a-{suffix}"))
        .bind(owner_a)
        .bind(tenant_a)
        .execute(&pool)
        .await
        .expect("create folder");

        let file_a = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO files (
                id, name, path, size, mime_type, content_hash, storage_key,
                owner_id, parent_folder_id, current_version, tenant_id, starred_at, deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, $9, $10, NULL, NULL)
            "#,
        )
        .bind(file_a)
        .bind(format!("file-a-{suffix}"))
        .bind(format!("/file-a-{suffix}"))
        .bind(123_i64)
        .bind("text/plain")
        .bind("abc123")
        .bind(format!("blobs/abc123-{suffix}"))
        .bind(owner_a)
        .bind(1_i32)
        .bind(tenant_a)
        .execute(&pool)
        .await
        .expect("create file");

        let user_share_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO shares (
                id, file_id, folder_id, share_token, permissions, password_hash,
                expires_at, access_count, recipient_user_id, recipient_group_id,
                created_by, created_at, revoked_at, upload_only, tenant_id
            )
            VALUES ($1, $2, NULL, NULL, $3, NULL, NULL, 0, $4, NULL, $5, NOW(), NULL, FALSE, $6)
            "#,
        )
        .bind(user_share_id)
        .bind(file_a)
        .bind("View")
        .bind(recipient_a)
        .bind(owner_a)
        .bind(tenant_a)
        .execute(&pool)
        .await
        .expect("create user share");

        let group_share_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO shares (
                id, file_id, folder_id, share_token, permissions, password_hash,
                expires_at, access_count, recipient_user_id, recipient_group_id,
                created_by, created_at, revoked_at, upload_only, tenant_id
            )
            VALUES ($1, NULL, $2, NULL, $3, NULL, NULL, 0, NULL, $4, $5, NOW(), NULL, FALSE, $6)
            "#,
        )
        .bind(group_share_id)
        .bind(folder_a)
        .bind("Edit")
        .bind(group_a)
        .bind(owner_a)
        .bind(tenant_a)
        .execute(&pool)
        .await
        .expect("create group share");

        // Correct tenant returns the resources.
        assert_eq!(
            repo.find_file_by_id(file_a, tenant_a)
                .await
                .unwrap()
                .map(|f| f.id),
            Some(file_a)
        );
        assert_eq!(
            repo.find_folder_by_id(folder_a, tenant_a)
                .await
                .unwrap()
                .map(|f| f.id),
            Some(folder_a)
        );
        assert!(repo
            .find_user_share(Some(file_a), None, recipient_a, tenant_a)
            .await
            .unwrap()
            .is_some());
        assert_eq!(
            repo.find_group_shares(Some(file_a), None, &[group_a], tenant_a)
                .await
                .unwrap()
                .len(),
            0 // user share, not group share
        );
        assert_eq!(
            repo.find_user_shares_for_folders(&[folder_a], recipient_a, tenant_a)
                .await
                .unwrap()
                .len(),
            0 // group share on folder, not user share
        );
        assert_eq!(
            repo.find_group_shares_for_folders(&[folder_a], &[group_a], tenant_a)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            repo.get_user_group_ids(recipient_a, tenant_a)
                .await
                .unwrap(),
            vec![group_a]
        );

        // Wrong tenant must not see any data.
        assert!(
            repo.find_file_by_id(file_a, tenant_b)
                .await
                .unwrap()
                .is_none(),
            "file query leaked across tenants"
        );
        assert!(
            repo.find_folder_by_id(folder_a, tenant_b)
                .await
                .unwrap()
                .is_none(),
            "folder query leaked across tenants"
        );
        assert!(
            repo.find_user_share(Some(file_a), None, recipient_a, tenant_b)
                .await
                .unwrap()
                .is_none(),
            "user share query leaked across tenants"
        );
        assert!(
            repo.find_group_shares(Some(file_a), None, &[group_a], tenant_b)
                .await
                .unwrap()
                .is_empty(),
            "group share query leaked across tenants"
        );
        assert!(
            repo.find_user_shares_for_folders(&[folder_a], recipient_a, tenant_b)
                .await
                .unwrap()
                .is_empty(),
            "user folder share query leaked across tenants"
        );
        assert!(
            repo.find_group_shares_for_folders(&[folder_a], &[group_a], tenant_b)
                .await
                .unwrap()
                .is_empty(),
            "group folder share query leaked across tenants"
        );
        assert!(
            repo.get_user_group_ids(recipient_a, tenant_b)
                .await
                .unwrap()
                .is_empty(),
            "group membership query leaked across tenants"
        );

        // Cleanup
        sqlx::query("DELETE FROM shares WHERE id = $1 OR id = $2")
            .bind(user_share_id)
            .bind(group_share_id)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(file_a)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(folder_a)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM group_members WHERE group_id = $1")
            .bind(group_a)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM user_groups WHERE id = $1")
            .bind(group_a)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
            .bind(owner_a)
            .bind(recipient_a)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM tenants WHERE id = $1 OR id = $2")
            .bind(tenant_a)
            .bind(tenant_b)
            .execute(&pool)
            .await
            .ok();
    }
}
