//! Metadata store for querying projection tables.
//!
//! NOTE: Currently uses runtime queries (`sqlx::query()`) instead of compile-time
//! queries (`sqlx::query!()`) because offline mode setup requires a running database.
//! This will be migrated to compile-time queries after Docker Compose is set up in Task 11.

use anyhow::Result;
use rustshare_core::domain::User;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::User;

    async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
        PgPool::connect(&database_url).await.unwrap()
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
}
