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
    pub async fn get_by_id(&self, file_id: FileId) -> Result<Option<File>, sqlx::Error> {
        let file = sqlx::query_as::<_, File>(
            r#"
            SELECT id, name, path, content_hash, size, mime_type,
                   parent_folder_id, owner_id, current_version,
                   created_at, modified_at, tenant_id
            FROM files
            WHERE id = $1
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
    ) -> Result<Option<rustshare_core::domain::File>, sqlx::Error> {
        self.get_by_id(file_id).await
    }
}
