use rustshare_core::domain::{File, FileId};
use sqlx::PgPool;

/// Repository for file database operations.
pub struct FileRepository {
    /// The database pool (public for use by other repositories).
    pub pool: PgPool,
}

impl FileRepository {
    /// Create a new FileRepository with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get a file by ID.
    pub async fn get_by_id(&self, file_id: FileId) -> anyhow::Result<Option<File>> {
        let file = sqlx::query_as::<_, File>(
            r#"
            SELECT id, name, path, content_hash, size, mime_type,
                   parent_folder_id, owner_id, current_version,
                   created_at, modified_at, starred_at, deleted_at, tenant_id
            FROM files
            WHERE id = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(file_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(file)
    }
}

// Implement FileOps trait for FileRepository
impl rustshare_core::services::FileOps for FileRepository {
    async fn get_by_id(
        &self,
        file_id: rustshare_core::domain::FileId,
    ) -> anyhow::Result<Option<rustshare_core::domain::File>> {
        self.get_by_id(file_id).await
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
    async fn get_by_id_returns_full_file_row() {
        let pool = test_pool().await;
        let repo = FileRepository::new(pool.clone());
        let test_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(tenant_id)
            .bind(format!("test-tenant-{test_id}"))
            .execute(&pool)
            .await
            .expect("create tenant");

        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota, tenant_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(user_id)
        .bind(format!("user-{test_id}"))
        .bind(format!("user-{test_id}@example.com"))
        .bind("hash")
        .bind("Test User")
        .bind(false)
        .bind(1024_i64)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("create user");

        sqlx::query(
            r#"
            INSERT INTO files (
                id, name, path, size, mime_type, content_hash, storage_key,
                owner_id, parent_folder_id, current_version, tenant_id, starred_at, deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, $9, $10, NOW(), NULL)
            "#,
        )
        .bind(file_id)
        .bind("preview.png")
        .bind("/preview.png")
        .bind(123_i64)
        .bind("image/png")
        .bind("abc123")
        .bind("blobs/abc123")
        .bind(user_id)
        .bind(1_i32)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("create file");

        let file = repo
            .get_by_id(file_id)
            .await
            .expect("query file")
            .expect("file exists");

        assert_eq!(file.id, file_id);
        assert_eq!(file.owner_id, user_id);
        assert_eq!(file.tenant_id, tenant_id);
        assert_eq!(file.name, "preview.png");
        assert!(file.starred_at.is_some());
        assert_eq!(file.deleted_at, None);

        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(file_id)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .execute(&pool)
            .await
            .ok();
    }
}
