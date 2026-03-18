use rustshare_core::domain::{User, UserId};
use sqlx::PgPool;

/// Repository for user database operations.
pub struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    /// Create a new UserRepository with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find user by email (case-insensitive).
    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        let email_lower = email.trim().to_lowercase();

        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, created_at, updated_at
            FROM users
            WHERE LOWER(email) = $1
            "#,
        )
        .bind(email_lower)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    /// Find a user by ID.
    pub async fn get_by_id(&self, user_id: UserId) -> Result<Option<User>, sqlx::Error> {
        let user = sqlx::query_as::<_, User>(
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, created_at, updated_at
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }
}

// Implement UserOps trait for UserRepository
impl rustshare_core::services::UserOps for UserRepository {
    async fn find_by_email(&self, email: &str) -> Result<Option<rustshare_core::domain::User>, sqlx::Error> {
        self.find_by_email(email).await
    }

    async fn get_by_id(&self, user_id: rustshare_core::domain::UserId) -> Result<Option<rustshare_core::domain::User>, sqlx::Error> {
        self.get_by_id(user_id).await
    }
}
