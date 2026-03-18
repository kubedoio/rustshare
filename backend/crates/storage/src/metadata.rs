//! Metadata store for querying projection tables.
//!
//! NOTE: Currently uses runtime queries (`sqlx::query()`) instead of compile-time
//! queries (`sqlx::query!()`) because offline mode setup requires a running database.
//! This will be migrated to compile-time queries after Docker Compose is set up in Task 11.

use anyhow::Result;
use rustshare_core::domain::{File, FileVersion, Folder, Share, SharePermissions, User};
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

    /// Create a new file version in the projection table
    pub async fn create_file_version(&self, version: &FileVersion) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            INSERT INTO file_versions (id, file_id, version_number, content_hash, storage_key, size, created_by, created_at, change_description)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(version.id)
        .bind(version.file_id)
        .bind(version.version_number)
        .bind(&version.content_hash)
        .bind(version.storage_key())
        .bind(version.size)
        .bind(version.created_by)
        .bind(version.created_at)
        .bind(&version.change_description)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List all versions for a file, ordered by version number descending (newest first)
    pub async fn list_file_versions(&self, file_id: Uuid) -> Result<Vec<FileVersion>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
            r#"
            SELECT id, file_id, version_number, content_hash, size, created_by, created_at, change_description
            FROM file_versions
            WHERE file_id = $1
            ORDER BY version_number DESC
            "#,
        )
        .bind(file_id)
        .fetch_all(&self.pool)
        .await?;

        let mut versions = Vec::new();
        for row in rows {
            let version = FileVersion {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                version_number: row.try_get("version_number")?,
                content_hash: row.try_get("content_hash")?,
                size: row.try_get("size")?,
                created_by: row.try_get("created_by")?,
                created_at: row.try_get("created_at")?,
                change_description: row.try_get("change_description")?,
            };
            versions.push(version);
        }

        Ok(versions)
    }

    /// Find a specific version of a file
    pub async fn find_file_version(&self, file_id: Uuid, version: i32) -> Result<Option<FileVersion>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"
            SELECT id, file_id, version_number, content_hash, size, created_by, created_at, change_description
            FROM file_versions
            WHERE file_id = $1 AND version_number = $2
            "#,
        )
        .bind(file_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let version = FileVersion {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                version_number: row.try_get("version_number")?,
                content_hash: row.try_get("content_hash")?,
                size: row.try_get("size")?,
                created_by: row.try_get("created_by")?,
                created_at: row.try_get("created_at")?,
                change_description: row.try_get("change_description")?,
            };
            Ok(Some(version))
        } else {
            Ok(None)
        }
    }

    /// Create a new folder in the projection table
    pub async fn create_folder(&self, folder: &Folder) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            INSERT INTO folders (id, name, path, parent_folder_id, owner_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(folder.id)
        .bind(&folder.name)
        .bind(&folder.path)
        .bind(folder.parent_folder_id)
        .bind(folder.owner_id)
        .bind(folder.created_at)
        .bind(folder.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find folder by ID
    pub async fn find_folder_by_id(&self, id: Uuid) -> Result<Option<Folder>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at
            FROM folders
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let folder = Folder {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                owner_id: row.try_get("owner_id")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            };
            Ok(Some(folder))
        } else {
            Ok(None)
        }
    }

    /// Update a folder in the projection table
    pub async fn update_folder(&self, folder: &Folder) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            UPDATE folders
            SET name = $2, path = $3, parent_folder_id = $4, updated_at = $5
            WHERE id = $1
            "#,
        )
        .bind(folder.id)
        .bind(&folder.name)
        .bind(&folder.path)
        .bind(folder.parent_folder_id)
        .bind(folder.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a folder from the projection table
    pub async fn delete_folder(&self, id: Uuid) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// List folders with optional filters
    ///
    /// Returns folders owned by the specified user, optionally filtered by parent folder.
    /// Pass `None` for parent_id to get folders in the root directory (no parent).
    pub async fn list_folders(&self, parent_id: Option<Uuid>, owner_id: Uuid) -> Result<Vec<Folder>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at
            FROM folders
            WHERE owner_id = $1 AND (parent_folder_id = $2 OR ($2 IS NULL AND parent_folder_id IS NULL))
            ORDER BY name ASC
            "#,
        )
        .bind(owner_id)
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await?;

        let mut folders = Vec::new();
        for row in rows {
            let folder = Folder {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                owner_id: row.try_get("owner_id")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            };
            folders.push(folder);
        }

        Ok(folders)
    }

    /// Find all descendant folders of a given folder using recursive CTE
    ///
    /// Returns all folders in the subtree rooted at the specified folder,
    /// including the folder itself and all its direct and indirect children.
    pub async fn find_descendant_folders(&self, folder_id: Uuid) -> Result<Vec<Folder>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
            r#"
            WITH RECURSIVE folder_tree AS (
                -- Base case: start with the specified folder
                SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at
                FROM folders
                WHERE id = $1

                UNION ALL

                -- Recursive case: get all direct children
                SELECT f.id, f.name, f.path, f.parent_folder_id, f.owner_id, f.created_at, f.updated_at
                FROM folders f
                INNER JOIN folder_tree ft ON f.parent_folder_id = ft.id
            )
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at
            FROM folder_tree
            ORDER BY path ASC
            "#,
        )
        .bind(folder_id)
        .fetch_all(&self.pool)
        .await?;

        let mut folders = Vec::new();
        for row in rows {
            let folder = Folder {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                owner_id: row.try_get("owner_id")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            };
            folders.push(folder);
        }

        Ok(folders)
    }

    /// Create a new share link for a file
    pub async fn create_share(&self, share: &Share) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let permissions = match share.permissions {
            SharePermissions::Read => "read",
            SharePermissions::ReadWrite => "readwrite",
        };

        sqlx::query(
            r#"
            INSERT INTO shares (id, file_id, share_token, created_by, permissions, password_hash, expires_at, access_count, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(share.id)
        .bind(share.file_id)
        .bind(&share.share_token)
        .bind(share.created_by)
        .bind(permissions)
        .bind(&share.password_hash)
        .bind(share.expires_at)
        .bind(share.access_count)
        .bind(share.created_at)
        .bind(share.created_at) // updated_at = created_at for new shares
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find a share by its token
    pub async fn get_share_by_token(&self, token: &str) -> Result<Option<Share>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"
            SELECT id, file_id, share_token, created_by, permissions, password_hash, expires_at, access_count, created_at
            FROM shares
            WHERE share_token = $1
            "#,
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let permissions_str: String = row.try_get("permissions")?;
            let permissions = match permissions_str.as_str() {
                "readwrite" => SharePermissions::ReadWrite,
                _ => SharePermissions::Read,
            };

            let share = Share {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                share_token: row.try_get("share_token")?,
                created_by: row.try_get("created_by")?,
                permissions,
                password_hash: row.try_get("password_hash")?,
                expires_at: row.try_get("expires_at")?,
                access_count: row.try_get("access_count")?,
                created_at: row.try_get("created_at")?,
            };
            Ok(Some(share))
        } else {
            Ok(None)
        }
    }

    /// Find a share by ID
    pub async fn get_share(&self, share_id: Uuid) -> Result<Option<Share>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let row = sqlx::query(
            r#"
            SELECT id, file_id, share_token, created_by, permissions, password_hash, expires_at, access_count, created_at
            FROM shares
            WHERE id = $1
            "#,
        )
        .bind(share_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let permissions_str: String = row.try_get("permissions")?;
            let permissions = match permissions_str.as_str() {
                "readwrite" => SharePermissions::ReadWrite,
                _ => SharePermissions::Read,
            };

            let share = Share {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                share_token: row.try_get("share_token")?,
                created_by: row.try_get("created_by")?,
                permissions,
                password_hash: row.try_get("password_hash")?,
                expires_at: row.try_get("expires_at")?,
                access_count: row.try_get("access_count")?,
                created_at: row.try_get("created_at")?,
            };
            Ok(Some(share))
        } else {
            Ok(None)
        }
    }

    /// Get all active (non-revoked) shares for a file
    pub async fn get_file_shares(&self, file_id: Uuid) -> Result<Vec<Share>> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        let rows = sqlx::query(
            r#"
            SELECT id, file_id, share_token, created_by, permissions, password_hash, expires_at, access_count, created_at
            FROM shares
            WHERE file_id = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .bind(file_id)
        .fetch_all(&self.pool)
        .await?;

        let mut shares = Vec::new();
        for row in rows {
            let permissions_str: String = row.try_get("permissions")?;
            let permissions = match permissions_str.as_str() {
                "readwrite" => SharePermissions::ReadWrite,
                _ => SharePermissions::Read,
            };

            let share = Share {
                id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                share_token: row.try_get("share_token")?,
                created_by: row.try_get("created_by")?,
                permissions,
                password_hash: row.try_get("password_hash")?,
                expires_at: row.try_get("expires_at")?,
                access_count: row.try_get("access_count")?,
                created_at: row.try_get("created_at")?,
            };
            shares.push(share);
        }

        Ok(shares)
    }

    /// Update a share's password and expiration
    pub async fn update_share(&self, share: &Share) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            UPDATE shares
            SET password_hash = $2, expires_at = $3, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(share.id)
        .bind(&share.password_hash)
        .bind(share.expires_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Revoke a share link (soft delete)
    pub async fn revoke_share(&self, share_id: Uuid) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            UPDATE shares
            SET revoked_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(share_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Increment share access count and update last_accessed_at
    pub async fn increment_share_access(&self, share_id: Uuid) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            UPDATE shares
            SET access_count = access_count + 1, last_accessed_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(share_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Log a share access attempt
    pub async fn log_share_access(
        &self,
        share_id: Uuid,
        ip_address: Option<String>,
        user_agent: Option<String>,
        action: String,
        success: bool,
    ) -> Result<()> {
        // TODO: Switch to sqlx::query!() after Docker Compose setup (Task 11)
        sqlx::query(
            r#"
            INSERT INTO share_access_log (share_id, ip_address, user_agent, action, success)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(share_id)
        .bind(ip_address)
        .bind(user_agent)
        .bind(action)
        .bind(success)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::{File, FileVersion, Folder, Share, SharePermissions, User};

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

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_file_versions() {
        let (store, pool) = setup_metadata_store().await;

        // First create a user to own the file
        let user = User::new(
            "versionuser".to_string(),
            "Version User".to_string(),
            "hash789".to_string(),
            "versionuser@example.com".to_string(),
            false,
            10_737_418_240,
        );
        store.create_user(&user).await.unwrap();

        // Create a file
        let file = File::new(
            "versioned-doc.txt".to_string(),
            "/Documents/versioned-doc.txt".to_string(),
            "hash1".to_string(),
            100,
            "text/plain".to_string(),
            None,
            user.id,
        );
        store.create_file(&file).await.unwrap();

        // Create file version 1
        let version1 = FileVersion::new(
            file.id,
            1,
            "hash1".to_string(),
            100,
            user.id,
            Some("Initial version".to_string()),
        );
        store.create_file_version(&version1).await.unwrap();

        // Create file version 2
        let version2 = FileVersion::new(
            file.id,
            2,
            "hash2".to_string(),
            200,
            user.id,
            Some("Second version".to_string()),
        );
        store.create_file_version(&version2).await.unwrap();

        // Create file version 3
        let version3 = FileVersion::new(
            file.id,
            3,
            "hash3".to_string(),
            300,
            user.id,
            None,
        );
        store.create_file_version(&version3).await.unwrap();

        // Test: list_file_versions (should be in DESC order: 3, 2, 1)
        let versions = store.list_file_versions(file.id).await.unwrap();
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version_number, 3);
        assert_eq!(versions[1].version_number, 2);
        assert_eq!(versions[2].version_number, 1);
        assert_eq!(versions[0].content_hash, "hash3");
        assert_eq!(versions[1].content_hash, "hash2");
        assert_eq!(versions[2].content_hash, "hash1");

        // Test: find_file_version (find version 2)
        let found_version = store.find_file_version(file.id, 2).await.unwrap();
        assert!(found_version.is_some());
        let found = found_version.unwrap();
        assert_eq!(found.version_number, 2);
        assert_eq!(found.content_hash, "hash2");
        assert_eq!(found.size, 200);
        assert_eq!(found.created_by, user.id);
        assert_eq!(found.change_description, Some("Second version".to_string()));

        // Test: find_file_version (non-existent version)
        let not_found = store.find_file_version(file.id, 99).await.unwrap();
        assert!(not_found.is_none());

        // Cleanup (file_versions will cascade delete with file)
        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_folder_crud() {
        let (store, pool) = setup_metadata_store().await;

        // First create a user to own the folders
        let owner = User::new(
            "folderowner".to_string(),
            "Folder Owner".to_string(),
            "hashabc".to_string(),
            "folderowner@example.com".to_string(),
            false,
            10_737_418_240,
        );
        store.create_user(&owner).await.unwrap();

        // Test: create_folder (root folder)
        let root_folder = Folder::new_root(owner.id);
        store.create_folder(&root_folder).await.unwrap();

        // Test: find_folder_by_id
        let found = store.find_folder_by_id(root_folder.id).await.unwrap();
        assert!(found.is_some());
        let found_folder = found.unwrap();
        assert_eq!(found_folder.id, root_folder.id);
        assert_eq!(found_folder.name, "Root");
        assert_eq!(found_folder.path, "/");
        assert_eq!(found_folder.parent_folder_id, None);
        assert_eq!(found_folder.owner_id, owner.id);

        // Test: create_folder (child folder - Documents)
        let docs_folder = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            root_folder.id,
            owner.id,
        );
        store.create_folder(&docs_folder).await.unwrap();

        // Test: create_folder (child folder - Photos)
        let photos_folder = Folder::new_child(
            "Photos".to_string(),
            "/Photos".to_string(),
            root_folder.id,
            owner.id,
        );
        store.create_folder(&photos_folder).await.unwrap();

        // Test: create_folder (nested folder - Documents/Work)
        let work_folder = Folder::new_child(
            "Work".to_string(),
            "/Documents/Work".to_string(),
            docs_folder.id,
            owner.id,
        );
        store.create_folder(&work_folder).await.unwrap();

        // Test: create_folder (deeply nested folder - Documents/Work/Projects)
        let projects_folder = Folder::new_child(
            "Projects".to_string(),
            "/Documents/Work/Projects".to_string(),
            work_folder.id,
            owner.id,
        );
        store.create_folder(&projects_folder).await.unwrap();

        // Test: list_folders (root level - should return Documents and Photos)
        let root_children = store
            .list_folders(Some(root_folder.id), owner.id)
            .await
            .unwrap();
        assert_eq!(root_children.len(), 2);
        assert!(root_children.iter().any(|f| f.name == "Documents"));
        assert!(root_children.iter().any(|f| f.name == "Photos"));

        // Test: list_folders (Documents children - should return Work)
        let docs_children = store
            .list_folders(Some(docs_folder.id), owner.id)
            .await
            .unwrap();
        assert_eq!(docs_children.len(), 1);
        assert_eq!(docs_children[0].name, "Work");

        // Test: list_folders (no parent - should return root folder)
        let root_folders = store.list_folders(None, owner.id).await.unwrap();
        assert_eq!(root_folders.len(), 1);
        assert_eq!(root_folders[0].name, "Root");

        // Test: find_descendant_folders (should find all descendants of Documents)
        let descendants = store
            .find_descendant_folders(docs_folder.id)
            .await
            .unwrap();
        // Should include: Documents, Work, Projects (3 folders)
        assert_eq!(descendants.len(), 3);
        assert!(descendants.iter().any(|f| f.name == "Documents"));
        assert!(descendants.iter().any(|f| f.name == "Work"));
        assert!(descendants.iter().any(|f| f.name == "Projects"));

        // Test: find_descendant_folders (leaf folder should only return itself)
        let leaf_descendants = store
            .find_descendant_folders(projects_folder.id)
            .await
            .unwrap();
        assert_eq!(leaf_descendants.len(), 1);
        assert_eq!(leaf_descendants[0].name, "Projects");

        // Test: update_folder (rename Photos to Pictures)
        let mut updated_photos = photos_folder.clone();
        updated_photos.name = "Pictures".to_string();
        updated_photos.path = "/Pictures".to_string();
        updated_photos.updated_at = chrono::Utc::now();
        store.update_folder(&updated_photos).await.unwrap();

        let found_updated = store
            .find_folder_by_id(photos_folder.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_updated.name, "Pictures");
        assert_eq!(found_updated.path, "/Pictures");

        // Test: delete_folder (delete leaf folder first)
        store.delete_folder(projects_folder.id).await.unwrap();
        let not_found = store
            .find_folder_by_id(projects_folder.id)
            .await
            .unwrap();
        assert!(not_found.is_none());

        // Verify descendants updated after deletion
        let updated_descendants = store
            .find_descendant_folders(docs_folder.id)
            .await
            .unwrap();
        assert_eq!(updated_descendants.len(), 2); // Only Documents and Work remain
        assert!(!updated_descendants.iter().any(|f| f.name == "Projects"));

        // Cleanup: Delete folders (cascade will handle children)
        // Delete in order: leaf -> parent
        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(work_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(docs_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(photos_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(root_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_share_crud() {
        let (store, pool) = setup_metadata_store().await;

        // First create a user to own the file
        let owner = User::new(
            "shareowner".to_string(),
            "Share Owner".to_string(),
            "hashxyz".to_string(),
            "shareowner@example.com".to_string(),
            false,
            10_737_418_240,
        );
        store.create_user(&owner).await.unwrap();

        // Create a file to share
        let file = File::new(
            "shareable-document.pdf".to_string(),
            "/Documents/shareable-document.pdf".to_string(),
            "abcdef123456hash".to_string(),
            3072,
            "application/pdf".to_string(),
            None,
            owner.id,
        );
        store.create_file(&file).await.unwrap();

        // Test: create_share
        let share = Share::new(
            file.id,
            "sharetoken123".to_string(),
            owner.id,
            SharePermissions::Read,
            Some("hashed_password".to_string()),
            None,
        );
        store.create_share(&share).await.unwrap();

        // Test: get_share_by_token
        let found_by_token = store.get_share_by_token("sharetoken123").await.unwrap();
        assert!(found_by_token.is_some());
        let found_share = found_by_token.unwrap();
        assert_eq!(found_share.id, share.id);
        assert_eq!(found_share.share_token, "sharetoken123");
        assert_eq!(found_share.file_id, file.id);
        assert_eq!(found_share.permissions, SharePermissions::Read);
        assert_eq!(found_share.password_hash, Some("hashed_password".to_string()));
        assert_eq!(found_share.access_count, 0);

        // Test: get_share
        let found_by_id = store.get_share(share.id).await.unwrap();
        assert!(found_by_id.is_some());
        let found_share_by_id = found_by_id.unwrap();
        assert_eq!(found_share_by_id.id, share.id);
        assert_eq!(found_share_by_id.share_token, "sharetoken123");

        // Create a second share for the same file
        let share2 = Share::new(
            file.id,
            "sharetoken456".to_string(),
            owner.id,
            SharePermissions::ReadWrite,
            None,
            None,
        );
        store.create_share(&share2).await.unwrap();

        // Test: get_file_shares
        let file_shares = store.get_file_shares(file.id).await.unwrap();
        assert_eq!(file_shares.len(), 2);
        assert!(file_shares.iter().any(|s| s.share_token == "sharetoken123"));
        assert!(file_shares.iter().any(|s| s.share_token == "sharetoken456"));

        // Test: increment_share_access
        store.increment_share_access(share.id).await.unwrap();
        let updated = store.get_share(share.id).await.unwrap().unwrap();
        assert_eq!(updated.access_count, 1);

        // Test: log_share_access
        store
            .log_share_access(
                share.id,
                Some("192.168.1.1".to_string()),
                Some("Mozilla/5.0".to_string()),
                "access".to_string(),
                true,
            )
            .await
            .unwrap();

        // Test: update_share
        let mut updated_share = found_share.clone();
        updated_share.password_hash = Some("new_hashed_password".to_string());
        store.update_share(&updated_share).await.unwrap();

        let after_update = store.get_share(share.id).await.unwrap().unwrap();
        assert_eq!(
            after_update.password_hash,
            Some("new_hashed_password".to_string())
        );

        // Test: revoke_share
        store.revoke_share(share.id).await.unwrap();

        // After revoke, share should not appear in get_file_shares (only active shares)
        let active_shares = store.get_file_shares(file.id).await.unwrap();
        assert_eq!(active_shares.len(), 1);
        assert!(active_shares.iter().all(|s| s.share_token == "sharetoken456"));

        // But should still be retrievable by ID
        let revoked_share = store.get_share(share.id).await.unwrap();
        assert!(revoked_share.is_some());

        // Cleanup
        sqlx::query("DELETE FROM shares WHERE file_id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
