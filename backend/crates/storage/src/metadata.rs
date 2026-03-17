//! Metadata store for querying projection tables.
//!
//! NOTE: Currently uses runtime queries (`sqlx::query()`) instead of compile-time
//! queries (`sqlx::query!()`) because offline mode setup requires a running database.
//! This will be migrated to compile-time queries after Docker Compose is set up in Task 11.

use anyhow::Result;
use rustshare_core::domain::{File, User};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Metadata store for querying projection tables
pub struct MetadataStore {
    pool: PgPool,
}

impl MetadataStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new user in the projection table
    pub async fn create_user(&self, user: &User) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(user.id)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(&user.display_name)
        .bind(user.is_admin)
        .bind(user.storage_quota)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find user by email
    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<User>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"SELECT id, username, email, password_hash, display_name, is_admin, storage_quota, created_at, updated_at FROM users WHERE email = $1"#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let user = User {
                id: row.try_get("id")?,
                username: row.try_get("username")?,
                email: row.try_get("email")?,
                password_hash: row.try_get("password_hash")?,
                display_name: row.try_get("display_name")?,
                is_admin: row.try_get("is_admin")?,
                storage_quota: row.try_get("storage_quota")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            };
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }

    /// Find user by ID
    pub async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"SELECT id, username, email, password_hash, display_name, is_admin, storage_quota, created_at, updated_at FROM users WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let user = User {
                id: row.try_get("id")?,
                username: row.try_get("username")?,
                email: row.try_get("email")?,
                password_hash: row.try_get("password_hash")?,
                display_name: row.try_get("display_name")?,
                is_admin: row.try_get("is_admin")?,
                storage_quota: row.try_get("storage_quota")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            };
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }

    /// Check if any users exist (for admin bootstrapping)
    pub async fn has_users(&self) -> Result<bool> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query("SELECT COUNT(*) as count FROM users")
            .fetch_one(&self.pool)
            .await?;

        let count: i64 = row.try_get("count")?;
        Ok(count > 0)
    }

    /// Create a new file in the projection table
    pub async fn create_file(&self, file: &File) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id, parent_folder_id, current_version, created_at, modified_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(file.id)
        .bind(&file.name)
        .bind(&file.path)
        .bind(file.size)
        .bind(&file.mime_type)
        .bind(&file.content_hash)
        .bind(file.storage_key())
        .bind(file.owner_id)
        .bind(file.parent_folder_id)
        .bind(file.current_version)
        .bind(file.created_at)
        .bind(file.modified_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find file by ID
    pub async fn find_file_by_id(&self, id: Uuid) -> Result<Option<File>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at FROM files WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let file = File {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                size: row.try_get("size")?,
                mime_type: row.try_get("mime_type")?,
                content_hash: row.try_get("content_hash")?,
                owner_id: row.try_get("owner_id")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                current_version: row.try_get("current_version")?,
                created_at: row.try_get("created_at")?,
                modified_at: row.try_get("modified_at")?,
            };
            Ok(Some(file))
        } else {
            Ok(None)
        }
    }

    /// Update a file in the projection table
    pub async fn update_file(&self, file: &File) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            UPDATE files
            SET name = $2, path = $3, size = $4, mime_type = $5, content_hash = $6,
                storage_key = $7, parent_folder_id = $8, current_version = $9, modified_at = $10
            WHERE id = $1
            "#,
        )
        .bind(file.id)
        .bind(&file.name)
        .bind(&file.path)
        .bind(file.size)
        .bind(&file.mime_type)
        .bind(&file.content_hash)
        .bind(file.storage_key())
        .bind(file.parent_folder_id)
        .bind(file.current_version)
        .bind(file.modified_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a file from the projection table
    pub async fn delete_file(&self, id: Uuid) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// List files with optional filters
    ///
    /// Returns files owned by the specified user, optionally filtered by parent folder.
    /// Pass `None` for parent_id to get files in the root directory (no parent).
    pub async fn list_files(&self, parent_id: Option<Uuid>, owner_id: Uuid) -> Result<Vec<File>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
            r#"
            SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at
            FROM files
            WHERE owner_id = $1 AND (parent_folder_id = $2 OR ($2 IS NULL AND parent_folder_id IS NULL))
            ORDER BY name ASC
            "#,
        )
        .bind(owner_id)
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut files = Vec::new();
        for row in rows {
            let file = File {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                size: row.try_get("size")?,
                mime_type: row.try_get("mime_type")?,
                content_hash: row.try_get("content_hash")?,
                owner_id: row.try_get("owner_id")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                current_version: row.try_get("current_version")?,
                created_at: row.try_get("created_at")?,
                modified_at: row.try_get("modified_at")?,
            };
            files.push(file);
        }

        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::{File, User};

    async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://rustshare:changeme@localhost:5432/rustshare".to_string()
        });
        PgPool::connect(&database_url).await.unwrap()
    }

    async fn setup_metadata_store() -> (MetadataStore, PgPool) {
        let pool = setup_test_db().await;
        let store = MetadataStore::new(pool.clone());
        (store, pool)
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_create_and_find_user() {
        let pool = setup_test_db().await;
        let store = MetadataStore::new(pool.clone());

        let user = User::new(
            "testuser".to_string(),
            "Test User".to_string(),
            "hash123".to_string(),
            "test@example.com".to_string(),
            false,
            10_737_418_240, // 10GB
        );

        store.create_user(&user).await.unwrap();

        let found = store.find_user_by_email("test@example.com").await.unwrap();
        assert!(found.is_some());
        let found_user = found.unwrap();
        assert_eq!(found_user.email, "test@example.com");
        assert_eq!(found_user.username, "testuser");

        // Cleanup
        sqlx::query("DELETE FROM users WHERE email = $1")
            .bind("test@example.com")
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_file_crud() {
        let (store, pool) = setup_metadata_store().await;

        // First create a user to own the file
        let owner = User::new(
            "fileowner".to_string(),
            "File Owner".to_string(),
            "hash456".to_string(),
            "fileowner@example.com".to_string(),
            false,
            10_737_418_240,
        );
        store.create_user(&owner).await.unwrap();

        // Create a file
        let file = File::new(
            "test-document.pdf".to_string(),
            "/Documents/test-document.pdf".to_string(),
            "abc123def456hash".to_string(),
            2048,
            "application/pdf".to_string(),
            None, // No parent folder
            owner.id,
        );

        // Test: create_file
        store.create_file(&file).await.unwrap();

        // Test: find_file_by_id
        let found = store.find_file_by_id(file.id).await.unwrap();
        assert!(found.is_some());
        let found_file = found.unwrap();
        assert_eq!(found_file.id, file.id);
        assert_eq!(found_file.name, "test-document.pdf");
        assert_eq!(found_file.path, "/Documents/test-document.pdf");
        assert_eq!(found_file.content_hash, "abc123def456hash");
        assert_eq!(found_file.size, 2048);
        assert_eq!(found_file.mime_type, "application/pdf");
        assert_eq!(found_file.owner_id, owner.id);
        assert_eq!(found_file.current_version, 1);

        // Test: update_file (modify name and size)
        let mut updated_file = found_file.clone();
        updated_file.name = "renamed-document.pdf".to_string();
        updated_file.size = 4096;
        store.update_file(&updated_file).await.unwrap();

        let found_updated = store.find_file_by_id(file.id).await.unwrap().unwrap();
        assert_eq!(found_updated.name, "renamed-document.pdf");
        assert_eq!(found_updated.size, 4096);

        // Test: list_files (with no parent_id filter)
        let files = store.list_files(None, owner.id).await.unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.id == file.id));

        // Test: delete_file
        store.delete_file(file.id).await.unwrap();
        let not_found = store.find_file_by_id(file.id).await.unwrap();
        assert!(not_found.is_none());

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
