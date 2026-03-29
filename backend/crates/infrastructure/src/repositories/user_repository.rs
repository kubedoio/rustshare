use rustshare_core::domain::{Theme, User, UserId};
use sqlx::{PgPool, Row};

/// Repository for user database operations.
pub struct UserRepository {
    /// The database pool (public for use by other repositories).
    pub pool: PgPool,
}

impl UserRepository {
    fn map_user_row(row: sqlx::postgres::PgRow) -> Result<User, sqlx::Error> {
        Ok(User {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            display_name: row.try_get("display_name")?,
            password_hash: row.try_get("password_hash")?,
            email: row.try_get("email")?,
            is_admin: row.try_get("is_admin")?,
            storage_quota: row.try_get("storage_quota")?,
            theme: row
                .try_get::<String, _>("theme")?
                .parse()
                .unwrap_or_default(),
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            disabled_at: row.try_get("disabled_at")?,
            name: row.try_get("name")?,
            surname: row.try_get("surname")?,
            avatar_path: row.try_get("avatar_path")?,
            email_sharing_enabled: row.try_get("email_sharing_enabled")?,
            tenant_id: row.try_get("tenant_id")?,
        })
    }

    /// Create a new UserRepository with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find user by email (case-insensitive).
    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        let email_lower = email.trim().to_lowercase();

        let row = sqlx::query(
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme, created_at, updated_at, disabled_at, tenant_id
            FROM users
            WHERE LOWER(email) = $1
            "#,
        )
        .bind(email_lower)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::map_user_row).transpose()
    }

    /// Find a user by ID.
    pub async fn get_by_id(&self, user_id: UserId) -> Result<Option<User>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme, created_at, updated_at, disabled_at, tenant_id
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::map_user_row).transpose()
    }

    /// Update user's theme preference.
    pub async fn update_theme(&self, user_id: UserId, theme: Theme) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE users
            SET theme = $1, updated_at = NOW()
            WHERE id = $2
            "#,
        )
        .bind(theme.to_string())
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

// Implement UserOps trait for UserRepository
impl rustshare_core::services::UserOps for UserRepository {
    async fn find_by_email(
        &self,
        email: &str,
    ) -> Result<Option<rustshare_core::domain::User>, sqlx::Error> {
        self.find_by_email(email).await
    }

    async fn get_by_id(
        &self,
        user_id: rustshare_core::domain::UserId,
    ) -> Result<Option<rustshare_core::domain::User>, sqlx::Error> {
        self.get_by_id(user_id).await
    }
}
