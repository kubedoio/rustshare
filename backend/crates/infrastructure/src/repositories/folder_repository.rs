use rustshare_core::domain::{Folder, FolderId};
use sqlx::PgPool;

/// Repository for folder database operations.
pub struct FolderRepository {
    /// The database pool (public for use by other repositories).
    pub pool: PgPool,
}

impl FolderRepository {
    /// Create a new FolderRepository with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get a folder by ID.
    pub async fn get_by_id(&self, folder_id: FolderId) -> anyhow::Result<Option<Folder>> {
        let row = sqlx::query!(
            r#"
            SELECT id, name, path, parent_folder_id, owner_id,
                   created_at, updated_at, starred_at, deleted_at, tenant_id
            FROM folders
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
            folder_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| Folder {
            id: row.id,
            name: row.name,
            path: row.path,
            parent_folder_id: row.parent_folder_id,
            owner_id: row.owner_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
            starred_at: row.starred_at,
            deleted_at: row.deleted_at,
            tenant_id: row.tenant_id,
            ancestor_ids: None,
        }))
    }
}

// Implement FolderOps trait for FolderRepository
impl rustshare_core::services::FolderOps for FolderRepository {
    async fn get_by_id(
        &self,
        folder_id: rustshare_core::domain::FolderId,
    ) -> anyhow::Result<Option<rustshare_core::domain::Folder>> {
        self.get_by_id(folder_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;
    use uuid::Uuid;

    const TEST_DATABASE_URL: &str = "postgres://rustshare:changeme@localhost:5432/rustshare";

    async fn test_pool() -> PgPool {
        let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());
        PgPool::connect(&url).await.expect("DB connect failed")
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn get_by_id_returns_folder_with_workspace_fields() {
        let pool = test_pool().await;
        let repo = FolderRepository::new(pool.clone());
        let test_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let folder_id = Uuid::new_v4();

        sqlx::query!(
            "INSERT INTO tenants (id, name) VALUES ($1, $2)",
            tenant_id,
            format!("test-tenant-{test_id}")
        )
        .execute(&pool)
        .await
        .expect("create tenant");

        sqlx::query!(
            r#"
            INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota, tenant_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            user_id,
            format!("user-{test_id}"),
            format!("user-{test_id}@example.com"),
            "hash",
            "Test User",
            false,
            1024_i64,
            tenant_id
        )
        .execute(&pool)
        .await
        .expect("create user");

        sqlx::query!(
            r#"
            INSERT INTO folders (
                id, name, path, owner_id, parent_folder_id, tenant_id, starred_at, deleted_at
            )
            VALUES ($1, $2, $3, $4, NULL, $5, NOW(), NULL)
            "#,
            folder_id,
            "A",
            "/A",
            user_id,
            tenant_id
        )
        .execute(&pool)
        .await
        .expect("create folder");

        let folder = repo
            .get_by_id(folder_id)
            .await
            .expect("query folder")
            .expect("folder exists");

        assert_eq!(folder.id, folder_id);
        assert_eq!(folder.owner_id, user_id);
        assert_eq!(folder.tenant_id, tenant_id);
        assert_eq!(folder.name, "A");
        assert!(folder.starred_at.is_some());
        assert_eq!(folder.deleted_at, None);
        assert_eq!(folder.ancestor_ids, None);

        sqlx::query!("DELETE FROM folders WHERE id = $1", folder_id)
            .execute(&pool)
            .await
            .ok();
        sqlx::query!("DELETE FROM users WHERE id = $1", user_id)
            .execute(&pool)
            .await
            .ok();
        sqlx::query!("DELETE FROM tenants WHERE id = $1", tenant_id)
            .execute(&pool)
            .await
            .ok();
    }
}
