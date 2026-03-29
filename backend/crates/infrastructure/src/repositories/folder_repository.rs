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
    pub async fn get_by_id(&self, folder_id: FolderId) -> Result<Option<Folder>, sqlx::Error> {
        let folder = sqlx::query_as::<_, Folder>(
            r#"
            SELECT id, name, path, parent_folder_id, owner_id,
                   created_at, updated_at, tenant_id
            FROM folders
            WHERE id = $1
            "#,
        )
        .bind(folder_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(folder)
    }
}

// Implement FolderOps trait for FolderRepository
impl rustshare_core::services::FolderOps for FolderRepository {
    async fn get_by_id(
        &self,
        folder_id: rustshare_core::domain::FolderId,
    ) -> Result<Option<rustshare_core::domain::Folder>, sqlx::Error> {
        self.get_by_id(folder_id).await
    }
}
