use rustshare_core::domain::{Theme, User, UserId};
use sqlx::PgPool;
use uuid::Uuid;

/// Repository for user database operations.
pub struct UserRepository {
    /// The database pool (public for use by other repositories).
    pub pool: PgPool,
}

impl UserRepository {
    /// Create a new UserRepository with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find user by email (case-insensitive).
    pub async fn find_by_email(&self, email: &str) -> anyhow::Result<Option<User>> {
        let email_lower = email.trim().to_lowercase();

        sqlx::query_as!(
            User,
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme as "theme: _", created_at, updated_at, disabled_at, name, surname,
                   avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config as "dashboard_config: _"
            FROM users
            WHERE LOWER(email) = $1
            "#,
            email_lower
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.into())
    }

    /// Find a user by ID.
    pub async fn get_by_id(&self, user_id: UserId) -> anyhow::Result<Option<User>> {
        sqlx::query_as!(
            User,
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme as "theme: _", created_at, updated_at, disabled_at, name, surname,
                   avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config as "dashboard_config: _"
            FROM users
            WHERE id = $1
            "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.into())
    }

    /// Update user's theme preference.
    pub async fn update_theme(&self, user_id: UserId, theme: Theme) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            UPDATE users
            SET theme = $1, updated_at = NOW()
            WHERE id = $2
            "#,
            theme.to_string(),
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get tenant ID for a user
    pub async fn get_tenant_id_for_user(&self, user_id: UserId) -> anyhow::Result<Option<Uuid>> {
        let row = sqlx::query!(r#"SELECT tenant_id FROM users WHERE id = $1"#, user_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| r.tenant_id))
    }

    /// List users by tenant ID.
    pub async fn list_by_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Vec<User>> {
        let rows = sqlx::query_as!(
            User,
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme as "theme: _", created_at, updated_at, disabled_at, 
                   trash_retention_days, tenant_id, name, surname, avatar_path,
                   email_sharing_enabled, dashboard_config as "dashboard_config: _"
            FROM users
            WHERE tenant_id = $1
            "#,
            tenant_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}

// Implement UserOps trait for UserRepository
impl rustshare_core::services::UserOps for UserRepository {
    async fn find_by_email(
        &self,
        email: &str,
    ) -> anyhow::Result<Option<rustshare_core::domain::User>> {
        self.find_by_email(email).await
    }

    async fn get_by_id(
        &self,
        user_id: rustshare_core::domain::UserId,
    ) -> anyhow::Result<Option<rustshare_core::domain::User>> {
        self.get_by_id(user_id).await
    }

    async fn get_tenant_id_for_user(
        &self,
        user_id: rustshare_core::domain::UserId,
    ) -> anyhow::Result<Option<uuid::Uuid>> {
        self.get_tenant_id_for_user(user_id).await
    }
}
