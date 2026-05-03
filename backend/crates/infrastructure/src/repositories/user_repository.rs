use rustshare_core::domain::{DashboardConfig, Theme, User, UserId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

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
            trash_retention_days: row.try_get("trash_retention_days")?,
            tenant_id: row.try_get("tenant_id")?,
            dashboard_config: row
                .try_get::<Option<sqlx::types::Json<DashboardConfig>>, _>("dashboard_config")
                .unwrap_or_else(|_| Some(sqlx::types::Json(DashboardConfig::default())))
                .unwrap_or_else(|| sqlx::types::Json(DashboardConfig::default())),
        })
    }

    /// Create a new UserRepository with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find user by email (case-insensitive).
    pub async fn find_by_email(&self, email: &str) -> anyhow::Result<Option<User>> {
        let email_lower = email.trim().to_lowercase();

        let row = sqlx::query(
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme, created_at, updated_at, disabled_at, name, surname,
                   avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config
            FROM users
            WHERE LOWER(email) = $1
            "#,
        )
        .bind(email_lower)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::map_user_row)
            .transpose()
            .map_err(|e| e.into())
    }

    /// Find a user by ID.
    pub async fn get_by_id(&self, user_id: UserId) -> anyhow::Result<Option<User>> {
        let row = sqlx::query(
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme, created_at, updated_at, disabled_at, name, surname,
                   avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(Self::map_user_row)
            .transpose()
            .map_err(|e| e.into())
    }

    /// Update user's theme preference.
    pub async fn update_theme(&self, user_id: UserId, theme: Theme) -> anyhow::Result<()> {
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

    /// Get tenant ID for a user
    pub async fn get_tenant_id_for_user(&self, user_id: UserId) -> anyhow::Result<Option<Uuid>> {
        let row = sqlx::query(r#"SELECT tenant_id FROM users WHERE id = $1"#)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(r) => Ok(r.try_get("tenant_id")?),
            None => Ok(None),
        }
    }

    /// List users by tenant ID.
    pub async fn list_by_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Vec<User>> {
        let rows = sqlx::query(
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme, created_at, updated_at, disabled_at, 
                   trash_retention_days, tenant_id, name, surname, avatar_path,
                   email_sharing_enabled, dashboard_config
            FROM users
            WHERE tenant_id = $1
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        let mut users = Vec::with_capacity(rows.len());
        for row in rows {
            users.push(Self::map_user_row(row)?);
        }
        Ok(users)
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
